// Copyright 2026 AsterSQL.
use super::*;
use crate::{NodeConfig, PdNodeBuilder, node_test::fixture};
use astersql_pd_core::{StoreInfo, StoreMeta};
use astersql_pd_server_config::PersistedConfig;
use std::sync::RwLock;
use std::time::Duration;

async fn endpoint() -> (
    String,
    Arc<RwLock<String>>,
    tokio::task::JoinHandle<()>,
    tokio::sync::mpsc::UnboundedReceiver<String>,
) {
    let body = Arc::new(RwLock::new(
        r#"{"coprocessor":{"region-max-size":"256MiB"},"storage":{"engine":"raft-kv2"}}"#
            .to_string(),
    ));
    let content = body.clone();
    let (observations, received) = tokio::sync::mpsc::unbounded_channel();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            axum::Router::new().route(
                "/config",
                axum::routing::get(move || {
                    let content = content.clone();
                    let observations = observations.clone();
                    async move {
                        let body = content.read().unwrap().clone();
                        let _ = observations.send(body.clone());
                        body
                    }
                }),
            ),
        )
        .await
        .unwrap();
    });
    (address, body, server, received)
}
async fn rust_scenario() -> PersistedConfig {
    let fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let storage = StorageEndpoint::new(store.clone());
    let (address, body, server, _received) = endpoint().await;
    let cluster = BasicCluster::new();
    cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 1,
        status_address: address,
        node_state: NodeState::Serving,
        ..Default::default()
    })));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let registry = Arc::new(prometheus::Registry::new());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .with_cluster(cluster)
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 514,
                member_id: 14,
                name: "store-config".into(),
                client_url: format!("http://{address}"),
                peer_url: "http://127.0.0.1:2380".into(),
            },
            listener,
            registry.clone(),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let host = node.cluster();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if host.store_config().await.unwrap().is_raft_kv2() {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "worker must publish fetched config: {:?}",
            host.store_config_error()
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let mut saved: PersistedConfig = storage
        .load_config(&ClusterPaths::new(514))
        .await
        .unwrap()
        .unwrap();
    saved.store.adjust();
    assert_eq!(saved.store.get_region_max_size(), 256);
    assert_eq!(saved.config.schedule.store_limit_version, "v2");
    assert_eq!(host.store_config().await.unwrap(), saved.store);
    // Exercise the same admitted operation without waiting another wall-clock minute.
    *body.write().unwrap() = "not JSON".into();
    {
        let mut resources = host.resources.lock().await;
        let resources = resources.as_mut().unwrap();
        assert!(
            resources
                .sync_store_configuration(&mut Vec::new(), &CancellationToken::new())
                .await
                .is_err()
        );
        assert_eq!(resources.state.lock().unwrap().config.store, saved.store);
    }
    assert_eq!(
        storage
            .load_config::<PersistedConfig>(&ClusterPaths::new(514))
            .await
            .unwrap()
            .unwrap(),
        saved
    );
    let failure = Arc::new(ConfigFailure {
        inner: store.clone(),
        fail: AtomicBool::new(true),
    });
    *body.write().unwrap() =
        r#"{"coprocessor":{"region-max-size":"512MiB"},"storage":{"engine":"raft-kv2"}}"#.into();
    {
        let mut resources = host.resources.lock().await;
        let resources = resources.as_mut().unwrap();
        resources.storage = Arc::new(StorageEndpoint::new(failure.clone()));
        assert!(
            resources
                .sync_store_configuration(&mut Vec::new(), &CancellationToken::new())
                .await
                .is_err()
        );
        assert_eq!(resources.state.lock().unwrap().config.store, saved.store);
        assert_eq!(
            storage
                .load_config::<PersistedConfig>(&ClusterPaths::new(514))
                .await
                .unwrap()
                .unwrap(),
            saved
        );
        failure.fail.store(false, Ordering::SeqCst);
        resources
            .sync_store_configuration(&mut Vec::new(), &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(
            resources
                .state
                .lock()
                .unwrap()
                .config
                .store
                .get_region_max_size(),
            512
        );
        let mut persisted = storage
            .load_config::<PersistedConfig>(&ClusterPaths::new(514))
            .await
            .unwrap()
            .unwrap();
        persisted.store.adjust();
        assert_eq!(persisted.store.get_region_max_size(), 512);
        let stopped = CancellationToken::new();
        stopped.cancel();
        *body.write().unwrap() = "{}".into();
        resources
            .sync_store_configuration(&mut Vec::new(), &stopped)
            .await
            .unwrap();
        assert_eq!(
            resources.state.lock().unwrap().config.store,
            persisted.store
        );
    }
    node.shutdown().await.unwrap();
    assert!(host.store_config().await.is_none());
    assert!(registry.gather().is_empty());
    assert!(tokio::net::TcpStream::connect(address).await.is_err());
    server.abort();
    let _ = server.await;
    drop((storage, failure));
    assert_eq!(Arc::strong_count(&store), 1);
    saved
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_014_runtime_success_error_and_shutdown() {
    rust_scenario().await;
}

use astersql_pd_storage::*;
use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, Ordering};
struct ConfigFailure {
    inner: Arc<dyn MetaStore>,
    fail: AtomicBool,
}
#[async_trait]
impl MetaStore for ConfigFailure {
    async fn read(&self, r: Read) -> Result<Snapshot> {
        self.inner.read(r).await
    }
    async fn transact(&self, c: Vec<Compare>, w: Vec<Mutation>) -> Result<Commit> {
        if w.iter()
            .any(|m| matches!(m, Mutation::Put { key, .. } if key.ends_with(b"/config")))
            && self.fail.load(Ordering::SeqCst)
        {
            return Err(StoreError::Unavailable);
        }
        self.inner.transact(c, w).await
    }
    async fn transact_branches(
        &self,
        c: Vec<Compare>,
        t: Vec<Mutation>,
        e: Vec<Mutation>,
    ) -> Result<Commit> {
        self.inner.transact_branches(c, t, e).await
    }
    async fn transact_raw(
        &self,
        c: Vec<Compare>,
        t: Vec<RawTxnOp>,
        e: Vec<RawTxnOp>,
    ) -> Result<RawTxnResponse> {
        self.inner.transact_raw(c, t, e).await
    }
    async fn watch(&self, p: Vec<u8>, r: Revision) -> Result<Box<dyn MetaWatch>> {
        self.inner.watch(p, r).await
    }
    async fn grant(&self, t: i64) -> Result<Lease> {
        self.inner.grant(t).await
    }
    async fn keep_alive(&self, l: LeaseId) -> Result<Lease> {
        self.inner.keep_alive(l).await
    }
    async fn revoke(&self, l: LeaseId) -> Result<()> {
        self.inner.revoke(l).await
    }
    async fn ttl(&self, l: LeaseId) -> Result<i64> {
        self.inner.ttl(l).await
    }
    async fn compact(&self, r: Revision) -> Result<()> {
        self.inner.compact(r).await
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go PD binary and read-only supervisor"]
async fn readonly_store_config_differential() {
    use astersql_pd_compat_tests::harness::Server;
    use astersql_pd_proto::{
        metapb,
        pdpb::{self, pd_client::PdClient},
    };
    use astersql_pd_storage_kv::EtcdMetaStore;
    let rust = rust_scenario().await;
    let (address, body, http, mut received) = endpoint().await;
    let go = std::env::var("PD_COMPAT_GO_BIN").unwrap();
    let mut server = Server::start(std::path::Path::new(&go)).unwrap();
    server.ready().unwrap();
    let mut client = PdClient::connect(server.url.clone()).await.unwrap();
    let id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let boot = client
        .bootstrap(pdpb::BootstrapRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: id,
                ..Default::default()
            }),
            store: Some(metapb::Store {
                id: 1,
                address: "127.0.0.1:29999".into(),
                status_address: address.clone(),
                version: "8.5.0".into(),
                ..Default::default()
            }),
            region: Some(metapb::Region {
                id: 1,
                peers: vec![metapb::Peer {
                    id: 1,
                    store_id: 1,
                    ..Default::default()
                }],
                region_epoch: Some(metapb::RegionEpoch {
                    version: 1,
                    conf_ver: 1,
                }),
                ..Default::default()
            }),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(boot.header.unwrap().error.is_none());
    let backend = Arc::new(
        EtcdMetaStore::connect(std::slice::from_ref(&server.url), Duration::from_secs(3))
            .await
            .unwrap(),
    );
    let endpoint = StorageEndpoint::new(backend);
    let paths = ClusterPaths::new(id);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(70);
    let saved = loop {
        if let Some(saved) = endpoint
            .load_config::<PersistedConfig>(&paths)
            .await
            .unwrap()
        {
            if saved.store.is_raft_kv2() {
                break saved;
            }
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "Go worker must persist TiKV config"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_eq!(saved.store, rust.store);
    assert_eq!(
        saved.config.schedule.store_limit_version,
        rust.config.schedule.store_limit_version
    );
    *body.write().unwrap() = "malformed JSON".into();
    // Existing Go binary has no test-only timer acceleration. Observe one real retry.
    tokio::time::timeout(Duration::from_secs(70), async {
        loop {
            if received.recv().await.as_deref() == Some("malformed JSON") {
                break;
            }
        }
    })
    .await
    .expect("observe real Go retry returning malformed HTTP body");
    tokio::time::sleep(Duration::from_millis(100)).await;
    let failed: PersistedConfig = endpoint.load_config(&paths).await.unwrap().unwrap();
    assert_eq!(failed.store, saved.store);
    assert_eq!(
        failed.config.schedule.store_limit_version,
        saved.config.schedule.store_limit_version
    );
    server.stop().unwrap();
    assert!(
        tokio::net::TcpStream::connect(server.url.trim_start_matches("http://"))
            .await
            .is_err()
    );
    http.abort();
    let _ = http.await;
    println!(
        "task014 real HTTP config, raft-v2 persistence and failed retry retain old config match Go/Rust"
    );
}
