// Copyright 2026 AsterSQL.
use astersql_pd_keyspace::KeyspaceGroupManager;
use astersql_pd_mcs_discovery::{ServiceRegister, ServiceRegistryEntry};
use astersql_pd_proto::tsopb::tso_server::TsoServer;
use astersql_pd_tso::{GroupAllocatorWatch, GroupWatchConfig, TsoError};
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
/// One service's resources; the caller owns its Tokio runtime, store and listener.
pub struct TsoServiceHost {
    pub(crate) watch: Arc<Mutex<GroupAllocatorWatch>>,
    cancel: CancellationToken,
    workers: Vec<tokio::task::JoinHandle<()>>,
    registry: Option<ServiceRegister>,
    pub address: std::net::SocketAddr,
}
impl TsoServiceHost {
    /// Load the authoritative assignment before publishing a lease-backed registry entry.
    /// The injected manager is the same PD catalog used by its HTTP boundary.
    pub async fn start(
        config: GroupWatchConfig,
        groups: Arc<KeyspaceGroupManager>,
        listener: tokio::net::TcpListener,
    ) -> Result<Self, TsoError> {
        Self::start_inner(config, groups, listener, None, None).await
    }
    /// Independent TSO hosts complete transitions through the PD leader's existing HTTP API.
    pub async fn start_with_pd_http(
        config: GroupWatchConfig,
        groups: Arc<KeyspaceGroupManager>,
        listener: tokio::net::TcpListener,
        pd_url: String,
    ) -> Result<Self, TsoError> {
        Self::start_inner(config, groups, listener, Some(pd_url), None).await
    }
    /// Start the same embedded host with process identity supplied by its caller.
    pub async fn start_with_entry(
        config: GroupWatchConfig,
        groups: Arc<KeyspaceGroupManager>,
        listener: tokio::net::TcpListener,
        pd_url: Option<String>,
        entry: ServiceRegistryEntry,
    ) -> Result<Self, TsoError> {
        Self::start_inner(config, groups, listener, pd_url, Some(entry)).await
    }
    async fn start_inner(
        config: GroupWatchConfig,
        groups: Arc<KeyspaceGroupManager>,
        listener: tokio::net::TcpListener,
        pd_url: Option<String>,
        entry: Option<ServiceRegistryEntry>,
    ) -> Result<Self, TsoError> {
        if !groups.uses_metadata_store(&config.store, config.cluster_id) {
            return Err(TsoError::StoreMismatch);
        }
        let address = listener
            .local_addr()
            .map_err(|_| TsoError::InvalidArgument)?;
        let cancel = config.cancel.child_token();
        let mut owned = config.clone();
        owned.cancel = cancel.clone();
        let watch = Arc::new(Mutex::new(GroupAllocatorWatch::load(owned.clone()).await?));
        let entry = entry.unwrap_or(ServiceRegistryEntry {
            name: config.address.clone(),
            service_addr: config.address.clone(),
            version: env!("CARGO_PKG_VERSION").into(),
            start_timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| TsoError::InvalidArgument)?
                .as_secs() as i64,
            ..Default::default()
        });
        let registry = ServiceRegister::start(
            config.store.clone(),
            config.runtime.clone(),
            cancel.clone(),
            config.cluster_id,
            "tso",
            &config.address,
            entry.serialize().map_err(|_| TsoError::InvalidArgument)?,
            config.lease_seconds,
        )
        .await;
        let registry = match registry {
            Ok(term) => term,
            Err(error) => {
                cancel.cancel();
                watch.lock().await.stop_allocators().await?;
                return Err(match error {
                    astersql_pd_mcs_discovery::DiscoveryError::Occupied => {
                        TsoError::Member(astersql_pd_election::ElectionError::Contended)
                    }
                    astersql_pd_mcs_discovery::DiscoveryError::Store(e) => TsoError::Store(e),
                    _ => TsoError::InvalidArgument,
                });
            }
        };
        let service = crate::grpc_service::Service {
            watch: watch.clone(),
            cluster: config.cluster_id,
            store: config.store.clone(),
            cancel: cancel.clone(),
        };
        let serve_cancel = cancel.clone();
        let failure_cancel = cancel.clone();
        let http = crate::primary_http::router(
            watch.clone(),
            config.store.clone(),
            config.cluster_id,
            cancel.clone(),
            entry,
        );
        let app = http.fallback_service(TsoServer::new(service));
        let serve = config.runtime.spawn(async move {
            let result = axum::serve(listener, app)
                .with_graceful_shutdown(serve_cancel.cancelled_owned())
                .await;
            if result.is_err() {
                failure_cancel.cancel();
            }
        });
        let watch_copy = watch.clone();
        let loop_cancel = cancel.clone();
        let worker = config.runtime.spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(100));
            loop {
                tokio::select! {
                    biased;
                    _ = loop_cancel.cancelled() => break,
                    _ = tick.tick() => {
                        let mut w = watch_copy.lock().await;
                        match w.poll_next(Duration::from_millis(20)).await {
                            Err(_) => {
                                if w.reload().await.is_err() { continue; }
                            },
                            Ok(_) => {},
                        }
                        let transitions = w.assigned_ids().iter()
                            .filter_map(|id| w.group(*id))
                            .filter(|g| g.is_split_target() || g.is_merge_target())
                            .cloned().collect::<Vec<_>>();
                        for observed in transitions {
                            // The serving default may have repaired implicit members.
                            // Complete against the persisted DTO rather than that local view.
                            let Ok(Some(g)) = groups.get_group(observed.id).await else { continue; };
                            if !g.is_split_target() && !g.is_merge_target() { continue; }
                            if astersql_pd_tso::reserve_group_handoff(&owned, &g).await.is_ok() {
                                let done = if let Some(url)=&pd_url {
                                    crate::transition_http::finish(url,&g,&loop_cancel).await.map_err(|e|e.to_string())
                                } else if g.is_split_target() {
                                    groups.finish_split_handoff(&g).await.map_err(|e| e.to_string())
                                } else {
                                    groups.finish_merge_handoff(&g).await.map_err(|e| e.to_string())
                                };
                                if done.is_ok() { let _ = w.reload().await; }
                            }
                        }
                    }
                }
            }
            let _ = watch_copy.lock().await.stop_allocators().await;

        });
        Ok(Self {
            watch,
            cancel,
            workers: vec![serve, worker],
            registry: Some(registry),
            address,
        })
    }
    pub fn is_ready(&self) -> bool {
        !self.cancel.is_cancelled() && !self.workers.iter().any(|w| w.is_finished())
    }
    /// Cancel streams/workers, release allocators and revoke only this host's registry lease.
    pub async fn shutdown(mut self) -> Result<(), TsoError> {
        self.cancel.cancel();
        let mut failure = None;
        for worker in self.workers.drain(..) {
            if worker.await.is_err() {
                failure = Some(TsoError::Cancelled);
            }
        }
        // All RPCs have ended, so the sole watch owner can deterministically release its leases.
        if let Err(error) = self.watch.lock().await.stop_allocators().await {
            failure = Some(error);
        }
        if let Some(registry) = self.registry.take() {
            if registry.shutdown().await.is_err() {
                failure = Some(TsoError::Store(
                    astersql_pd_storage_domain::StoreError::Unavailable,
                ));
            }
        }
        failure.map_or(Ok(()), Err)
    }
}
impl Drop for TsoServiceHost {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
