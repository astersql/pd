// Copyright 2026 AsterSQL.
//! Pinned kvproto conversion for encryption dictionaries, outside the domain crate.
use astersql_pd_encryption::{
    DataKey, EncryptionMethod, KeyDictionary, KeyDictionaryCodec, KeyEnvelope, MasterKeyMeta,
};
use astersql_pd_errs::{ErrorTemplate, TypedError, catalog::ERR_PROTO_UNMARSHAL};
use astersql_pd_proto::encryptionpb as pb;
use prost::Message;
#[derive(Default)]
pub struct ProtoKeyDictionaryCodec;
fn meta_to_proto(meta: &MasterKeyMeta) -> pb::MasterKey {
    use pb::master_key::Backend;
    pb::MasterKey {
        backend: Some(match meta {
            MasterKeyMeta::Plaintext => Backend::Plaintext(pb::MasterKeyPlaintext {}),
            MasterKeyMeta::File { path } => Backend::File(pb::MasterKeyFile { path: path.clone() }),
            MasterKeyMeta::Kms {
                vendor,
                key_id,
                region,
                endpoint,
            } => Backend::Kms(pb::MasterKeyKms {
                vendor: vendor.clone(),
                key_id: key_id.clone(),
                region: region.clone(),
                endpoint: endpoint.clone(),
                ..Default::default()
            }),
        }),
    }
}
impl KeyDictionaryCodec for ProtoKeyDictionaryCodec {
    fn encode_keys(&self, keys: &KeyDictionary) -> Result<Vec<u8>, TypedError> {
        Ok(pb::KeyDictionary {
            current_key_id: keys.current_key_id,
            keys: keys
                .keys
                .iter()
                .map(|(id, key)| {
                    (
                        *id,
                        pb::DataKey {
                            key: key.key.clone(),
                            method: key.method.value(),
                            creation_time: key.creation_time,
                            was_exposed: key.was_exposed,
                        },
                    )
                })
                .collect(),
        }
        .encode_to_vec())
    }
    fn decode_keys(&self, bytes: &[u8]) -> Result<KeyDictionary, TypedError> {
        let keys = pb::KeyDictionary::decode(bytes).map_err(|e| {
            ErrorTemplate::new(
                ERR_PROTO_UNMARSHAL.code(),
                "fail to unmarshal encryption keys",
            )
            .with_source([] as [String; 0], e)
        })?;
        Ok(KeyDictionary {
            current_key_id: keys.current_key_id,
            keys: keys
                .keys
                .into_iter()
                .map(|(id, key)| {
                    (
                        id,
                        DataKey {
                            key: key.key,
                            method: EncryptionMethod::from_value(key.method),
                            creation_time: key.creation_time,
                            was_exposed: key.was_exposed,
                        },
                    )
                })
                .collect(),
        })
    }
    fn encode_envelope(&self, content: &KeyEnvelope) -> Result<Vec<u8>, TypedError> {
        Ok(pb::EncryptedContent {
            content: content.content.clone(),
            master_key: content.master_key.as_ref().map(meta_to_proto),
            iv: content.iv.clone(),
            ciphertext_key: content.ciphertext_key.clone(),
            ..Default::default()
        }
        .encode_to_vec())
    }
    fn decode_envelope(&self, bytes: &[u8]) -> Result<KeyEnvelope, TypedError> {
        let content = pb::EncryptedContent::decode(bytes).map_err(|e| {
            ErrorTemplate::new(
                ERR_PROTO_UNMARSHAL.code(),
                "fail to unmarshal encrypted encryption keys",
            )
            .with_source([] as [String; 0], e)
        })?;
        let meta = content
            .master_key
            .map(|m| match m.backend {
                Some(pb::master_key::Backend::Plaintext(_)) => Ok(MasterKeyMeta::Plaintext),
                Some(pb::master_key::Backend::File(c)) => Ok(MasterKeyMeta::File { path: c.path }),
                Some(pb::master_key::Backend::Kms(c)) => Ok(MasterKeyMeta::Kms {
                    vendor: c.vendor,
                    key_id: c.key_id,
                    region: c.region,
                    endpoint: c.endpoint,
                }),
                None => Err(ErrorTemplate::new(
                    "PD:encryption:ErrEncryptionNewMasterKey",
                    "unrecognized master key type",
                )
                .instantiate([] as [String; 0])),
            })
            .transpose()?;
        Ok(KeyEnvelope {
            content: content.content,
            master_key: meta,
            iv: content.iv,
            ciphertext_key: content.ciphertext_key,
        })
    }
}
