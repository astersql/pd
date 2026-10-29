// Copyright 2026 AsterSQL.

use crate::{EndpointError, StorageEndpoint, protobuf};
use astersql_pd_keypath::ClusterPaths;

/// Cluster metadata persisted independently of transport messages.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cluster {
    /// Cluster identity stored in metapb.Cluster field 1.
    pub id: u64,
    /// Desired peer count stored in metapb.Cluster field 2.
    pub max_peer_count: u32,
}

#[derive(prost::Message)]
struct WireCluster {
    #[prost(uint64, tag = "1")]
    id: u64,
    #[prost(uint32, tag = "2")]
    max_peer_count: u32,
}

impl StorageEndpoint {
    /// Read cluster metadata; missing and empty values are absent.
    pub async fn load_meta(&self, paths: &ClusterPaths) -> Result<Option<Cluster>, EndpointError> {
        Ok(
            protobuf::load::<WireCluster>(self, paths.cluster_path().as_bytes())
                .await?
                .map(|wire| Cluster {
                    id: wire.id,
                    max_peer_count: wire.max_peer_count,
                }),
        )
    }

    /// Save cluster metadata at the host-supplied cluster path without a lease.
    pub async fn save_meta(
        &self,
        paths: &ClusterPaths,
        meta: &Cluster,
    ) -> Result<(), EndpointError> {
        protobuf::save(
            self,
            paths.cluster_path().as_bytes(),
            &WireCluster {
                id: meta.id,
                max_peer_count: meta.max_peer_count,
            },
        )
        .await
    }
}

impl StorageEndpoint {
    /// Atomically create the initial raft metadata. A losing concurrent caller writes nothing.
    pub async fn bootstrap_cluster(
        &self,
        paths: &ClusterPaths,
        meta: &Cluster,
        store: &astersql_pd_core::StoreMeta,
        region: &astersql_pd_core::RegionMeta,
        unix_nanos: u64,
    ) -> Result<bool, EndpointError> {
        use astersql_pd_storage_kv::{Compare, Mutation};
        use prost::Message;
        let cluster = WireCluster {
            id: meta.id,
            max_peer_count: meta.max_peer_count,
        }
        .encode_to_vec();
        Ok(self
            .store
            .transact(
                vec![Compare::CreateRevision(
                    paths.cluster_path().into_bytes(),
                    0,
                )],
                vec![
                    Mutation::put(paths.cluster_path().as_bytes(), &cluster),
                    Mutation::put(
                        paths.cluster_bootstrap_time_path().as_bytes(),
                        &unix_nanos.to_be_bytes(),
                    ),
                    Mutation::put(
                        paths.store_path(store.id).as_bytes(),
                        &crate::store_meta::bootstrap_bytes(store),
                    ),
                    Mutation::put(
                        paths.region_path(region.id).as_bytes(),
                        &crate::region_meta::bootstrap_bytes(region),
                    ),
                ],
            )
            .await?
            .succeeded)
    }
}
