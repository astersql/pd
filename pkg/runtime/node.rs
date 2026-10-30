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
    identity: Identity,
    pub(crate) cluster: Arc<crate::cluster_host::ClusterHost>,
    pub(crate) leader: Arc<Leadership>,
    allocator: Arc<Allocator>,
    pub(crate) oracle: Oracle,
    pub(crate) requests: prometheus::IntCounter,
    pub(crate) cancel: CancellationToken,
}
#[async_trait::async_trait]
impl PdServices for Application {
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
    async fn timestamp(&self, count: u32) -> Result<Timestamp, String> {
        self.requests.inc();
        crate::tso::allocate(&self.oracle, &self.leader, count).await
    }
}
/// Starts services on the host's executor without installing process-global state.
pub struct PdNodeBuilder {
    runtime: Handle,
    cluster: astersql_pd_core::BasicCluster,
    diagnostics: Option<Arc<crate::etcd_diagnostics::EtcdMemberDiagnostics>>,
    member_admin: Option<Arc<crate::member_admin::EtcdMemberAdmin>>,
    cancellation: CancellationToken,
}
impl PdNodeBuilder {
    pub fn new(runtime: Handle, cancellation: CancellationToken) -> Self {
        Self {
            runtime,
            cluster: astersql_pd_core::BasicCluster::new(),
            diagnostics: None,
            member_admin: None,
            cancellation: cancellation.child_token(),
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
    pub fn with_cluster(mut self, cluster: astersql_pd_core::BasicCluster) -> Self {
        self.cluster = cluster;
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
        self.runtime
            .spawn(initialize_node(
                store,
                config,
                listener,
                registry,
                cancel,
                self.cluster,
                self.diagnostics,
                self.member_admin,
            ))
            .await?
    }
}
async fn initialize_node(
    store: Arc<dyn MetaStore>,
    config: NodeConfig,
    listener: TcpListener,
    registry: Arc<prometheus::Registry>,
    cancel: CancellationToken,
    cluster: astersql_pd_core::BasicCluster,
    diagnostics: Option<Arc<crate::etcd_diagnostics::EtcdMemberDiagnostics>>,
    member_admin: Option<Arc<crate::member_admin::EtcdMemberAdmin>>,
) -> Result<PdNode, NodeError> {
    let mut config = config;
    config.client_url = crate::net_address::Endpoint::parse(&config.client_url)?
        .advertised_url(listener.local_addr()?)?;
    config.peer_url = crate::net_address::Endpoint::parse(&config.peer_url)?.url();
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
    let services = Arc::new(crate::member_role_worker::RoleServices::new(
        config.clone(),
        store.clone(),
    ));
    services.set_member_admin(member_admin.clone());
    let initial = if !services.eligible().await? {
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
    let router = astersql_pd_server::router(services.clone(), cancel.clone());
    record(&events, "listener");
    let (tx, rx) = oneshot::channel();
    let worker_events = events.clone();
    let worker_cancel = cancel.clone();
    let cluster_view = services.clone();
    let worker = tokio::spawn(async move {
        let server = axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(worker_cancel.clone().cancelled_owned());
        let mut server = Box::pin(std::future::IntoFuture::into_future(server));
        // Poll the serving future before publishing readiness. A listener that
        // terminates immediately must never acknowledge initialization.
        let first = std::future::poll_fn(|cx| {
            std::task::Poll::Ready(std::future::Future::poll(server.as_mut(), cx))
        })
        .await;
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
        let (result, finished) = tokio::select! {
            _ = &mut membership => {
                worker_cancel.cancel();
                let result = (&mut roles).await;
                (result, false)
            },
            result = &mut server => (result.map_err(Into::into), true),
            result = &mut roles => (result, false),
        };
        services.set_listening(false);
        worker_cancel.cancel();
        record(&worker_events, "stop-admission");
        if finished {
            let _ = (&mut roles).await;
        }
        let drained = if finished {
            Ok(())
        } else {
            (&mut server).await
        };
        drop(server);
        record(&worker_events, "background-joined");
        record(&worker_events, "leadership-released");
        drop(membership);
        drop((services, store));
        record(&worker_events, "storage-released");
        result?;
        drained?;
        Ok(())
    });
    Ok(PdNode {
        cancellation: cancel,
        cluster: cluster_view,
        ready: Some(rx),
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
) -> Result<Arc<Application>, NodeError> {
    let setup = async {
        let (cluster, cluster_config, restored_config) = {
            let recovery = crate::cluster_restore::restore(
                cluster,
                Arc::new(crate::member_role_worker::TermStore::new(
                    store.clone(),
                    leader.clone(),
                    cancel.clone(),
                )),
                &config,
                &cancel,
            );
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
        let oracle = Oracle::new(
            store.clone(),
            leader.clone(),
            Arc::new(SystemClock),
            cancel.clone(),
            3000,
        )
        .await?;
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
                registry.unregister(Box::new(requests))?;
                return Err(error);
            }
        };
        Ok::<_, NodeError>(Arc::new(Application {
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
            leader.resign().await?;
            return Err(if interrupted {
                astersql_pd_member::MemberError::Cancelled.into()
            } else {
                error
            });
        }
    };
    Ok(application)
}
/// Owns the serving scope. Shutdown joins admission and maintenance before releasing leadership.
pub struct PdNode {
    cluster: Arc<crate::member_role_worker::RoleServices>,
    cancellation: CancellationToken,
    ready: Option<oneshot::Receiver<()>>,
    worker: Option<JoinHandle<Result<(), NodeError>>>,
    events: Events,
}
impl PdNode {
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
        if let Some(ready) = self.ready.take() {
            ready.await?;
        }
        if self.cancellation.is_cancelled() {
            return Err("node is stopped".into());
        }
        Ok(())
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
