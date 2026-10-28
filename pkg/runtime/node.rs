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

struct Application {
    identity: Identity,
    cluster: Arc<crate::cluster_host::ClusterHost>,
    leader: Arc<Leadership>,
    allocator: Allocator,
    oracle: Oracle,
    requests: prometheus::IntCounter,
    cancel: CancellationToken,
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
    async fn healthy(&self) -> Result<(), String> {
        if self.cancel.is_cancelled() {
            return Err("node stopped".into());
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
    let leader = Arc::new(member.campaign(5).await?);
    record(&events, "leadership");
    let setup = async {
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
        let cluster = match crate::cluster_host::ClusterHost::new(
            cluster,
            store.clone(),
            config.cluster_id,
            cancel.clone(),
            &registry,
        ) {
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
            leader.resign().await?;
            return Err(error);
        }
    };
    record(&events, "services");
    let router = astersql_pd_server::router(application.clone(), cancel.clone());
    record(&events, "listener");
    let (tx, rx) = oneshot::channel();
    let worker_events = events.clone();
    let worker_cancel = cancel.clone();
    let cluster_view = application.cluster.clone();
    let worker = tokio::spawn(async move {
        let server = axum::serve(listener, router)
            .with_graceful_shutdown(worker_cancel.clone().cancelled_owned());
        let mut server = Box::pin(std::future::IntoFuture::into_future(server));
        record(&worker_events, "ready");
        let _ = tx.send(());
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(50));
        let mut renew = tokio::time::interval(std::time::Duration::from_secs(1));
        let mut server_finished = false;
        let result: Result<(), NodeError> = loop {
            tokio::select! {
                biased;
                _ = worker_cancel.cancelled() => break Ok(()),
                result = &mut server => {
                    server_finished = true;
                    break result.map_err(Into::into);
                }
                _ = renew.tick() => {
                    if let Err(error) = leader.renew().await {
                        break if worker_cancel.is_cancelled() { Ok(()) } else { Err(error.into()) };
                    }
                }
                _ = tick.tick() => {
                    if let Err(error) = application.oracle.update().await {
                        break if worker_cancel.is_cancelled() { Ok(()) } else { Err(error.into()) };
                    }
                }
            }
        };
        worker_cancel.cancel();
        record(&worker_events, "stop-admission");
        // Supported response streams observe cancellation. Join all connections rather than
        // reporting successful shutdown while detached transport tasks still use storage.
        let drained = if server_finished {
            Ok(())
        } else {
            (&mut server).await
        };
        drop(server);
        let cluster_stopped = application.cluster.shutdown().await;
        record(&worker_events, "background-joined");
        let resignation = leader.resign().await;
        record(&worker_events, "leadership-released");
        let unregistered = registry.unregister(Box::new(application.requests.clone()));
        drop((application, leader, member, store));
        record(&worker_events, "storage-released");
        result?;
        cluster_stopped?;
        drained?;
        resignation?;
        unregistered?;
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
/// Owns the serving scope. Shutdown joins admission and maintenance before releasing leadership.
pub struct PdNode {
    cluster: Arc<crate::cluster_host::ClusterHost>,
    cancellation: CancellationToken,
    ready: Option<oneshot::Receiver<()>>,
    worker: Option<JoinHandle<Result<(), NodeError>>>,
    events: Events,
}
impl PdNode {
    /// Host read adapters share the same state as network heartbeat adapters.
    pub fn cluster(&self) -> Arc<crate::cluster_host::ClusterHost> {
        self.cluster.clone()
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
