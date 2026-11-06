// Copyright 2026 AsterSQL.
use crate::{ControllerConfig, Host, Manager, Service};
use astersql_pd_election::{LeaderWatch, Leadership, LeadershipStore};
use astersql_pd_keypath::{ClusterPaths, MsParam};
use astersql_pd_mcs_discovery::{ServiceRegister, ServiceRegistryEntry};
use astersql_pd_mcs_server::BaseServer;
use astersql_pd_proto::{
    resource_manager::resource_manager_server::ResourceManagerServer, tsopb::Participant,
};
use astersql_pd_storage_domain::{Compare, Consistency, MetaStore, Read};
use futures_util::{FutureExt, future::FusedFuture};
use prost::Message;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    runtime::Handle,
    sync::{Mutex, RwLock},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use tonic::Status;
/// All process resources are supplied by the embedding host.
pub struct HostConfig {
    pub runtime: Handle,
    pub cancel: CancellationToken,
    pub store: Arc<dyn MetaStore>,
    pub cluster_id: u64,
    pub entry: ServiceRegistryEntry,
    pub controller: ControllerConfig,
}
struct Term {
    leader: Arc<Leadership>,
    cancel: CancellationToken,
    manager: Arc<Mutex<Manager>>,
}
struct State {
    cluster_id: u64,
    store: Arc<dyn MetaStore>,
    cancel: CancellationToken,
    term: RwLock<Option<Term>>,
}
#[async_trait::async_trait]
impl Host for State {
    async fn manager(&self) -> Result<Arc<Mutex<Manager>>, Status> {
        let term = self.term.read().await;
        match term.as_ref() {
            Some(term)
                if !self.cancel.is_cancelled()
                    && !term.cancel.is_cancelled()
                    && !term.leader.is_expired() =>
            {
                Ok(term.manager.clone())
            }
            _ => Err(Status::unavailable("not leader")),
        }
    }
    fn reject_metadata_writes(&self) -> bool {
        true
    }
    async fn consumption_keyspace_exists(&self, id: u32) -> bool {
        // The standalone process observes the same keyspace metadata as PD.
        let key = ClusterPaths::new(self.cluster_id).keyspace_meta_path(id);
        self.store
            .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
            .await
            .is_ok_and(|s| !s.entries.is_empty())
    }
}
/// Owns the listener, service registration and primary worker on a caller executor.
pub struct ResourceManagerHost {
    state: Arc<State>,
    listener: BaseServer,
    registry: Option<ServiceRegister>,
    worker: Option<JoinHandle<()>>,
}
impl ResourceManagerHost {
    pub async fn start<L>(config: HostConfig, listener: L) -> Result<Self, String>
    where
        L: axum::serve::Listener<Addr = SocketAddr> + Send + 'static,
    {
        Self::start_with_log_file(config, listener, String::new()).await
    }
    /// Register common diagnostics with the log path owned by the embedding host.
    pub async fn start_with_log_file<L>(
        config: HostConfig,
        listener: L,
        log_file: String,
    ) -> Result<Self, String>
    where
        L: axum::serve::Listener<Addr = SocketAddr> + Send + 'static,
    {
        Self::start_with_primary_lease(
            config,
            listener,
            log_file,
            astersql_pd_mcs_server::DEFAULT_LEASE,
        )
        .await
    }
    /// Use the caller's primary lease while keeping registry leases independent.
    pub async fn start_with_primary_lease<L>(
        config: HostConfig,
        listener: L,
        log_file: String,
        primary_lease: i64,
    ) -> Result<Self, String>
    where
        L: axum::serve::Listener<Addr = SocketAddr> + Send + 'static,
    {
        let cancel = config.cancel.child_token();
        let state = Arc::new(State {
            cluster_id: config.cluster_id,
            store: config.store.clone(),
            cancel: cancel.clone(),
            term: RwLock::new(None),
        });
        let host = state.clone();
        let diagnostics_runtime = config.runtime.clone();
        let mut listener = BaseServer::start(
            config.runtime.clone(),
            &cancel,
            listener,
            move |cancel| {
                let service = ResourceManagerServer::new(Service::new(host, cancel.clone()));
                Ok(
                    astersql_pd_mcs_server::auxiliary_routes(diagnostics_runtime, cancel, log_file)
                        .add_service(service)
                        .into_axum_router(),
                )
            },
            vec![],
        )
        .await?;
        let registry = match ServiceRegister::start(
            config.store.clone(),
            config.runtime.clone(),
            cancel.clone(),
            config.cluster_id,
            "resource_manager",
            &config.entry.service_addr,
            config.entry.serialize().map_err(|e| e.to_string())?,
            5,
        )
        .await
        {
            Ok(registry) => registry,
            Err(error) => {
                cancel.cancel();
                let _ = listener.shutdown().await;
                return Err(error.to_string());
            }
        };
        let worker_state = state.clone();
        let worker = config.runtime.clone().spawn(async move {
            run(config, worker_state, primary_lease).await;
        });
        Ok(Self {
            state,
            listener,
            registry: Some(registry),
            worker: Some(worker),
        })
    }
    pub fn address(&self) -> SocketAddr {
        self.listener.address
    }
    pub async fn shutdown(mut self) -> Result<(), String> {
        self.state.cancel.cancel();
        let result = self.listener.shutdown().await;
        if let Some(worker) = self.worker.take() {
            worker.await.map_err(|e| e.to_string())?;
        }
        if let Some(registry) = self.registry.take() {
            registry.shutdown().await.map_err(|e| e.to_string())?;
        }
        result
    }
}
impl Drop for ResourceManagerHost {
    fn drop(&mut self) {
        self.state.cancel.cancel();
    }
}
async fn pause(cancel: &CancellationToken) {
    tokio::select! {_=cancel.cancelled()=>{},_=tokio::time::sleep(Duration::from_millis(100))=>{}}
}
async fn run(config: HostConfig, state: Arc<State>, primary_lease: i64) {
    let paths = ClusterPaths::new(config.cluster_id);
    let param = MsParam {
        service_name: "resource_manager".into(),
        group_id: 0,
    };
    let key = paths.election_path(Some(&param)).into_bytes();
    let value = Participant {
        name: config.entry.service_addr.clone(),
        id: astersql_pd_memberutil::generate_unique_id(&config.entry.service_addr),
        listen_urls: vec![config.entry.service_addr.clone()],
        ..Default::default()
    }
    .encode_to_vec();
    while !state.cancel.is_cancelled() {
        let attempt = async {
            let snapshot = config
                .store
                .read(Read::range(&key, b"", Consistency::Linearizable))
                .await
                .map_err(|e| e.to_string())?;
            if !snapshot.entries.is_empty() {
                let mut watch =
                    LeaderWatch::new(config.store.clone(), key.clone(), snapshot.revision + 1)
                        .map_err(|e| e.to_string())?;
                while let Ok(event) = watch.next(&state.cancel).await {
                    if matches!(event, astersql_pd_storage_domain::Event::Delete { .. }) {
                        break;
                    }
                }
                return Ok::<(), String>(());
            }
            let expected =
                astersql_pd_mcs_utils::expected_primary(config.store.as_ref(), &paths, &param)
                    .await?;
            let guards = match &expected {
                Some(row) if !row.value.is_empty() => {
                    if row.value != config.entry.service_addr.as_bytes() {
                        return Ok(());
                    }
                    vec![Compare::Value(
                        paths.expected_primary_path(&param).into_bytes(),
                        row.value.clone(),
                    )]
                }
                _ => vec![],
            };
            let term_cancel = state.cancel.child_token();
            let leader = Arc::new(
                Leadership::campaign_with_key(
                    config.store.clone(),
                    term_cancel.clone(),
                    config.cluster_id,
                    key.clone(),
                    value.clone(),
                    primary_lease,
                    guards,
                )
                .await
                .map_err(|e| e.to_string())?,
            );
            let keep = leader.keep(&config.runtime, term_cancel.clone());
            let keep_join = keep.join().fuse();
            tokio::pin!(keep_join);
            let fenced: Arc<dyn MetaStore> = Arc::new(LeadershipStore::new(
                config.store.clone(),
                leader.clone(),
                term_cancel.clone(),
            ));
            let load = Manager::open_watched(fenced, config.controller.clone());
            let manager = tokio::select! {
                _=state.cancel.cancelled()=>None,
                _=&mut keep_join=>None,
                result=load=>match result {
                    Ok((m,watch))=>Some((Arc::new(Mutex::new(m)),watch)),
                    Err(error)=>{tracing::warn!(component="resource-manager",%error,"metadata bootstrap failed");None}
                },
            };
            if let Some((manager, mut watch)) = manager {
                if let Some(row) = &expected {
                    let _ = astersql_pd_mcs_utils::clear_expected_primary(
                        config.store.as_ref(),
                        paths.expected_primary_path(&param).into_bytes(),
                        row,
                    )
                    .await;
                }
                *state.term.write().await = Some(Term {
                    leader: leader.clone(),
                    cancel: term_cancel.clone(),
                    manager: manager.clone(),
                });
                let mut priority = tokio::time::interval(Duration::from_secs(1));
                let mut persistence = tokio::time::interval(Duration::from_secs(60));
                priority.tick().await;
                persistence.tick().await;
                loop {
                    tokio::select! {
                        biased;
                        _=state.cancel.cancelled()=>break,
                        _=&mut keep_join=>break,
                        events=watch.next(&term_cancel)=>{
                            let events=match events {
                                Ok(events)=>events,
                                Err(astersql_pd_storage_domain::StoreError::Unavailable)=>{pause(&term_cancel).await;continue;}
                                Err(error)=>{if !term_cancel.is_cancelled(){tracing::warn!(component="resource-manager",%error,"metadata watch requires reload");}break;}
                            };
                            let mut manager=tokio::select!{_=term_cancel.cancelled()=>break,manager=manager.lock()=>manager};
                            if let Err(error)=events.events.into_iter().try_for_each(|event|manager.metadata_event(event)) {
                                tracing::warn!(component="resource-manager",%error,"metadata event rejected");break;
                            }
                        }
                        _=priority.tick()=>{
                            tokio::select!{_=state.cancel.cancelled()=>break,mut manager=manager.lock()=>{manager.conciliate();}}
                        }
                        _=persistence.tick()=>{
                            tokio::select!{_=state.cancel.cancelled()=>break,mut manager=manager.lock()=>{let _=manager.persist_states().await;manager.cleanup_demand_trackers(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos() as i64);}}
                        }
                    }
                }
                state.term.write().await.take();
            }
            term_cancel.cancel();
            // A completed JoinHandle future must never be polled twice.
            if !keep_join.is_terminated() {
                let _ = keep_join.await;
            }
            let _ = tokio::time::timeout(Duration::from_secs(5), leader.resign()).await;
            Ok(())
        };
        if let Err(error) = attempt.await {
            if !state.cancel.is_cancelled() {
                tracing::warn!(component="resource-manager",%error,"primary attempt failed");
            }
        }
        pause(&state.cancel).await;
    }
}
