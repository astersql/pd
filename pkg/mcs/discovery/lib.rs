// Copyright 2026 AsterSQL.
//! Service discovery over the host's canonical metadata store.
mod registry_entry;
mod service_register;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{Consistency, MetaStore, Read};
pub use registry_entry::*;
pub use service_register::*;
use tokio_util::sync::CancellationToken;
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error(transparent)]
    Store(#[from] astersql_pd_storage_domain::StoreError),
    #[error("unknown service name {0}")]
    UnknownService(String),
    #[error("service registry key is occupied by another live instance")]
    Occupied,
    #[error("service registry key is occupied by an entry with no lease and will never expire")]
    Permanent,
}
/// Return raw registry values in metadata key order, without interpreting the service name.
pub async fn discover(
    store: &dyn MetaStore,
    cluster: u64,
    service: &str,
    cancel: &CancellationToken,
) -> Result<Vec<Vec<u8>>, DiscoveryError> {
    tokio::select! {biased;_=cancel.cancelled()=>Err(astersql_pd_storage_domain::StoreError::Cancelled.into()),r=store.read(Read::prefix(ClusterPaths::new(cluster).service_path(service).as_bytes(),Consistency::Linearizable))=>Ok(r?.entries.into_iter().map(|e|e.value).collect())}
}
/// Read current members. Corrupt entries are skipped; an empty valid directory is JSON null.
pub async fn get_ms_members(
    store: &dyn MetaStore,
    cluster: u64,
    service: &str,
    cancel: &CancellationToken,
) -> Result<Option<Vec<ServiceRegistryEntry>>, DiscoveryError> {
    if !matches!(
        service,
        "tso" | "scheduling" | "resource_manager" | "router"
    ) {
        return Err(DiscoveryError::UnknownService(service.into()));
    }
    let entries = discover(store, cluster, service, cancel)
        .await?
        .into_iter()
        .filter_map(|b| ServiceRegistryEntry::deserialize(&b).ok())
        .collect::<Vec<_>>();
    Ok((!entries.is_empty()).then_some(entries))
}
#[cfg(test)]
mod discovery_test;
