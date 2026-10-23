// Copyright 2026 AsterSQL.
//! Encryption configuration values, without key IO or KMS initialization.
use astersql_pd_typeutil::Duration;
use serde::{Deserialize, Serialize};
/// Pure values from Go Config; no service or IO initialization.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EncryptionConfig {
    #[serde(rename = "data-encryption-method")]
    #[serde(deserialize_with = "null_default")]
    pub data_encryption_method: String,
    #[serde(rename = "data-key-rotation-period")]
    pub data_key_rotation_period: Duration,
    #[serde(rename = "master-key")]
    #[serde(deserialize_with = "null_default")]
    pub master_key: MasterKeyConfig,
}
/// Pure values from Go MasterKeyKMSConfig; no service or IO initialization.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MasterKeyKMSConfig {
    #[serde(rename = "key-id")]
    #[serde(deserialize_with = "null_default")]
    pub kms_key_id: String,
    #[serde(rename = "region")]
    #[serde(deserialize_with = "null_default")]
    pub kms_region: String,
    #[serde(rename = "endpoint")]
    #[serde(deserialize_with = "null_default")]
    pub kms_endpoint: String,
}
/// Pure values from Go MasterKeyFileConfig; no service or IO initialization.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MasterKeyFileConfig {
    #[serde(rename = "path")]
    #[serde(deserialize_with = "null_default")]
    pub file_path: String,
}

/// Flattened Go master key fields, without loading key material.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MasterKeyConfig {
    #[serde(rename = "type")]
    #[serde(deserialize_with = "null_default")]
    pub key_type: String,
    #[serde(flatten)]
    #[serde(deserialize_with = "null_default")]
    pub kms: MasterKeyKMSConfig,
    #[serde(flatten)]
    #[serde(deserialize_with = "null_default")]
    pub file: MasterKeyFileConfig,
}
/// Protocol-independent encryption method.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncryptionMethod {
    Plaintext,
    Aes128Ctr,
    Aes192Ctr,
    Aes256Ctr,
}
/// Master key metadata without protobuf or key IO.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MasterKeyMeta {
    Plaintext,
    Kms {
        vendor: &'static str,
        key_id: String,
        region: String,
        endpoint: String,
    },
    File {
        path: String,
    },
}
impl EncryptionConfig {
    /// Validates values and applies defaults in Go's error precedence order.
    pub fn adjust(&mut self) -> Result<(), astersql_pd_errs::TypedError> {
        if self.data_encryption_method.is_empty() {
            self.data_encryption_method = "plaintext".into();
        } else {
            self.method()?;
        }
        if self.data_key_rotation_period.as_nanos() == 0 {
            self.data_key_rotation_period = Duration::from_nanos(604800000000000);
        } else if self.data_key_rotation_period.as_nanos() < 0 {
            return Err(invalid_config(format!(
                "negative data-key-rotation-period {}",
                self.data_key_rotation_period.as_nanos()
            )));
        }
        if self.master_key.key_type.is_empty() {
            self.master_key.key_type = "plaintext".into();
        } else {
            self.master_key_meta()?;
        }
        Ok(())
    }
    /// Parses the method without applying defaults.
    pub fn method(&self) -> Result<EncryptionMethod, astersql_pd_errs::TypedError> {
        match self.data_encryption_method.as_str() {
            "plaintext" => Ok(EncryptionMethod::Plaintext),
            "aes128-ctr" => Ok(EncryptionMethod::Aes128Ctr),
            "aes192-ctr" => Ok(EncryptionMethod::Aes192Ctr),
            "aes256-ctr" => Ok(EncryptionMethod::Aes256Ctr),
            _ => Err(astersql_pd_errs::ErrorTemplate::new(
                "PD:encryption:ErrEncryptionInvalidMethod",
                "unknown method",
            )
            .instantiate([] as [String; 0])),
        }
    }
    /// Creates domain metadata; does not validate AWS fields or open a file.
    pub fn master_key_meta(&self) -> Result<MasterKeyMeta, astersql_pd_errs::TypedError> {
        match self.master_key.key_type.as_str() {
            "plaintext" => Ok(MasterKeyMeta::Plaintext),
            "kms" => Ok(MasterKeyMeta::Kms {
                vendor: "AWS",
                key_id: self.master_key.kms.kms_key_id.clone(),
                region: self.master_key.kms.kms_region.clone(),
                endpoint: self.master_key.kms.kms_endpoint.clone(),
            }),
            "file" => Ok(MasterKeyMeta::File {
                path: self.master_key.file.file_path.clone(),
            }),
            _ => Err(invalid_config(format!(
                "unrecognized encryption master key type: {}",
                self.master_key.key_type
            ))),
        }
    }
}
fn invalid_config(message: String) -> astersql_pd_errs::TypedError {
    astersql_pd_errs::ErrorTemplate::new("PD:encryption:ErrEncryptionInvalidConfig", "%s")
        .instantiate([message])
}

fn null_default<'de, T: serde::Deserialize<'de> + Default, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<T, D::Error> {
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[cfg(test)]
mod config_test;
