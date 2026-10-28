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
    allocator: Allocator,
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
    async fn healthy(&self) -> Result<(), String> {
        if self.cancel.is_cancelled() || self.leader.is_expired() {
            return Err("node stopped or leadership expired".into());
        }
        self.leader.validate().await.map_err(|e| e.to_string())
    }
    async fn alloc_id(&self) -> Result<u64, String> {
        self.requests.inc();
        self.allocator.alloc().await.map_err(|e| e.to_string())
    }
    async fn timestamp(&self, count: u32) -> Result<Timestamp, String> {
        self.requests.inc();
        self.oracle.allocate(count).await.map_err(|e| e.to_string())
    }
}
/// Starts services on the host's executor without installing process-global state.
pub struct PdNodeBuilder {
    runtime: Handle,
    cluster: astersql_pd_core::BasicCluster,
    cancellation: CancellationToken,
}
impl PdNodeBuilder {
    pub fn new(runtime: Handle, cancellation: CancellationToken) -> Self {
        Self {
            runtime,
            cluster: astersql_pd_core::BasicCluster::new(),
            cancellation: cancellation.child_token(),
        }
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
) -> Result<PdNode, NodeError> {
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
    let services = Arc::new(crate::member_role_worker::RoleServices::new(config.clone()));
    let initial = match member.campaign(5).await {
        Ok(leader) => {
            record(&events, "leadership");
            Some(
                prepare_term(
                    store.clone(),
                    config.clone(),
                    registry.clone(),
                    cancel.child_token(),
                    cluster,
                    Arc::new(leader),
                )
                .await?,
            )
        }
        Err(astersql_pd_member::MemberError::Contended) => None,
        Err(error) => return Err(error.into()),
    };
    services.replace(initial.clone());
    record(&events, "services");
    let router = astersql_pd_server::router(services.clone(), cancel.clone());
    record(&events, "listener");
    let (tx, rx) = oneshot::channel();
    let worker_events = events.clone();
    let worker_cancel = cancel.clone();
    let cluster_view = services.clone();
    let worker = tokio::spawn(async move {
        let server = axum::serve(listener, router)
            .with_graceful_shutdown(worker_cancel.clone().cancelled_owned());
        let mut server = Box::pin(std::future::IntoFuture::into_future(server));
        record(&worker_events, "ready");
        let _ = tx.send(());
        let roles = crate::member_role_worker::run(
            services.clone(),
            member,
            store.clone(),
            config,
            registry.clone(),
            worker_cancel.clone(),
        );
        tokio::pin!(roles);
        let (result, finished) = tokio::select! {
            result = &mut server => (result.map_err(Into::into), true),
            result = &mut roles => (result, false),
        };
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
                    _ = cancel.cancelled() => return Err("cluster recovery cancelled".into()),
                    result = &mut recovery => break result?,
                    _ = renew.tick() => leader.renew().await?,
                }
            }
        };
        leader.validate().await?;
        let allocator = Allocator::new(store.clone(), leader.clone(), cancel.clone(), 0)?;
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
            cancel.clone(),
            &registry,
            leader.clone(),
        );
        tokio::pin!(creation);
        let mut renew = tokio::time::interval(std::time::Duration::from_secs(1));
        let created = loop {
            tokio::select! { biased;
                _ = cancel.cancelled() => break Err("cluster initialization cancelled".into()),
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
            cancel.cancel();
            leader.resign().await?;
            return Err(error);
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
