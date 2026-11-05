// Copyright 2026 AsterSQL.
//! GC resources live exactly as long as their immutable serving term.
use astersql_pd_gc::{GCLeadership, GCStateManager};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_keyspace::KeyspaceManager;
use astersql_pd_member::Leadership;
use astersql_pd_storage::MetaStore;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
pub(crate) struct GCResources {
    pub(crate) manager: Arc<GCStateManager>,
    role: tokio::sync::Mutex<Option<GCLeadership>>,
}
impl GCResources {
    pub(crate) async fn new(
        store: Arc<dyn MetaStore>,
        leader: Arc<Leadership>,
        cancel: &CancellationToken,
        cluster: u64,
    ) -> Self {
        let store: Arc<dyn MetaStore> = Arc::new(crate::member_role_worker::TermStore::new(
            store,
            leader,
            cancel.clone(),
        ));
        let paths = ClusterPaths::new(cluster);
        let keyspaces = Arc::new(KeyspaceManager::new(
            astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone())
                .keyspace_storage(paths.clone()),
        ));
        let manager = Arc::new(GCStateManager::new(
            astersql_pd_storage_domain::StorageEndpoint::new(store),
            paths,
            keyspaces,
            tokio::runtime::Handle::current(),
        ));
        let role = manager.on_node_becomes_leader_with_cancel(cancel).await;
        Self {
            manager,
            role: tokio::sync::Mutex::new(Some(role)),
        }
    }
    pub(crate) async fn shutdown(&self) {
        if let Some(role) = self.role.lock().await.take() {
            role.shutdown().await;
        }
    }
}
