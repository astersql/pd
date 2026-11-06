// Copyright 2026 AsterSQL.
use crate::Service;
use astersql_pd_core::BasicCluster;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_mcs_discovery::{ServiceRegister, ServiceRegistryEntry};
use astersql_pd_mcs_server::{BaseServer, DelegateConnections, PrimaryObserver};
use astersql_pd_mcs_utils::StoreWatcher;
use astersql_pd_proto::{pdpb, routerpb::router_server::RouterServer};
use astersql_pd_storage_domain::MetaStore;
use prost::Message;
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{runtime::Handle, task::JoinHandle};
use tokio_util::sync::CancellationToken;
/// The embedding host owns executor, backend, credentials, and canonical cache.
pub struct HostConfig {
    pub runtime: Handle,
    pub cancel: CancellationToken,
    pub store: Arc<dyn MetaStore>,
    pub cluster_id: u64,
    pub entry: ServiceRegistryEntry,
    pub cluster: Arc<Mutex<BasicCluster>>,
    pub delegates: Arc<DelegateConnections>,
}
/// Owns Router registration, its listener and cache observation workers.
pub struct RouterHost {
    listener: BaseServer,
    registry: Option<ServiceRegister>,
    cancel: CancellationToken,
    workers: Vec<JoinHandle<()>>,
}
impl RouterHost {
    pub async fn start<L>(config: HostConfig, listener: L) -> Result<Self, String>
    where
        L: axum::serve::Listener<Addr = SocketAddr> + Send + 'static,
    {
        let cancel = config.cancel.child_token();
        let stores = config.cluster.lock().unwrap().stores.clone();
        let mut watch = StoreWatcher::load(
            config.store.clone(),
            ClusterPaths::new(config.cluster_id),
            stores,
            &cancel,
        )
        .await
        .map_err(|e| e.to_string())?;
        let service = Service::new(config.cluster_id, config.cluster.clone(), cancel.clone());
        let mut listener = BaseServer::start(
            config.runtime.clone(),
            &cancel,
            listener,
            move |_| Ok(tonic::service::Routes::new(RouterServer::new(service)).into_axum_router()),
            vec![],
        )
        .await?;
        let registry = match ServiceRegister::start(
            config.store.clone(),
            config.runtime.clone(),
            cancel.clone(),
            config.cluster_id,
            "router",
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
        let task_cancel = cancel.clone();
        let metadata = config.runtime.spawn(async move {
            while !task_cancel.is_cancelled() {
                let result = watch.advance(&task_cancel).await;
                if result.is_err() {
                    pause(&task_cancel).await;
                }
            }
        });
        let task_cancel = cancel.clone();
        let runtime = config.runtime.clone();
        let regions = runtime.spawn(async move {
            sync_regions(config, task_cancel).await;
        });
        Ok(Self {
            listener,
            registry: Some(registry),
            cancel,
            workers: vec![metadata, regions],
        })
    }
    pub async fn shutdown(mut self) -> Result<(), String> {
        self.cancel.cancel();
        let result = self.listener.shutdown().await;
        for worker in self.workers.drain(..) {
            worker.await.map_err(|e| e.to_string())?;
        }
        if let Some(registry) = self.registry.take() {
            registry.shutdown().await.map_err(|e| e.to_string())?;
        }
        result
    }
}
impl Drop for RouterHost {
    fn drop(&mut self) {
        self.cancel.cancel();
        for worker in &self.workers {
            worker.abort();
        }
    }
}
async fn pause(cancel: &CancellationToken) {
    tokio::select! {_=cancel.cancelled()=>{},_=tokio::time::sleep(Duration::from_millis(100))=>{}}
}
async fn sync_regions(config: HostConfig, cancel: CancellationToken) {
    let primary = PrimaryObserver::start(
        &config.runtime,
        &cancel,
        config.store,
        ClusterPaths::new(config.cluster_id)
            .election_path(None)
            .into_bytes(),
        |bytes| {
            pdpb::Member::decode(bytes)
                .ok()?
                .client_urls
                .into_iter()
                .next()
        },
    );
    let mut next_index = 0;
    while !cancel.is_cancelled() {
        let Some(target) = primary.current() else {
            pause(&cancel).await;
            continue;
        };
        let attempt = async {
            let channel = config.delegates.get(&target).await?;
            let mut client = pdpb::pd_client::PdClient::new(channel)
                .max_decoding_message_size(100 * 1024 * 1024);
            let (send, receive) = tokio::sync::mpsc::channel(1);
            send.send(pdpb::SyncRegionRequest {
                header: Some(pdpb::RequestHeader {
                    cluster_id: config.cluster_id,
                    ..Default::default()
                }),
                member: Some(pdpb::Member {
                    name: config.entry.name.clone(),
                    client_urls: vec![config.entry.service_addr.clone()],
                    ..Default::default()
                }),
                start_index: next_index,
            })
            .await
            .map_err(|_| tonic::Status::cancelled("sync closed"))?;
            let mut stream = client
                .sync_regions(tokio_stream::wrappers::ReceiverStream::new(receive))
                .await?
                .into_inner();
            let mut check = tokio::time::interval(Duration::from_millis(100));
            loop {
                let response = tokio::select! {biased;
                    _=cancel.cancelled()=>break,
                    _=check.tick()=>{if primary.current().as_deref()!=Some(&target) {break;}continue;},
                    response=stream.message()=>response?,
                };
                let Some(response) = response else {
                    break;
                };
                if response.header.as_ref().is_some_and(|h| h.error.is_some()) {
                    break;
                }
                next_index = response.start_index;
                let decoded =
                    astersql_pd_server::sync_codec::response_from_proto(response).into_apply();
                let mut cluster = config.cluster.lock().unwrap();
                for region in decoded.into_regions() {
                    cluster.regions.put_region(Arc::new(region));
                    next_index = next_index.saturating_add(1);
                }
            }
            drop(send);
            Ok::<(), tonic::Status>(())
        };
        tokio::select! {biased;_=cancel.cancelled()=>break,_=attempt=>{}}
        config.delegates.invalidate(&target);
        pause(&cancel).await;
    }
    primary.shutdown().await;
}
