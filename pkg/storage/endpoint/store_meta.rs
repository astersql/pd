// Copyright 2026 AsterSQL.

use crate::{EndpointError, StorageEndpoint, protobuf};
use astersql_pd_core::{NodeState, StoreLabel, StoreMeta, StoreState, TxnProtocolVersionRange};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::Base;

// Pinned metapb.Store persistence schema; protocol details stay in this adapter.
mod wire {
    #[derive(prost::Message)]
    pub(super) struct WireStore {
        #[prost(uint64, tag = "1")]
        pub id: u64,
        #[prost(string, tag = "2")]
        pub address: String,
        #[prost(int32, tag = "3")]
        pub state: i32,
        #[prost(message, repeated, tag = "4")]
        pub labels: Vec<WireLabel>,
        #[prost(string, tag = "5")]
        pub version: String,
        #[prost(string, tag = "6")]
        pub peer_address: String,
        #[prost(string, tag = "7")]
        pub status_address: String,
        #[prost(string, tag = "8")]
        pub git_hash: String,
        #[prost(int64, tag = "9")]
        pub start_timestamp: i64,
        #[prost(string, tag = "10")]
        pub deploy_path: String,
        #[prost(int64, tag = "11")]
        pub last_heartbeat: i64,
        #[prost(bool, tag = "12")]
        pub physically_destroyed: bool,
        #[prost(int32, tag = "13")]
        pub node_state: i32,
        #[prost(message, optional, tag = "14")]
        pub txn_protocol_version_range: Option<WireRange>,
    }
    #[derive(prost::Message)]
    pub(super) struct WireLabel {
        #[prost(string, tag = "1")]
        pub key: String,
        #[prost(string, tag = "2")]
        pub value: String,
    }
    #[derive(prost::Message)]
    pub(super) struct WireRange {
        #[prost(uint32, tag = "1")]
        pub min: u32,
        #[prost(uint32, tag = "2")]
        pub max: u32,
    }
}
use wire::{WireLabel, WireRange, WireStore};

impl From<&StoreMeta> for WireStore {
    fn from(meta: &StoreMeta) -> Self {
        Self {
            id: meta.id,
            address: meta.address.clone(),
            state: match meta.state {
                StoreState::Up => 0,
                StoreState::Offline => 1,
                StoreState::Tombstone => 2,
                StoreState::Unknown(value) => value,
            },
            labels: meta
                .labels
                .iter()
                .map(|label| WireLabel {
                    key: label.key.clone(),
                    value: label.value.clone(),
                })
                .collect(),
            version: meta.version.clone(),
            peer_address: meta.peer_address.clone(),
            status_address: meta.status_address.clone(),
            git_hash: meta.git_hash.clone(),
            start_timestamp: meta.start_timestamp,
            deploy_path: meta.deploy_path.clone(),
            last_heartbeat: meta.last_heartbeat,
            physically_destroyed: meta.physically_destroyed,
            node_state: match meta.node_state {
                NodeState::Preparing => 0,
                NodeState::Serving => 1,
                NodeState::Removing => 2,
                NodeState::Removed => 3,
                NodeState::Unknown(value) => value,
            },
            txn_protocol_version_range: meta.txn_protocol_version_range.as_ref().map(|range| {
                WireRange {
                    min: range.min,
                    max: range.max,
                }
            }),
        }
    }
}
impl From<WireStore> for StoreMeta {
    fn from(meta: WireStore) -> Self {
        Self {
            id: meta.id,
            address: meta.address,
            state: match meta.state {
                0 => StoreState::Up,
                1 => StoreState::Offline,
                2 => StoreState::Tombstone,
                value => StoreState::Unknown(value),
            },
            labels: meta
                .labels
                .into_iter()
                .map(|label| StoreLabel {
                    key: label.key,
                    value: label.value,
                })
                .collect(),
            version: meta.version,
            peer_address: meta.peer_address,
            status_address: meta.status_address,
            git_hash: meta.git_hash,
            start_timestamp: meta.start_timestamp,
            deploy_path: meta.deploy_path,
            last_heartbeat: meta.last_heartbeat,
            physically_destroyed: meta.physically_destroyed,
            node_state: match meta.node_state {
                0 => NodeState::Preparing,
                1 => NodeState::Serving,
                2 => NodeState::Removing,
                3 => NodeState::Removed,
                value => NodeState::Unknown(value),
            },
            txn_protocol_version_range: meta.txn_protocol_version_range.map(|range| {
                TxnProtocolVersionRange {
                    min: range.min,
                    max: range.max,
                }
            }),
        }
    }
}
impl StorageEndpoint {
    /// Read one store without applying LoadStores lifecycle compatibility mappings.
    pub async fn load_store_meta(
        &self,
        paths: &ClusterPaths,
        id: u64,
    ) -> Result<Option<StoreMeta>, EndpointError> {
        Ok(
            protobuf::load::<WireStore>(self, paths.store_path(id).as_bytes())
                .await?
                .map(Into::into),
        )
    }
    /// Persist every known store field without a lease or address validation.
    pub async fn save_store_meta(
        &self,
        paths: &ClusterPaths,
        meta: &StoreMeta,
    ) -> Result<(), EndpointError> {
        protobuf::save(
            self,
            paths.store_path(meta.id).as_bytes(),
            &WireStore::from(meta),
        )
        .await
    }
    /// Remove only the metadata key, leaving scheduling weights intact.
    pub async fn delete_store_meta(
        &self,
        paths: &ClusterPaths,
        meta: &StoreMeta,
    ) -> Result<(), EndpointError> {
        Base::new(self.store.as_ref())
            .remove(paths.store_path(meta.id).as_bytes())
            .await?;
        Ok(())
    }
}

// Range loading must decode even empty values as the zero Store.
pub(crate) fn decode_store(bytes: &[u8]) -> Result<StoreMeta, EndpointError> {
    use prost::Message;
    WireStore::decode(bytes)
        .map(Into::into)
        .map_err(|error| EndpointError::ProtoUnmarshal(Box::new(error)))
}
