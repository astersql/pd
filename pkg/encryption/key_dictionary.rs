// Copyright 2026 AsterSQL.
//! Backend-neutral dictionary snapshots; protobuf conversion belongs to the server adapter.
use crate::{DataKey, MasterKey, MasterKeyMeta};
use astersql_pd_election::{ElectionError, Leadership};
use astersql_pd_errs::{ErrorTemplate, TypedError, catalog::*};
use astersql_pd_storage_domain::{Commit, Consistency, Entry, MetaStore, Mutation, Read};
use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};
use zeroize::Zeroizing;
pub const ENCRYPTION_KEYS_PATH: &[u8] = b"encryption_keys";
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyDictionary {
    pub current_key_id: u64,
    pub keys: BTreeMap<u64, DataKey>,
}
pub struct KeyEnvelope {
    pub content: Vec<u8>,
    pub master_key: Option<MasterKeyMeta>,
    pub iv: Vec<u8>,
    pub ciphertext_key: Vec<u8>,
}
pub trait KeyDictionaryCodec: Send + Sync {
    fn encode_keys(&self, keys: &KeyDictionary) -> Result<Vec<u8>, TypedError>;
    fn decode_keys(&self, bytes: &[u8]) -> Result<KeyDictionary, TypedError>;
    fn encode_envelope(&self, content: &KeyEnvelope) -> Result<Vec<u8>, TypedError>;
    fn decode_envelope(&self, bytes: &[u8]) -> Result<KeyEnvelope, TypedError>;
}
#[derive(Clone, Debug, Default)]
pub struct KeySnapshot {
    pub revision: i64,
    pub keys: Option<Arc<KeyDictionary>>,
}
pub struct DictionaryStorage {
    store: Arc<dyn MetaStore>,
    codec: Arc<dyn KeyDictionaryCodec>,
    operation: tokio::sync::Mutex<()>,
    published: RwLock<KeySnapshot>,
}
impl DictionaryStorage {
    pub fn new(store: Arc<dyn MetaStore>, codec: Arc<dyn KeyDictionaryCodec>) -> Self {
        Self {
            store,
            codec,
            operation: tokio::sync::Mutex::new(()),
            published: RwLock::new(KeySnapshot::default()),
        }
    }
    pub fn snapshot(&self) -> KeySnapshot {
        self.published
            .read()
            .expect("dictionary snapshot poisoned")
            .clone()
    }
    /// Initial absence is permitted; deletion after a successful load is an error.
    pub async fn load(&self) -> Result<KeySnapshot, TypedError> {
        let _operation = self.operation.lock().await;
        let rows = self
            .store
            .read(Read::range(
                ENCRYPTION_KEYS_PATH,
                b"",
                Consistency::Linearizable,
            ))
            .await
            .map_err(|e| ERR_ETCD_KV_GET.with_source([] as [String; 0], e))?;
        if let Some(entry) = rows.entries.first() {
            self.load_entry_locked(entry).await
        } else {
            let snapshot = self.snapshot();
            if snapshot.revision > 0 {
                Err(ErrorTemplate::new(
                    ERR_ENCRYPTION_LOAD_KEYS.code(),
                    "encryption keys is deleted unexpectedly",
                )
                .instantiate([] as [String; 0]))
            } else {
                Ok(snapshot)
            }
        }
    }
    /// Older/equal watch events are ignored before parsing or requesting a master key.
    pub async fn load_entry(&self, entry: &Entry) -> Result<KeySnapshot, TypedError> {
        let _operation = self.operation.lock().await;
        self.load_entry_locked(entry).await
    }
    async fn load_entry_locked(&self, entry: &Entry) -> Result<KeySnapshot, TypedError> {
        let previous = self.snapshot();
        if entry.mod_revision <= previous.revision {
            return Ok(previous);
        }
        let envelope = self.codec.decode_envelope(&entry.value)?;
        let meta = envelope.master_key.as_ref().ok_or_else(|| {
            ErrorTemplate::new(
                ERR_ENCRYPTION_LOAD_KEYS.code(),
                "no master key config found with encryption keys",
            )
            .instantiate([] as [String; 0])
        })?;
        let master = MasterKey::from_config(Some(meta), &envelope.ciphertext_key).await?;
        let plaintext = Zeroizing::new(master.decrypt(&envelope.content, &envelope.iv)?);
        let keys = self.codec.decode_keys(&plaintext)?;
        let next = KeySnapshot {
            revision: entry.mod_revision,
            keys: Some(Arc::new(keys)),
        };
        *self
            .published
            .write()
            .expect("dictionary snapshot poisoned") = next.clone();
        Ok(next)
    }
    /// Persist through the actual captured term. Publication remains the loader's job.
    pub async fn save(
        &self,
        leadership: &Leadership,
        meta: &MasterKeyMeta,
        mut keys: KeyDictionary,
    ) -> Result<Commit, TypedError> {
        let _operation = self.operation.lock().await;
        if !leadership.uses_store(&self.store) {
            return Err(ErrorTemplate::new(
                ERR_ENCRYPTION_SAVE_DATA_KEYS.code(),
                "leadership uses a different metadata store",
            )
            .instantiate([] as [String; 0]));
        }
        let master = MasterKey::from_config(Some(meta), &[]).await?;
        if master.is_plaintext() {
            for key in keys.keys.values_mut() {
                key.was_exposed = true;
            }
        }
        let plaintext = Zeroizing::new(self.codec.encode_keys(&keys)?);
        let (content, iv) = master.encrypt(&plaintext)?;
        let envelope = KeyEnvelope {
            content,
            iv,
            master_key: Some(meta.clone()),
            ciphertext_key: master.ciphertext_key().to_vec(),
        };
        let value = self.codec.encode_envelope(&envelope)?;
        let receipt = leadership
            .transact(vec![], vec![Mutation::put(ENCRYPTION_KEYS_PATH, &value)])
            .await
            .map_err(|e| match e {
                ElectionError::NotLeader => {
                    ErrorTemplate::new(ERR_ENCRYPTION_SAVE_DATA_KEYS.code(), "leader expired")
                        .instantiate([] as [String; 0])
                }
                e => {
                    ErrorTemplate::new(ERR_ETCD_TXN_INTERNAL.code(), "fail to save encryption keys")
                        .with_source([] as [String; 0], e)
                }
            })?;
        if !receipt.succeeded {
            return Err(
                ErrorTemplate::new(ERR_ENCRYPTION_SAVE_DATA_KEYS.code(), "leader expired")
                    .instantiate([] as [String; 0]),
            );
        }
        Ok(receipt)
    }
}
