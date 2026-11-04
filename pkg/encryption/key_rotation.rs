// Copyright 2026 AsterSQL.
//! Term-owned rotation with host time/cryptographic entropy and durable readback.
use crate::{
    DictionaryStorage, EncryptionConfig, EncryptionMethod, KeySnapshot, MasterKeyMeta,
    new_data_key_with_rng,
};
use astersql_pd_election::Leadership;
use astersql_pd_errs::{ErrorTemplate, TypedError, catalog::*};
use astersql_pd_storage_domain::Commit;
use std::sync::Arc;
pub type KeyClock = Arc<dyn Fn() -> i128 + Send + Sync>;
pub trait CryptoRandom: rand::RngCore + rand::CryptoRng + Send {}
impl<T: rand::RngCore + rand::CryptoRng + Send> CryptoRandom for T {}
struct RotationState {
    term: Option<Arc<Leadership>>,
    random: Box<dyn CryptoRandom>,
}
pub struct KeyRotation {
    storage: Arc<DictionaryStorage>,
    method: EncryptionMethod,
    period: i64,
    master_key: MasterKeyMeta,
    clock: KeyClock,
    state: tokio::sync::Mutex<RotationState>,
}
fn wall_time() -> i128 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(value) => value.as_nanos() as i128,
        Err(value) => -(value.duration().as_nanos() as i128),
    }
}
impl KeyRotation {
    pub fn new(
        storage: Arc<DictionaryStorage>,
        config: &EncryptionConfig,
    ) -> Result<Self, TypedError> {
        Self::new_with_resources(storage, config, Arc::new(wall_time), rand::rngs::OsRng)
    }
    /// Host resources are real clock and CryptoRng implementations; production defaults to OsRng.
    pub fn new_with_resources<R: CryptoRandom + 'static>(
        storage: Arc<DictionaryStorage>,
        config: &EncryptionConfig,
        clock: KeyClock,
        random: R,
    ) -> Result<Self, TypedError> {
        Ok(Self {
            storage,
            method: config.method()?,
            period: config.data_key_rotation_period.as_nanos(),
            master_key: config.master_key_meta()?,
            clock,
            state: tokio::sync::Mutex::new(RotationState {
                term: None,
                random: Box::new(random),
            }),
        })
    }
    pub(crate) fn storage(&self) -> Arc<DictionaryStorage> {
        self.storage.clone()
    }
    pub(crate) fn period_nanos(&self) -> i64 {
        self.period
    }
    pub fn snapshot(&self) -> KeySnapshot {
        self.storage.snapshot()
    }
    /// Assignment forces master-key rewrapping, unless plaintext is already disabled.
    pub async fn set_leadership(
        &self,
        term: Option<Arc<Leadership>>,
    ) -> Result<Option<Commit>, TypedError> {
        let mut state = self.state.lock().await;
        state.term = term;
        self.rotate(&mut state, true).await
    }
    pub async fn tick(&self) -> Result<Option<Commit>, TypedError> {
        let mut state = self.state.lock().await;
        self.rotate(&mut state, false).await
    }
    async fn rotate(
        &self,
        state: &mut RotationState,
        force: bool,
    ) -> Result<Option<Commit>, TypedError> {
        let Some(term) = state.term.clone() else {
            return Ok(None);
        };
        if term.is_expired() {
            state.term = None;
            return Ok(None);
        }
        let operation = self.storage.begin_operation().await;
        let snapshot = operation.load().await?;
        // Never mutate a dictionary already visible to readers, including on a failed commit.
        let mut keys = snapshot.keys.as_deref().cloned().unwrap_or_default();
        let mut update = force;
        if self.method == EncryptionMethod::Plaintext {
            if keys.current_key_id == 0 {
                return Ok(None);
            }
            keys.current_key_id = 0;
            update = true;
        } else {
            let rotate = if keys.current_key_id == 0 {
                true
            } else {
                let key = keys.keys.get(&keys.current_key_id).ok_or_else(|| {
                    ErrorTemplate::new(ERR_ENCRYPTION_CURRENT_KEY_NOT_FOUND.code(), "keyId = %d")
                        .instantiate([keys.current_key_id.to_string()])
                })?;
                // Go casts uint64 creation time to int64 before Unix and uses strict Before.
                let expiry =
                    (key.creation_time as i64 as i128) * 1_000_000_000 + i128::from(self.period);
                key.method != self.method || key.was_exposed || expiry < (self.clock)()
            };
            if rotate {
                let mut rotated = false;
                for _ in 0..10 {
                    let creation = ((self.clock)().div_euclid(1_000_000_000) as i64) as u64;
                    let Ok((id, key)) =
                        new_data_key_with_rng(self.method, creation, &mut *state.random)
                    else {
                        // Go suppresses NewDataKey errors here; keep the durable snapshot.
                        return Ok(None);
                    };
                    if let std::collections::btree_map::Entry::Vacant(entry) = keys.keys.entry(id) {
                        entry.insert(key);
                        keys.current_key_id = id;
                        rotated = true;
                        break;
                    }
                }
                if !rotated {
                    return Err(ErrorTemplate::new(
                        ERR_ENCRYPTION_ROTATE_DATA_KEY.code(),
                        "maximum attempts reached",
                    )
                    .instantiate([] as [String; 0]));
                }
                update = true;
            }
        }
        if !update {
            return Ok(None);
        }
        let receipt = operation.save(&term, &self.master_key, keys).await?;
        operation.load().await?;
        Ok(Some(receipt))
    }
}
