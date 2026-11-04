// Copyright 2026 AsterSQL.
//! Lossless ciphertext representation in the existing Rust-only history namespace.
use astersql_pd_core::{KeyRange, RegionEncryptionMeta, RegionMeta};
use astersql_pd_encryption::{DictionaryStorage, decrypt_region, encrypt_region};
use astersql_pd_storage::{HistoryCipher, HistoryError, HistoryHotRegion};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::sync::Arc;
struct RegionHistoryCipher {
    keys: Arc<DictionaryStorage>,
}
pub(crate) fn codec(keys: Option<Arc<DictionaryStorage>>) -> Option<Arc<dyn HistoryCipher>> {
    keys.map(|keys| Arc::new(RegionHistoryCipher { keys }) as Arc<dyn HistoryCipher>)
}
fn failure(error: impl std::error::Error + Send + Sync + 'static) -> HistoryError {
    HistoryError::Cipher(Box::new(error))
}
fn invalid() -> HistoryError {
    failure(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "invalid encrypted history key encoding",
    ))
}
#[async_trait::async_trait]
impl HistoryCipher for RegionHistoryCipher {
    fn encrypt(&self, row: &HistoryHotRegion) -> Result<HistoryHotRegion, HistoryError> {
        if row.encryption_meta.is_some() {
            return Err(HistoryError::EncryptionUnavailable);
        }
        let input = RegionMeta {
            id: row.region_id,
            key_range: KeyRange {
                start_key: row.start_key.as_bytes().to_vec(),
                end_key: row.end_key.as_bytes().to_vec(),
            },
            ..Default::default()
        };
        let encrypted = encrypt_region(Some(&input), Some(&self.keys)).map_err(failure)?;
        let mut output = row.clone();
        if let Some(meta) = &encrypted.encryption_meta {
            output.start_key = STANDARD.encode(&encrypted.key_range.start_key);
            output.end_key = STANDARD.encode(&encrypted.key_range.end_key);
            output.encryption_meta = Some(
                serde_json::json!({"key_id":meta.key_id,"iv":STANDARD.encode(&meta.iv),"key_encoding":"base64-v1"}),
            );
        }
        Ok(output)
    }
    async fn decrypt(&self, mut row: HistoryHotRegion) -> Result<HistoryHotRegion, HistoryError> {
        let Some(metadata) = row.encryption_meta.as_ref() else {
            return Ok(row);
        };
        if metadata.get("key_encoding").and_then(|v| v.as_str()) != Some("base64-v1") {
            return Err(invalid());
        }
        let id = metadata
            .get("key_id")
            .and_then(|v| v.as_u64())
            .ok_or_else(invalid)?;
        let iv = STANDARD
            .decode(
                metadata
                    .get("iv")
                    .and_then(|v| v.as_str())
                    .ok_or_else(invalid)?,
            )
            .map_err(failure)?;
        let mut region = RegionMeta {
            id: row.region_id,
            key_range: KeyRange {
                start_key: STANDARD.decode(&row.start_key).map_err(failure)?,
                end_key: STANDARD.decode(&row.end_key).map_err(failure)?,
            },
            encryption_meta: Some(RegionEncryptionMeta { key_id: id, iv }),
            ..Default::default()
        };
        decrypt_region(Some(&mut region), Some(&self.keys))
            .await
            .map_err(failure)?;
        row.start_key = std::str::from_utf8(&region.key_range.start_key)
            .map_err(failure)?
            .to_owned();
        row.end_key = std::str::from_utf8(&region.key_range.end_key)
            .map_err(failure)?
            .to_owned();
        row.encryption_meta = None;
        Ok(row)
    }
}
