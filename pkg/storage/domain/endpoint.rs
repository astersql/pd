// Copyright 2026 AsterSQL.

use crate::kv::{Base, Consistency, MetaStore, Read, StoreError};
use serde::{Serialize, de::DeserializeOwned};
use std::sync::Arc;
/// Codec failures remain distinct from backend failures.
#[derive(Debug, thiserror::Error)]
pub enum EndpointError {
    #[error("parse float error")]
    ParseFloat,
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("json marshal: {0}")]
    JsonMarshal(serde_json::Error),
    #[error("json unmarshal: {0}")]
    JsonUnmarshal(serde_json::Error),
    #[error("proto marshal: {0}")]
    ProtoMarshal(Box<dyn std::error::Error + Send + Sync>),
    #[error("proto unmarshal: {0}")]
    ProtoUnmarshal(Box<dyn std::error::Error + Send + Sync>),
}
/// Physical single-key load result before decoding.
#[derive(Debug, PartialEq, Eq)]
pub enum LoadState {
    Missing,
    Empty,
    Value(Vec<u8>),
}
/// Shared store with caller-supplied absolute keys; no global cluster state.
#[derive(Clone)]
pub struct StorageEndpoint {
    pub(crate) store: Arc<dyn MetaStore>,
}
impl StorageEndpoint {
    /// Use the canonical MetaStore supplied by the host.
    pub fn new(store: Arc<dyn MetaStore>) -> Self {
        Self { store }
    }
    /// Preserve physical presence; Go-compatible typed helpers collapse empty and missing.
    pub async fn load(&self, key: &[u8]) -> Result<LoadState, EndpointError> {
        let snapshot = self
            .store
            .read(Read::range(key, b"", Consistency::Linearizable))
            .await?;
        match snapshot.entries.as_slice() {
            [] => Ok(LoadState::Missing),
            [entry] if entry.value.is_empty() => Ok(LoadState::Empty),
            [entry] => Ok(LoadState::Value(entry.value.clone())),
            _ => Err(StoreError::Backend.into()),
        }
    }
    /// Encode a Serde persistence model and overwrite one key without a lease.
    /// Models own Go schema mappings (field names, nullable values and custom codecs).
    pub async fn save_json<T: Serialize + ?Sized>(
        &self,
        key: &[u8],
        value: &T,
    ) -> Result<(), EndpointError> {
        let bytes = serde_json::to_vec(value).map_err(EndpointError::JsonMarshal)?;
        Base::new(self.store.as_ref()).save(key, &bytes).await?;
        Ok(())
    }
    /// Decode a model, returning its default for missing, empty or JSON null values.
    pub async fn load_json<T: DeserializeOwned + Default>(
        &self,
        key: &[u8],
    ) -> Result<T, EndpointError> {
        match self.load(key).await? {
            LoadState::Missing | LoadState::Empty => Ok(T::default()),
            LoadState::Value(bytes) => serde_json::from_slice::<Option<T>>(&bytes)
                .map(Option::unwrap_or_default)
                .map_err(EndpointError::JsonUnmarshal),
        }
    }
}
impl AsRef<StorageEndpoint> for StorageEndpoint {
    fn as_ref(&self) -> &StorageEndpoint {
        self
    }
}
