// Copyright 2026 AsterSQL.
//! Term-owned canonical keyspace resources. The scheduler retains its one labeler.
use super::cluster_host::ClusterHost;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_keyspace::*;
use astersql_pd_storage::MetaStore;
use std::sync::{Arc, RwLock, Weak};
struct Labels(Weak<ClusterHost>);
#[async_trait::async_trait]
impl KeyspaceLabeler for Labels {
    async fn set(
        &self,
        rule: astersql_pd_schedule_labeler::LabelRule,
        now: i128,
    ) -> Result<(), astersql_pd_schedule_labeler::LabelerError> {
        self.0
            .upgrade()
            .ok_or_else(stopped)?
            .keyspace_label(Some(rule), String::new(), now)
            .await
    }
    async fn delete(&self, id: &str) -> Result<(), astersql_pd_schedule_labeler::LabelerError> {
        self.0
            .upgrade()
            .ok_or_else(stopped)?
            .keyspace_label(None, id.into(), 0)
            .await
    }
}
fn stopped() -> astersql_pd_schedule_labeler::LabelerError {
    astersql_pd_schedule_labeler::LabelerError::Load(
        astersql_pd_storage_domain::EndpointError::Store(
            astersql_pd_storage::StoreError::Unavailable,
        ),
    )
}
pub(crate) struct KeyspaceResources {
    cancel: tokio_util::sync::CancellationToken,
    preallocations: Vec<String>,
    errors: Arc<std::sync::Mutex<Vec<String>>>,
    work: tokio_util::task::TaskTracker,
    pub manager: Arc<KeyspaceManager>,
    pub groups: Option<Arc<KeyspaceGroupManager>>,
    initialized: tokio::sync::OnceCell<()>,
    creation_config: Arc<RwLock<KeyspaceCreateConfig>>,
    split_config: Arc<RwLock<KeyspaceSplitConfig>>,
}
impl KeyspaceResources {
    pub async fn track(
        &self,
        work: impl std::future::Future<
            Output = Result<astersql_pd_storage_domain::Keyspace, KeyspaceError>,
        > + Send
        + 'static,
    ) -> Result<astersql_pd_storage_domain::Keyspace, KeyspaceError> {
        self.work
            .spawn(work)
            .await
            .map_err(|e| KeyspaceError::Validation(e.to_string()))?
    }
    pub async fn shutdown(&self) {
        self.cancel.cancel();
        self.work.close();
        self.work.wait().await;
    }

    pub async fn new(
        store: Arc<dyn MetaStore>,
        leader: Arc<astersql_pd_member::Leadership>,
        cancel: &tokio_util::sync::CancellationToken,
        cluster: &Arc<ClusterHost>,

        id: u64,
        groups_enabled: bool,
    ) -> Result<Self, String> {
        let allocator = Arc::new(
            astersql_pd_id::Allocator::with_namespace(
                store.clone(),
                leader.clone(),
                cancel.clone(),
                100,
                astersql_pd_id::AllocatorNamespace::Keyspace,
            )
            .map_err(|e| e.to_string())?,
        );
        let store: Arc<dyn MetaStore> = Arc::new(crate::member_role_worker::TermStore::new(
            store,
            leader,
            cancel.clone(),
        ));
        let paths = ClusterPaths::new(id);
        let config = cluster.keyspace_configuration().await?;
        let groups = groups_enabled
            .then(|| Arc::new(KeyspaceGroupManager::new(store.clone(), paths.clone())));
        let meta_groups = Arc::new(MetaServiceGroupManager::new(
            store.clone(),
            paths.clone(),
            config
                .keyspace
                .meta_service_groups
                .clone()
                .unwrap_or_default(),
        ));
        let split_config = Arc::new(RwLock::new(KeyspaceSplitConfig {
            interval: std::time::Duration::from_nanos(
                config.keyspace.check_region_split_interval.as_nanos() as u64,
            ),
            timeout: std::time::Duration::from_nanos(
                config.keyspace.wait_region_split_timeout.as_nanos() as u64,
            ),
        }));
        let split = KeyspaceRegionSplit::with_labeler(
            Some(Arc::new(Labels(Arc::downgrade(cluster)))),
            Some(cluster.keyspace_regions().await?),
            split_config.clone(),
        );
        let bound = if config.pd_server.key_type == "raw" {
            BoundType::Raw
        } else {
            BoundType::Txn
        };
        let creation_config = Arc::new(RwLock::new(KeyspaceCreateConfig {
            wait_region_split: config.keyspace.wait_region_split,
            bound,
        }));
        let manager = Arc::new(KeyspaceManager::with_resources(
            astersql_pd_storage_endpoint::StorageEndpoint::new(store).keyspace_storage(paths),
            KeyspaceCreateResources {
                allocator: Some(allocator),
                splitter: Some(Arc::new(split)),
                groups: groups.clone(),
                meta_groups: Some(meta_groups),
                config: creation_config.clone(),
                ..Default::default()
            },
        ));
        Ok(Self {
            cancel: cancel.clone(),
            preallocations: config.keyspace.pre_alloc.clone().unwrap_or_default(),
            errors: Arc::new(std::sync::Mutex::new(Vec::new())),
            work: tokio_util::task::TaskTracker::new(),
            manager,
            groups,
            initialized: tokio::sync::OnceCell::new(),
            creation_config,
            split_config,
        })
    }
    pub async fn refresh(&self, cluster: &ClusterHost) -> Result<(), String> {
        let c = cluster.keyspace_configuration().await?;
        self.creation_config.write().unwrap().wait_region_split = c.keyspace.wait_region_split;
        *self.split_config.write().unwrap() = KeyspaceSplitConfig {
            interval: std::time::Duration::from_nanos(
                c.keyspace.check_region_split_interval.as_nanos() as u64,
            ),
            timeout: std::time::Duration::from_nanos(
                c.keyspace.wait_region_split_timeout.as_nanos() as u64,
            ),
        };
        Ok(())
    }
    pub async fn initialize(&self) -> Result<(), String> {
        self.initialized
            .get_or_try_init(|| async {
                if let Some(groups) = &self.groups {
                    groups.bootstrap().await.map_err(|e| e.to_string())?;
                }
                Ok::<_, String>(())
            })
            .await
            .map(|_| ())
    }
    pub fn record_error(&self, error: String) {
        self.errors.lock().unwrap().push(error);
    }
    pub async fn bootstrap(&self) -> Result<(), String> {
        self.initialize().await?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs() as i64;
        let work = self
            .manager
            .bootstrap_with_preallocations(now, &self.preallocations)
            .await
            .map_err(|e| e.to_string())?;
        for name in work.preallocations {
            let manager = self.manager.clone();
            let cancel = self.cancel.clone();
            let errors = self.errors.clone();
            self.work.spawn(async move {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(now);
                if let Err(error) = manager.preallocate(&cancel, &name, now).await {
                    errors
                        .lock()
                        .unwrap()
                        .push(format!("preallocate {name}: {error}"));
                }
            });
        }
        Ok(())
    }
}
