// Copyright 2026 AsterSQL.
use astersql_pd_id::Allocator;
use astersql_pd_member::{Leadership, Member};
use astersql_pd_server::{Identity, PdServices};
use astersql_pd_storage::MetaStore;
use astersql_pd_tso::{Oracle, SystemClock, Timestamp};
use prost::Message;
use std::sync::{Arc, Mutex};
use tokio::{net::TcpListener, runtime::Handle, sync::oneshot, task::JoinHandle};
use tokio_util::sync::CancellationToken;

pub type NodeConfig = Identity;
/// Loads or atomically creates the durable cluster identity for standalone and embedded hosts.
pub async fn bootstrap_config(
    store: &dyn MetaStore,
    name: String,
    client_url: String,
    peer_url: String,
) -> Result<NodeConfig, NodeError> {
    let client_endpoint = crate::net_address::Endpoint::parse(&client_url)?;
    client_endpoint.validate_advertised()?;
    let client_url = client_endpoint.url();
    let peer_url = crate::net_address::Endpoint::parse(&peer_url)?.url();
    use astersql_pd_storage::{Compare, Consistency, Mutation, Read};
    let key = b"/pd/cluster_id";
    let proposed = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
            & u128::from(u64::MAX),
    )?
    .max(1);
    store
        .transact(
            vec![Compare::CreateRevision(key.to_vec(), 0)],
            vec![Mutation::put(key, &proposed.to_be_bytes())],
        )
        .await?;
    let snapshot = store
        .read(Read::prefix(key, Consistency::Linearizable))
        .await?;
    let entry = snapshot
        .entries
        .iter()
        .find(|e| e.key == key)
        .ok_or("cluster identity missing")?;
    let cluster_id = u64::from_be_bytes(entry.value.as_slice().try_into()?);
    // Deterministic identity is persisted and checked by Member::bootstrap on every restart.
    let member_id = name
        .bytes()
        .fold(14695981039346656037u64, |hash, b| {
            (hash ^ u64::from(b)).wrapping_mul(1099511628211)
        })
        .max(1);
    Ok(NodeConfig {
        cluster_id,
        member_id,
        name,
        client_url,
        peer_url,
    })
}
pub type NodeError = Box<dyn std::error::Error + Send + Sync>;
type Events = Arc<Mutex<Vec<&'static str>>>;
fn record(events: &Events, event: &'static str) {
    events.lock().unwrap().push(event);
}

pub(crate) struct Application {
    pub(crate) encryption: Option<std::sync::Weak<crate::encryption::EncryptionResources>>,
    identity: Identity,
    pub(crate) cluster: Arc<crate::cluster_host::ClusterHost>,
    pub(crate) leader: Arc<Leadership>,
    pub(crate) gc: crate::control_gc::GCResources,
    pub(crate) keyspaces: crate::control_keyspace::KeyspaceResources,
    allocator: Arc<Allocator>,
    pub(crate) oracle: Option<Oracle>,
    pub(crate) requests: prometheus::IntCounter,
    pub(crate) cancel: CancellationToken,
}
#[async_trait::async_trait]
impl PdServices for Application {
    async fn create_keyspace(
        &self,
        request: astersql_pd_keyspace::CreateKeyspaceRequest,
        cancel: CancellationToken,
    ) -> Result<astersql_pd_storage_domain::Keyspace, astersql_pd_keyspace::KeyspaceError> {
        let manager = self
            .keyspace_manager()
            .await
            .map_err(astersql_pd_keyspace::KeyspaceError::Validation)?;
        self.keyspaces
            .track(async move { manager.create(&cancel, request).await })
            .await
    }
    async fn create_keyspace_by_id(
        &self,
        request: astersql_pd_keyspace::CreateKeyspaceByIdRequest,
        cancel: CancellationToken,
    ) -> Result<astersql_pd_storage_domain::Keyspace, astersql_pd_keyspace::KeyspaceError> {
        let manager = self
            .keyspace_manager()
            .await
            .map_err(astersql_pd_keyspace::KeyspaceError::Validation)?;
        self.keyspaces
            .track(async move { manager.create_by_id(&cancel, request).await })
            .await
    }

    async fn keyspace_manager(&self) -> Result<Arc<astersql_pd_keyspace::KeyspaceManager>, String> {
        self.healthy().await?;
        self.keyspaces.refresh(&self.cluster).await?;
        if self.cluster.keyspace_bootstrapped().await {
            self.keyspaces.initialize().await?;
        }
        Ok(self.keyspaces.manager.clone())
    }
    async fn meta_service_group_manager(
        &self,
    ) -> Result<Arc<astersql_pd_keyspace::MetaServiceGroupManager>, String> {
        self.healthy().await?;
        self.keyspaces
            .manager
            .meta_service_group_manager()
            .ok_or("meta-service groups manager is not initialized".into())
    }
    async fn patch_meta_service_groups(
        &self,
        patch: std::collections::BTreeMap<String, Option<String>>,
        cancel: CancellationToken,
    ) -> Result<(), (u16, String)> {
        self.healthy().await.map_err(|e| (500, e))?;
        let _update = self.keyspaces.meta_update.lock().await;
        let manager = self
            .keyspaces
            .manager
            .meta_service_group_manager()
            .ok_or((500, "meta-service groups manager is not initialized".into()))?;
        let mut groups = manager.get_groups().await;
        let mut deleted = Vec::new();
        for (id, value) in patch {
            if let Some(value) = value {
                groups.insert(id, value);
            } else {
                groups.remove(&id);
                deleted.push(id);
            }
        }
        let health = crate::meta_group_http::Health(
            self.cluster.clone(),
            cancel.clone(),
            self.cancel.clone(),
        );
        let outcome = manager
            .update_groups_safely(
                groups,
                &deleted,
                &health,
                |groups| async {
                    if cancel.is_cancelled() {
                        return Err("context canceled".into());
                    }
                    // Once admitted, complete durable publication even if the HTTP caller leaves.
                    self.cluster.persist_meta_groups(groups).await
                },
                || {},
            )
            .await
            .map_err(crate::meta_group_http::update_error)?;
        if let Some(error) = outcome.cleanup_error {
            self.keyspaces.record_error(error.to_string());
        }
        Ok(())
    }
    async fn keyspace_group_manager(
        &self,
    ) -> Result<Arc<astersql_pd_keyspace::KeyspaceGroupManager>, String> {
        self.healthy().await?;
        self.keyspaces.initialize().await?;
        self.keyspaces
            .groups
            .clone()
            .ok_or("keyspace group manager is not initialized".into())
    }

    async fn gc_manager(&self) -> Result<Arc<astersql_pd_gc::GCStateManager>, String> {
        self.healthy().await?;
        Ok(self.gc.manager.clone())
    }

    async fn abort_unsafe_recovery(&self) -> Result<(), String> {
        self.cluster.abort_unsafe_recovery().await
    }
    async fn show_unsafe_recovery(
        &self,
    ) -> Result<Vec<astersql_pd_unsaferecovery::StageOutput>, String> {
        self.cluster.show_unsafe_recovery().await
    }
    async fn start_unsafe_recovery(
        &self,
        request: astersql_pd_unsaferecovery::StartRequest,
    ) -> Result<(), String> {
        self.cluster.start_unsafe_recovery(request).await
    }
    async fn replication_mode(
        &self,
    ) -> Result<Option<astersql_pd_replication::ModeSnapshot>, String> {
        self.cluster.replication_snapshot().await
    }
    async fn report_store_dr_status(
        &self,
        store: u64,
        status: astersql_pd_replication::StoreDRStatus,
    ) -> Result<(), String> {
        self.cluster.report_store_dr_status(store, status).await
    }

    fn identity(&self) -> &Identity {
        &self.identity
    }
    fn store_heartbeat_service(
        &self,
    ) -> Option<&dyn astersql_pd_server::store_heartbeat_adapter::StoreHeartbeatService> {
        Some(self.cluster.as_ref())
    }
    fn region_heartbeat_service(
        &self,
    ) -> Option<&dyn astersql_pd_server::region_heartbeat_adapter::RegionHeartbeatService> {
        Some(self.cluster.as_ref())
    }
    fn bucket_heartbeat_service(
        &self,
    ) -> Option<&dyn astersql_pd_server::bucket_heartbeat_adapter::BucketHeartbeatService> {
        Some(self.cluster.as_ref())
    }
    fn cluster_read_service(&self) -> Option<&dyn astersql_pd_server::ClusterReadService> {
        Some(self.cluster.as_ref())
    }
    fn region_sync_handle(&self) -> Option<Arc<astersql_pd_server::sync_transport::RegionSyncer>> {
        self.cluster.region_sync_handle()
    }
    async fn external_timestamp(&self) -> Result<u64, String> {
        self.cluster.external_timestamp().await
    }
    async fn set_external_timestamp(&self, timestamp: u64) -> Result<(), String> {
        self.cluster.set_external_timestamp(timestamp).await
    }
    async fn resolved_ts_observation(
        &self,
    ) -> Result<astersql_pd_server::http_resolved_ts::Observation, String> {
        self.cluster.resolved_ts_observation().await
    }
    async fn report_min_resolved_ts(&self, store_id: u64, ts: u64) -> Result<(), String> {
        self.cluster.report_min_resolved_ts(store_id, ts).await
    }
    async fn put_store(
        &self,
        store: astersql_pd_core::StoreMeta,
    ) -> Result<(), astersql_pd_server::grpc_store::StoreError> {
        self.cluster.register_store(store).await
    }
    async fn mutate_store(
        &self,
        operation: astersql_pd_server::http_store_state::StoreMutation,
    ) -> Result<(), String> {
        self.cluster.mutate_store(operation).await
    }
    async fn store_progress(
        &self,
        id: Option<u64>,
        action: String,
    ) -> Result<serde_json::Value, String> {
        self.cluster.store_progress(id, action).await
    }
    async fn stores(&self) -> Result<Vec<astersql_pd_core::StoreMeta>, String> {
        self.cluster.registered_stores().await
    }
    async fn split_regions(
        &self,
        request: astersql_pd_server::grpc_split_regions::SplitRequest,
    ) -> Result<astersql_pd_server::grpc_split_regions::SplitResult, String> {
        self.cluster.split_regions(request).await
    }
    async fn scatter(
        &self,
        request: astersql_pd_server::grpc_scatter::ScatterRequest,
    ) -> Result<
        astersql_pd_server::grpc_scatter::ScatterResult,
        astersql_pd_server::grpc_scatter::ScatterError,
    > {
        self.cluster.scatter(request).await
    }
    async fn accelerate_regions(
        &self,
        request: astersql_pd_server::http_region_schedule::AccelerateRequest,
    ) -> Result<(), String> {
        self.cluster.accelerate_regions(request).await
    }
    async fn checker(
        &self,
        command: astersql_pd_server::http_checker::Command,
    ) -> Result<serde_json::Value, String> {
        self.cluster.checker(command).await
    }
    async fn scheduler_config(
        &self,
        request: axum::http::Request<axum::body::Body>,
    ) -> Result<axum::response::Response, String> {
        self.cluster.scheduler_config(request).await
    }
    async fn scheduler(
        &self,
        command: astersql_pd_server::http_scheduler::Command,
    ) -> Result<(u16, serde_json::Value), String> {
        self.cluster.scheduler(command).await
    }
    async fn operator_write(
        &self,
        command: astersql_pd_server::http_operator_write::Command,
    ) -> Result<(u16, serde_json::Value), String> {
        self.cluster.operator_write(command).await
    }
    async fn operator_read(
        &self,
        query: astersql_pd_server::http_operator_read::Query,
    ) -> Result<serde_json::Value, String> {
        self.cluster.operator_read(query).await
    }
    async fn operator(
        &self,
        id: u64,
    ) -> Result<Option<astersql_pd_server::grpc_scatter::OperatorResult>, String> {
        self.cluster.operator(id).await
    }
    async fn healthy(&self) -> Result<(), String> {
        if self.cancel.is_cancelled() || self.leader.is_expired() {
            return Err("node stopped or leadership expired".into());
        }
        self.leader.validate().await.map_err(|e| e.to_string())
    }
    async fn batch_split_version(&self) -> Result<Option<String>, String> {
        self.cluster.batch_split_version().await
    }
    async fn split_ids(
        &self,
        region: astersql_pd_core::RegionMeta,
        count: u32,
        batch: bool,
        reason: i32,
    ) -> Result<
        astersql_pd_server::grpc_ask_split::SplitIds,
        astersql_pd_server::grpc_ask_split::SplitError,
    > {
        self.cluster.split_ids(region, count, batch, reason).await
    }
    async fn recover_alloc_id(&self, id: u64) -> Result<(), String> {
        self.allocator
            .set_base(id)
            .await
            .map_err(|error| match error {
                astersql_pd_id::AllocatorError::Exhausted => {
                    "[PD:idalloc:ErrIDExhausted]id exhausted".into()
                }
                other => other.to_string(),
            })
    }
    async fn alloc_id(&self) -> Result<u64, String> {
        self.requests.inc();
        self.allocator.alloc().await.map_err(|e| e.to_string())
    }
    async fn alloc_ids(&self, count: u32) -> Result<(u64, u32), String> {
        self.requests.inc();
        self.allocator
            .alloc_count(count)
            .await
            .map_err(|error| match error {
                astersql_pd_id::AllocatorError::Exhausted => {
                    "[PD:idalloc:ErrIDExhausted]id exhausted".into()
                }
                other => other.to_string(),
            })
    }
    async fn reset_timestamp(&self, tso: u64, force: bool) -> Result<(), String> {
        let config = self.cluster.configuration().await?;
        self.oracle.as_ref().ok_or("tso service is not initialized")?
            .reset(tso, force, config.pd_server.max_reset_ts_gap.as_nanos() / 1_000_000)
            .await
            .map_err(|e| match e {
            astersql_pd_tso::TsoError::WindowLost => "[PD:etcd:ErrEtcdTxnConflict]etcd transaction failed, conflicted and rolled back".into(),
            other => other.to_string(),
        })
    }
    async fn timestamp(&self, count: u32) -> Result<Timestamp, String> {
        self.requests.inc();
        crate::tso::allocate(
            self.oracle
                .as_ref()
                .ok_or("tso service is not initialized")?,
            &self.leader,
            count,
        )
        .await
    }
}
/// Starts services on the host's executor without installing process-global state.
pub struct PdNodeBuilder {
    keyspace_groups: bool,
    runtime: Handle,
    cluster: astersql_pd_core::BasicCluster,
    diagnostics: Option<Arc<crate::etcd_diagnostics::EtcdMemberDiagnostics>>,
    member_admin: Option<Arc<crate::member_admin::EtcdMemberAdmin>>,
    cancellation: CancellationToken,
    listener_config: astersql_pd_server_config::Config,
    telemetry_config: Option<astersql_pd_server_config::Config>,
    join: String,
    log_level: Option<Arc<dyn astersql_pd_logutil::LogLevelControl>>,
}
impl PdNodeBuilder {
    pub fn new(runtime: Handle, cancellation: CancellationToken) -> Self {
        Self {
            keyspace_groups: false,
            runtime,
            cluster: astersql_pd_core::BasicCluster::new(),
            diagnostics: None,
            member_admin: None,
            cancellation: cancellation.child_token(),
            listener_config: Default::default(),
            telemetry_config: None,
            join: String::new(),
            log_level: None,
        }
    }
    /// Bind diagnostics to an explicitly configured independent etcd member.
    pub fn with_etcd_member_diagnostics(
        mut self,
        diagnostics: Arc<crate::etcd_diagnostics::EtcdMemberDiagnostics>,
    ) -> Self {
        self.diagnostics = Some(diagnostics);
        self
    }
    /// Explicitly dedicate an independent etcd member to this PD identity.
    pub fn with_etcd_member_admin(
        mut self,
        admin: Arc<crate::member_admin::EtcdMemberAdmin>,
    ) -> Self {
        self.member_admin = Some(admin);
        self
    }
    /// Injects the canonical cluster rather than creating adapter-local caches.
    /// Enable the same group manager used by API-mode keyspace operations.
    pub fn with_keyspace_groups(mut self, enabled: bool) -> Self {
        self.keyspace_groups = enabled;
        self
    }
    pub fn with_cluster(mut self, cluster: astersql_pd_core::BasicCluster) -> Self {
        self.cluster = cluster;
        self
    }
    /// Configures inbound TLS and host metric query/push transports.
    pub fn with_listener_config(mut self, config: astersql_pd_server_config::Config) -> Self {
        self.listener_config = config;
        self
    }
    /// Configure metric destinations and outbound credentials independently of
    /// inbound listener policy. Defaults to the canonical listener configuration.
    pub fn with_telemetry_config(mut self, config: astersql_pd_server_config::Config) -> Self {
        self.telemetry_config = Some(config);
        self
    }
    /// Supplies host-owned log filtering without installing any global subscriber.
    pub fn with_log_level_control(
        mut self,
        control: Arc<dyn astersql_pd_logutil::LogLevelControl>,
    ) -> Self {
        self.log_level = Some(control);
        self
    }
    /// Requests fresh admission against the explicitly supplied external MetaStore.
    /// Empty means bootstrap/restart, preserving the durable member identity.
    pub fn with_join(mut self, join: impl Into<String>) -> Self {
        self.join = join.into();
        self
    }
    /// Reserves leadership and a durable TSO window before accepting traffic.
    pub async fn start(
        self,
        store: Arc<dyn MetaStore>,
        config: NodeConfig,
        listener: TcpListener,
        registry: Arc<prometheus::Registry>,
    ) -> Result<PdNode, NodeError> {
        // Initialization runs on the supplied executor, including when the caller uses another one.
        let cancel = self.cancellation.clone();
        // Cancellation follows the caller's start future, including timeout/drop.
        let mut guard = StartGuard(Some(cancel.clone()));
        let result = self
            .runtime
            .spawn(initialize_node(
                store,
                config,
                listener,
                registry,
                cancel,
                self.cluster,
                self.keyspace_groups,
                self.diagnostics,
                self.member_admin,
                self.listener_config,
                self.telemetry_config,
                self.join,
                self.log_level,
            ))
            .await?;
        if result.is_ok() {
            guard.0 = None;
        }
        result
    }
}
struct StartGuard(Option<CancellationToken>);
impl Drop for StartGuard {
    fn drop(&mut self) {
        if let Some(cancel) = &self.0 {
            cancel.cancel();
        }
    }
}
async fn initialize_node(
    store: Arc<dyn MetaStore>,
    config: NodeConfig,
    listener: TcpListener,
    registry: Arc<prometheus::Registry>,
    cancel: CancellationToken,
    cluster: astersql_pd_core::BasicCluster,
    keyspace_groups: bool,
    diagnostics: Option<Arc<crate::etcd_diagnostics::EtcdMemberDiagnostics>>,
    member_admin: Option<Arc<crate::member_admin::EtcdMemberAdmin>>,
    listener_config: astersql_pd_server_config::Config,
    telemetry_config: Option<astersql_pd_server_config::Config>,
    join: String,
    log_level: Option<Arc<dyn astersql_pd_logutil::LogLevelControl>>,
) -> Result<PdNode, NodeError> {
    let join_config = config.clone();
    let mut config = config;
    config.client_url = crate::net_address::Endpoint::parse(&config.client_url)?
        .advertised_url(listener.local_addr()?)?;
    let listener =
        crate::tls_listener::Listener::new(listener, &config.client_url, &listener_config)?;
    config.peer_url = crate::net_address::Endpoint::parse(&config.peer_url)?.url();
    crate::join_preflight::check_advertised(
        store.as_ref(),
        &join_config,
        &join,
        &config.client_url,
        &cancel,
    )
    .await?;
    // Detect collector ownership conflicts before any persistent member binding.
    let telemetry_config = telemetry_config.unwrap_or_else(|| listener_config.clone());
    let metrics = astersql_pd_server::service_metrics::ServiceMetrics::register(registry.clone())?;
    let store: Arc<dyn MetaStore> = Arc::new(crate::store_metrics::MeteredStore::new(
        store,
        &metrics.classic,
    ));
    cluster.regions.set_metrics(metrics.core.clone());
    cluster
        .stores
        .set_filter_metrics(metrics.store_metric_sink());
    metrics.prepare_transport(&telemetry_config)?;
    metrics.set_cluster(config.cluster_id);
    if let Some(diagnostics) = &diagnostics {
        tokio::select! { biased;
            _ = cancel.cancelled() => return Err("node is stopped".into()),
            result = diagnostics.bind(store.as_ref(), config.cluster_id, config.member_id) => result?,
        }
    }
    tokio::select! { biased;
        _ = cancel.cancelled() => return Err("node is stopped".into()),
        result = crate::member_admin::ensure_admitted(store.as_ref(), config.cluster_id, config.member_id) => result?,
    }
    if let Some(admin) = &member_admin {
        tokio::select! { biased;
            _ = cancel.cancelled() => return Err("node is stopped".into()),
            result = admin.bind(store.as_ref(), config.cluster_id, config.member_id) => result?,
        }
    }
    if !join.is_empty() {
        tokio::select! { biased;
            _ = cancel.cancelled() => return Err("node is stopped".into()),
            result = crate::join_resume::register(store.as_ref(), &config) => result?,
        }
    }
    // A restart without --join can also finish a previously interrupted admission.
    let unfinished_join = !join.is_empty()
        || tokio::select! { biased;
            _ = cancel.cancelled() => return Err("node is stopped".into()),
            result = crate::join_resume::pending(store.as_ref(), &config) => result?,
        };
    let events = Arc::new(Mutex::new(vec!["storage"]));
    let member = Member::bootstrap(
        store.clone(),
        cancel.clone(),
        config.cluster_id,
        config.member_id,
        astersql_pd_proto::pdpb::Member {
            name: config.name.clone(),
            member_id: config.member_id,
            client_urls: vec![config.client_url.clone()],
            peer_urls: vec![config.peer_url.clone()],
            ..Default::default()
        }
        .encode_to_vec(),
    )
    .await?;
    let encryption = crate::encryption::EncryptionResources::start(
        store.clone(),
        listener_config.security.encryption.clone(),
        &cancel,
    )
    .await?;
    let services = Arc::new(crate::member_role_worker::RoleServices::new(
        config.clone(),
        store.clone(),
    ));
    let _ = services.encryption.set(Arc::downgrade(&encryption));
    services
        .keyspace_groups
        .store(keyspace_groups, std::sync::atomic::Ordering::Release);
    services.set_telemetry(Arc::new(telemetry_config.clone()), &metrics);
    if !listener_config.scalars.data_dir.is_empty() {
        let _ = services
            .file_directory
            .set(std::path::PathBuf::from(&listener_config.scalars.data_dir));
    }

    services.set_log_level_control(log_level);
    services.set_member_admin(member_admin.clone());
    let eligible = tokio::select! { biased;
        _ = cancel.cancelled() => return Err("node is stopped".into()),
        result = services.eligible() => result?,
    };
    let initial = if !eligible {
        None
    } else {
        match member.campaign(5).await {
            Ok(leader) => {
                record(&events, "leadership");
                match prepare_term(
                    store.clone(),
                    config.clone(),
                    registry.clone(),
                    cancel.child_token(),
                    cluster,
                    Arc::new(leader),
                    Some(metrics.clone()),
                    Some(Arc::new(telemetry_config.clone())),
                    Some(encryption.clone()),
                    services.recovery_steps.clone(),
                    keyspace_groups,
                )
                .await
                {
                    Ok(term) => Some(term),
                    Err(error)
                        if member_admin.is_some()
                            && crate::member_role_worker::retryable_role_error(&error) =>
                    {
                        None
                    }
                    Err(error) => return Err(error),
                }
            }
            Err(astersql_pd_member::MemberError::Contended) => None,
            Err(error) => return Err(error.into()),
        }
    };
    services.set_diagnostics(diagnostics);
    services.replace(initial.clone());
    record(&events, "services");
    let router =
        astersql_pd_server::router_with_metrics(services.clone(), cancel.clone(), metrics.clone());
    record(&events, "listener");
    let (tx, rx) = oneshot::channel();
    let worker_events = events.clone();
    let worker_cancel = cancel.clone();
    let cluster_view = services.clone();
    let worker = tokio::spawn(async move {
        let mut server = crate::tls_listener::serve(listener, router, worker_cancel.clone());
        // Poll the serving future before publishing readiness. A listener that
        // terminates immediately must never acknowledge initialization.
        let first = std::future::poll_fn(|cx| {
            std::task::Poll::Ready(std::future::Future::poll(server.as_mut(), cx))
        })
        .await;
        let completion = if first.is_pending() && !worker_cancel.is_cancelled() && unfinished_join {
            tokio::select! { biased;
                _ = worker_cancel.cancelled() => Err("node is stopped".into()),
                result = crate::join_resume::complete(store.as_ref(), &config) => result,
            }
        } else {
            Ok(())
        };
        if completion.is_err() {
            worker_cancel.cancel();
        }
        if first.is_pending() && !worker_cancel.is_cancelled() {
            services.set_listening(true);
            record(&worker_events, "ready");
            let _ = tx.send(());
        } else {
            drop(tx);
        }
        let mut server = Box::pin(async move {
            match first {
                std::task::Poll::Ready(result) => result,
                std::task::Poll::Pending => server.await,
            }
        });
        let member_cluster = config.cluster_id;
        let member_id = config.member_id;
        let roles = crate::member_role_worker::run(
            services.clone(),
            member,
            store.clone(),
            config,
            registry.clone(),
            worker_cancel.clone(),
        );
        tokio::pin!(roles);
        let monitor_store = store.clone();
        let monitor_cancel = worker_cancel.clone();
        let managed = member_admin.is_some();
        let mut membership = Box::pin(async move {
            let mut unavailable_since = None;
            loop {
                tokio::select! { biased;
                    _ = monitor_cancel.cancelled() => break,
                    _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
                }
                if let Err(error) = crate::member_admin::ensure_admitted(
                    monitor_store.as_ref(),
                    member_cluster,
                    member_id,
                )
                .await
                {
                    if error == "PD member has been revoked" {
                        break;
                    }
                    if managed {
                        // Keep admission fail-closed, but tolerate the brief
                        // linearizable-read interruption during consensus transfer.
                        let start = unavailable_since.get_or_insert_with(tokio::time::Instant::now);
                        if start.elapsed() >= std::time::Duration::from_secs(2) {
                            break;
                        }
                    }
                } else {
                    unavailable_since = None;
                }
            }
        });
        let push = astersql_pd_server::service_metrics::push_worker(
            registry.clone(),
            Arc::new(telemetry_config),
            rustix::system::uname()
                .nodename()
                .to_string_lossy()
                .into_owned(),
            worker_cancel.clone(),
        );
        let serving = async {
            let result = tokio::select! {
                _ = &mut membership => {
                    worker_cancel.cancel();
                    // Retirement may wait for backend IO. Keep polling the
                    // listener shutdown while joining roles so admission stops
                    // before a slow lease revoke can hold the node open.
                    let (role_result, server_result) = tokio::join!(&mut roles, &mut server);
                    (role_result.and_then(|()| server_result.map_err(Into::into)), true, true)
                },
                result = &mut server => (result.map_err(Into::into), true, false),
                result = &mut roles => (result, false, true),
            };
            worker_cancel.cancel();
            result
        };
        let monitor = metrics.run_worker(worker_cancel.clone());
        let ((result, finished, roles_finished), (), ()) = tokio::join!(serving, push, monitor);
        services.set_listening(false);
        worker_cancel.cancel();
        record(&worker_events, "stop-admission");
        if !roles_finished {
            let _ = (&mut roles).await;
        }
        let drained = if finished {
            Ok(())
        } else {
            (&mut server).await
        };
        drop(server);
        let encryption_cleanup = encryption.shutdown().await;
        record(&worker_events, "background-joined");
        record(&worker_events, "leadership-released");
        drop(membership);
        drop((services, store));
        record(&worker_events, "storage-released");
        // Admission failure is the initiating error; cleanup has already run.
        // Do not replace it with a secondary serving/retirement failure.
        completion?;
        encryption_cleanup?;
        result?;
        drained?;
        Ok(())
    });
    Ok(PdNode {
        cancellation: cancel,
        cluster: cluster_view,
        ready: crate::ready_gate::ReadyGate::new(rx),
        worker: Some(worker),
        events,
    })
}

pub(crate) async fn prepare_term(
    store: Arc<dyn MetaStore>,
    config: NodeConfig,
    registry: Arc<prometheus::Registry>,
    cancel: CancellationToken,
    cluster: astersql_pd_core::BasicCluster,
    leader: Arc<Leadership>,
    metrics: Option<Arc<astersql_pd_server::service_metrics::ServiceMetrics>>,
    transport_config: Option<Arc<astersql_pd_server_config::Config>>,
    encryption: Option<Arc<crate::encryption::EncryptionResources>>,
    recovery_steps: Arc<astersql_pd_unsaferecovery::RecoverySteps>,
    keyspace_groups: bool,
) -> Result<Arc<Application>, NodeError> {
    if let Some(metrics) = &metrics {
        leader.set_metrics(&metrics.classic);
    }
    let setup = async {
        let (cluster, cluster_config, restored_config) = {
            let recovery = async {
                if let Some(keys)=&encryption {keys.rotation.set_leadership(Some(leader.clone())).await?;}
                crate::cluster_restore::restore_with_encryption(
                cluster,
                Arc::new(crate::member_role_worker::TermStore::new(
                    store.clone(),
                    leader.clone(),
                    cancel.clone(),
                )),
                &config,
                &cancel,
                encryption.as_ref().map(|keys|keys.dictionary.clone()),
            ).await
            };
            tokio::pin!(recovery);
            let mut renew = tokio::time::interval(std::time::Duration::from_secs(1));
            // Large snapshots must not exhaust the election lease before serving starts.
            loop {
                tokio::select! { biased;
                    _ = cancel.cancelled() => return Err(astersql_pd_member::MemberError::Cancelled.into()),
                    result = &mut recovery => break result?,
                    _ = renew.tick() => leader.renew().await?,
                }
            }
        };
        leader.validate().await?;
        let allocator = Arc::new(Allocator::new(
            store.clone(),
            leader.clone(),
            cancel.clone(),
            0,
        )?);
        if let Some(metrics) = &metrics { allocator.set_metrics(metrics.classic.id_gauge.with_label_values(&["idalloc"])); }
        let oracle = if keyspace_groups { None } else { Some(Oracle::new_with_metrics(
            store.clone(),
            leader.clone(),
            Arc::new(SystemClock),
            cancel.clone(),
            3000,
            metrics.as_ref().map(|m| astersql_pd_tso::OracleMetrics::new(&m.classic)),
        )
        .await?) };
        let requests = prometheus::IntCounter::new(
            "pd_rust_requests_total",
            "Minimal Rust PD domain requests",
        )?;
        registry.register(Box::new(requests.clone()))?;
        let creation = crate::cluster_host::ClusterHost::new(
            cluster,
            cluster_config,
            restored_config,
            Arc::new(crate::member_role_worker::TermStore::new(
                store.clone(),
                leader.clone(),
                cancel.clone(),
            )),
            config.cluster_id,
            config.member_id,
            cancel.clone(),
            &registry,
            leader.clone(),
            allocator.clone(),
            metrics.as_ref().map(|m|m.domain_metrics()),
            encryption.clone(),
            recovery_steps.clone(),
        );
        tokio::pin!(creation);
        let mut renew = tokio::time::interval(std::time::Duration::from_secs(1));
        let created = loop {
            tokio::select! { biased;
                _ = cancel.cancelled() => break Err(astersql_pd_member::MemberError::Cancelled.into()),
                result = &mut creation => break result,
                _ = renew.tick() => {
                    if let Err(error) = leader.renew().await { break Err(error.into()); }
                }
            }
        };
        let cluster = match created {
            Ok(cluster) => Arc::new(cluster),
            Err(error) => {
                let cleanup = registry.unregister(Box::new(requests)).map_err(Into::into);
                return Err(crate::start_cleanup::preserve(error, cleanup));
            }
        };
        // Go reserves the next ID window after raft-cluster creation, before ready.
        // Keep the completed cluster owned while cancellation/renewal races this I/O.
        let reserved: Result<(),NodeError> = {
            let reservation=allocator.rebase();
            tokio::pin!(reservation);
            loop {
                tokio::select! {biased;
                    _=cancel.cancelled()=>break Err(astersql_pd_member::MemberError::Cancelled.into()),
                    result=&mut reservation=>break result.map_err(Into::into),
                    _=renew.tick()=>{if let Err(error)=leader.renew().await{break Err(error.into());}},
                }
            }
        };
        if let Err(error)=reserved {
            let cleanup=cluster.shutdown().await;
            let unregistered=registry.unregister(Box::new(requests)).map_err(Into::into);
            return Err(crate::start_cleanup::preserve(crate::start_cleanup::preserve(error,cleanup),unregistered));
        }
        cluster.set_replication_transport_config(transport_config).await;
        let keyspaces={
        let keyspace_setup=async {
            let resources=crate::control_keyspace::KeyspaceResources::new(store.clone(), leader.clone(), &cancel, &cluster, config.cluster_id, keyspace_groups).await?;
            if cluster.keyspace_bootstrapped().await {resources.initialize().await?;}
            Ok::<_,String>(resources)
        };
        tokio::pin!(keyspace_setup);
        loop {
            let result=tokio::select!{biased;
                _=cancel.cancelled()=>Err("operation cancelled".into()),
                result=&mut keyspace_setup=>result,
                _=renew.tick()=>{if let Err(error)=leader.renew().await{break Err(NodeError::from(error));}continue;}
            };
            break result.map_err(NodeError::from);
        }
        };
        let keyspaces=match keyspaces {
            Ok(resources)=>resources,
            Err(error)=>{
                let cleanup=cluster.shutdown().await;
                let unregistered=registry.unregister(Box::new(requests)).map_err(Into::into);
                return Err(crate::start_cleanup::preserve(crate::start_cleanup::preserve(error,cleanup),unregistered));
            }
        };
        let gc=crate::control_gc::GCResources::new(store.clone(), leader.clone(), &cancel, config.cluster_id, keyspaces.manager.clone()).await;
        if let Some(metrics) = &metrics {
            if let Err(error) = gc.manager.attach_gc_metrics(metrics.gc.clone()) {
                gc.shutdown().await;
                let cleanup = cluster.shutdown().await;
                let unregistered = registry.unregister(Box::new(requests)).map_err(Into::into);
                return Err(crate::start_cleanup::preserve(crate::start_cleanup::preserve(NodeError::from(error.to_string()), cleanup), unregistered));
            }
        }
        Ok::<_, NodeError>(Arc::new(Application {
            gc,
            keyspaces,
            encryption:encryption.as_ref().map(Arc::downgrade),
            cluster,
            identity: config,
            leader: leader.clone(),
            allocator,
            oracle,
            requests,
            cancel: cancel.clone(),
        }))
    }
    .await;
    let application = match setup {
        Ok(application) => application,
        Err(error) => {
            // Nested domain adapters may wrap cancellation transparently. Host
            // cancellation wins an in-flight initialization race, but cleanup
            // failures must still propagate before reporting graceful shutdown.
            let interrupted = cancel.is_cancelled();
            cancel.cancel();
            if let Some(keys) = &encryption {
                keys.rotation.clear_leadership_if(&leader).await;
            }
            let cleanup = leader.resign().await.map_err(Into::into);
            let error = if interrupted {
                astersql_pd_member::MemberError::Cancelled.into()
            } else {
                error
            };
            return Err(crate::start_cleanup::preserve(error, cleanup));
        }
    };
    Ok(application)
}
/// Owns the serving scope. Shutdown joins admission and maintenance before releasing leadership.
pub struct PdNode {
    cluster: Arc<crate::member_role_worker::RoleServices>,
    cancellation: CancellationToken,
    ready: crate::ready_gate::ReadyGate,
    worker: Option<JoinHandle<Result<(), NodeError>>>,
    events: Events,
}
impl PdNode {
    /// Return the current term's canonical keyspace manager without retaining a retired host.
    pub fn keyspace_manager(&self) -> Option<Arc<astersql_pd_keyspace::KeyspaceManager>> {
        self.cluster
            .current()
            .filter(|t| !t.cancel.is_cancelled() && !t.leader.is_expired())
            .map(|t| t.keyspaces.manager.clone())
    }
    /// The host-owned GC manager for the current serving term.
    pub fn gc_manager(&self) -> Option<Arc<astersql_pd_gc::GCStateManager>> {
        self.cluster
            .current()
            .filter(|term| !term.cancel.is_cancelled() && !term.leader.is_expired())
            .map(|term| term.gc.manager.clone())
    }
    /// Host read adapters share the active leader cache.
    /// Panics on a follower; use `try_cluster` when the node may change roles.
    pub fn cluster(&self) -> Arc<crate::cluster_host::ClusterHost> {
        self.try_cluster()
            .expect("cluster is available only during a leader term")
    }
    /// Returns the active leader cache; followers have no scheduling scope.
    pub fn try_cluster(&self) -> Option<Arc<crate::cluster_host::ClusterHost>> {
        self.cluster.current().map(|term| term.cluster.clone())
    }
    pub async fn ready(&mut self) -> Result<(), NodeError> {
        if let Err(error) = self.ready.wait().await {
            self.wait().await?;
            return Err(error.into());
        }
        loop {
            tokio::select! { biased;
                _ = self.cancellation.cancelled() => return Err("node is stopped".into()),
                status = self.cluster.probe_status() => {
                    let status = status?;
                    if !status.is_bootstrapped || status.region_loaded {
                        if let Some(term) = self.cluster.current() {
                            term.healthy().await?;
                        }
                        if self.cancellation.is_cancelled() {
                            return Err("node is stopped".into());
                        }
                        return Ok(());
                    }
                }
            }
            tokio::select! { biased;
                _ = self.cancellation.cancelled() => return Err("node is stopped".into()),
                _ = tokio::time::sleep(std::time::Duration::from_millis(10)) => {}
            }
        }
    }
    pub fn events(&self) -> Vec<&'static str> {
        self.events.lock().unwrap().clone()
    }
    /// Waits for host cancellation or a terminal listener/leadership/storage failure.
    /// Dropping this future leaves the worker available to shutdown.
    pub async fn wait(&mut self) -> Result<(), NodeError> {
        let result = match self.worker.as_mut() {
            Some(worker) => worker.await,
            None => return Ok(()),
        };
        self.worker = None;
        result?
    }
    pub async fn shutdown(&mut self) -> Result<(), NodeError> {
        self.cancellation.cancel();
        self.wait().await
    }
}
impl Drop for PdNode {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}
