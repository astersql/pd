// Copyright 2026 AsterSQL.
//! Wire persistence adapter over the canonical neutral endpoint.
pub use astersql_pd_storage_domain::{EndpointError, LoadState};
use astersql_pd_storage_kv::{Base, MetaStore};
use std::sync::Arc;
/// Compatibility endpoint adds wire codecs; scheduling consumes only its neutral handle.
pub struct StorageEndpoint {
    pub(crate) store: Arc<dyn MetaStore>,
    pub(crate) next_region_id: u64,
    domain: astersql_pd_storage_domain::StorageEndpoint,
}
impl StorageEndpoint {
    pub fn new(store: Arc<dyn MetaStore>) -> Self {
        Self {
            domain: astersql_pd_storage_domain::StorageEndpoint::new(store.clone()),
            store,
            next_region_id: 0,
        }
    }
}
impl AsRef<astersql_pd_storage_domain::StorageEndpoint> for StorageEndpoint {
    fn as_ref(&self) -> &astersql_pd_storage_domain::StorageEndpoint {
        &self.domain
    }
}
impl From<StorageEndpoint> for astersql_pd_storage_domain::StorageEndpoint {
    fn from(endpoint: StorageEndpoint) -> Self {
        endpoint.domain
    }
}
impl std::ops::Deref for StorageEndpoint {
    type Target = astersql_pd_storage_domain::StorageEndpoint;
    fn deref(&self) -> &Self::Target {
        &self.domain
    }
}
pub mod protobuf {
    use super::*;
    use prost::Message;
    /// Encode protobuf in the persistence adapter before writing.
    pub async fn save<M: Message>(
        endpoint: &StorageEndpoint,
        key: &[u8],
        value: &M,
    ) -> Result<(), EndpointError> {
        let mut bytes = Vec::with_capacity(value.encoded_len());
        value
            .encode(&mut bytes)
            .map_err(|error| EndpointError::ProtoMarshal(Box::new(error)))?;
        Base::new(endpoint.store.as_ref()).save(key, &bytes).await?;
        Ok(())
    }
    /// Missing and empty values are absent; malformed nonempty wire bytes are errors.
    pub async fn load<M: Message + Default>(
        endpoint: &StorageEndpoint,
        key: &[u8],
    ) -> Result<Option<M>, EndpointError> {
        match endpoint.load(key).await? {
            LoadState::Missing | LoadState::Empty => Ok(None),
            LoadState::Value(bytes) => M::decode(bytes.as_slice())
                .map(Some)
                .map_err(|error| EndpointError::ProtoUnmarshal(Box::new(error))),
        }
    }
}
