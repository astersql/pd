// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use astersql_pd_storage::{Consistency, MetaStore, Read};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_032_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let reserved = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = reserved.local_addr().unwrap();
    drop(reserved);
    for restart in [false, true] {
        let listener = tokio::net::TcpListener::bind(address).await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let cancel = CancellationToken::new();
        let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 3200,
                    member_id: 1,
                    name: "global".into(),
                    client_url: url.clone(),
                    peer_url: "http://peer".into(),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
        node.ready().await.unwrap();
        let mut client = PdClient::connect(url.clone()).await.unwrap();
        if !restart {
            let result = client
                .store_global_config(pdpb::StoreGlobalConfigRequest {
                    changes: vec![pdpb::GlobalConfigItem {
                        name: "persist".into(),
                        payload: vec![0, 255, 1],
                        ..Default::default()
                    }],
                    ..Default::default()
                })
                .await;
            assert!(
                result.is_ok(),
                "global config must work before bootstrap: {result:?}"
            );
            store
                .transact(
                    vec![],
                    vec![
                        Mutation::put(b"resource_group/controller", b"controller-value"),
                        Mutation::put(b"resource_group/controller-other", b"sibling-value"),
                        Mutation::put(b"/global/config-sibling/secret", b"outside"),
                    ],
                )
                .await
                .unwrap();
            let controller = client
                .load_global_config(pdpb::LoadGlobalConfigRequest {
                    config_path: "resource_group/controller".into(),
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner();
            assert_eq!(controller.items.len(), 2);
            assert_eq!(controller.items[0].name, "resource_group/controller");
            assert_eq!(controller.items[1].name, "resource_group/controller-other");
            assert_eq!(controller.items[0].payload, b"controller-value");
            super::grpc_global_config_scenarios_test::observe(&mut client).await;
        }
        let response = client
            .load_global_config(pdpb::LoadGlobalConfigRequest {
                names: vec!["persist".into()],
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(response.items[0].payload, vec![0, 255, 1]);
        assert_eq!(
            store
                .read(Read::range(
                    b"/global/config/persist",
                    b"",
                    Consistency::Linearizable
                ))
                .await
                .unwrap()
                .entries[0]
                .value,
            vec![0, 255, 1]
        );
        assert!(
            std::process::Command::new("curl")
                .args(["-fsS", &format!("{url}/pd/api/v1/ping")])
                .output()
                .unwrap()
                .status
                .success()
        );
        cancel.cancel();
        assert!(
            client
                .load_global_config(pdpb::LoadGlobalConfigRequest::default())
                .await
                .is_err()
        );
        node.shutdown().await.unwrap();
    }
}

use astersql_pd_storage::{
    Commit, Compare, Lease, LeaseId, MetaWatch, Mutation, RawTxnOp, RawTxnResponse, Revision,
    Snapshot,
};
struct ReadGateStore {
    inner: Arc<dyn MetaStore>,
    entered: tokio::sync::Notify,
    dropped: std::sync::atomic::AtomicBool,
}
#[async_trait::async_trait]
impl MetaStore for ReadGateStore {
    async fn read(&self, request: Read) -> astersql_pd_storage::Result<Snapshot> {
        if request.start.as_deref() == Some(b"/global/config/fault") {
            let mut request = request;
            request.revision = Some(i64::MAX);
            return self.inner.read(request).await;
        }
        if request.start.as_deref() == Some(b"/global/config/blocked") {
            struct Dropped<'a>(&'a std::sync::atomic::AtomicBool);
            impl Drop for Dropped<'_> {
                fn drop(&mut self) {
                    self.0.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            }
            let _guard = Dropped(&self.dropped);
            self.entered.notify_one();
            std::future::pending::<()>().await;
        }
        self.inner.read(request).await
    }
    async fn transact(
        &self,
        compares: Vec<Compare>,
        writes: Vec<Mutation>,
    ) -> astersql_pd_storage::Result<Commit> {
        self.inner.transact(compares, writes).await
    }
    async fn transact_branches(
        &self,
        compares: Vec<Compare>,
        then_writes: Vec<Mutation>,
        else_writes: Vec<Mutation>,
    ) -> astersql_pd_storage::Result<Commit> {
        self.inner
            .transact_branches(compares, then_writes, else_writes)
            .await
    }
    async fn transact_raw(
        &self,
        compares: Vec<Compare>,
        then_ops: Vec<RawTxnOp>,
        else_ops: Vec<RawTxnOp>,
    ) -> astersql_pd_storage::Result<RawTxnResponse> {
        self.inner.transact_raw(compares, then_ops, else_ops).await
    }
    async fn watch(
        &self,
        prefix: Vec<u8>,
        start_revision: Revision,
    ) -> astersql_pd_storage::Result<Box<dyn MetaWatch>> {
        self.inner.watch(prefix, start_revision).await
    }
    async fn grant(&self, ttl_seconds: i64) -> astersql_pd_storage::Result<Lease> {
        self.inner.grant(ttl_seconds).await
    }
    async fn keep_alive(&self, lease: LeaseId) -> astersql_pd_storage::Result<Lease> {
        self.inner.keep_alive(lease).await
    }
    async fn revoke(&self, lease: LeaseId) -> astersql_pd_storage::Result<()> {
        self.inner.revoke(lease).await
    }
    async fn ttl(&self, lease: LeaseId) -> astersql_pd_storage::Result<i64> {
        self.inner.ttl(lease).await
    }
    async fn compact(&self, revision: Revision) -> astersql_pd_storage::Result<()> {
        self.inner.compact(revision).await
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_032_backend_failure_and_cancel() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(ReadGateStore {
        inner: Arc::new(fixture.store().await),
        entered: tokio::sync::Notify::new(),
        dropped: std::sync::atomic::AtomicBool::new(false),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let cancel = CancellationToken::new();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 3201,
                member_id: 1,
                name: "fault".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let mut client = PdClient::connect(url.clone()).await.unwrap();
    let error = client
        .store_global_config(pdpb::StoreGlobalConfigRequest {
            changes: vec![
                pdpb::GlobalConfigItem {
                    name: "small".into(),
                    payload: b"must not commit".to_vec(),
                    ..Default::default()
                },
                pdpb::GlobalConfigItem {
                    name: "large".into(),
                    payload: vec![7; 1600 * 1024],
                    ..Default::default()
                },
            ],
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::Unknown);
    assert_eq!(error.message(), "etcdserver: request is too large");
    assert!(
        store
            .inner
            .read(Read::prefix(b"/global/config/", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    let response = client
        .load_global_config(pdpb::LoadGlobalConfigRequest {
            names: vec!["missing".into(), "fault".into(), "missing-again".into()],
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.items.len(), 3);
    assert_eq!(
        response.items[0].error.as_ref().unwrap().r#type,
        pdpb::ErrorType::GlobalConfigNotFound as i32
    );
    assert_eq!(
        response.items[1].error.as_ref().unwrap().r#type,
        pdpb::ErrorType::Unknown as i32
    );
    assert!(
        response.items[1]
            .error
            .as_ref()
            .unwrap()
            .message
            .contains("newer")
    );
    assert_eq!(
        response.items[2].error.as_ref().unwrap().r#type,
        pdpb::ErrorType::GlobalConfigNotFound as i32
    );
    let mut request = tonic::Request::new(pdpb::LoadGlobalConfigRequest {
        names: vec!["blocked".into()],
        ..Default::default()
    });
    request.set_timeout(std::time::Duration::from_millis(200));
    let pending = tokio::spawn(async move { client.load_global_config(request).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), store.entered.notified())
        .await
        .unwrap();
    let error = pending.await.unwrap().unwrap_err();
    assert!(matches!(
        error.code(),
        tonic::Code::Cancelled | tonic::Code::DeadlineExceeded
    ));
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !store.dropped.load(std::sync::atomic::Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    // A second blocked request is cancelled by host shutdown, which must also join the IO future.
    store
        .dropped
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let mut client = PdClient::connect(url).await.unwrap();
    let pending = tokio::spawn(async move {
        client
            .load_global_config(pdpb::LoadGlobalConfigRequest {
                names: vec!["blocked".into()],
                ..Default::default()
            })
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), store.entered.notified())
        .await
        .unwrap();
    cancel.cancel();
    assert!(pending.await.unwrap().is_err());
    node.shutdown().await.unwrap();
    assert!(store.dropped.load(std::sync::atomic::Ordering::SeqCst));
}

struct NoBackend(crate::Identity);
#[async_trait::async_trait]
impl crate::PdServices for NoBackend {
    fn identity(&self) -> &crate::Identity {
        &self.0
    }
    async fn healthy(&self) -> Result<(), String> {
        Ok(())
    }
    async fn alloc_id(&self) -> Result<u64, String> {
        Err("not configured".into())
    }
    async fn timestamp(&self, _: u32) -> Result<astersql_pd_tso::Timestamp, String> {
        Err("not configured".into())
    }
}
#[tokio::test]
async fn task_032_missing_backend() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let services = Arc::new(NoBackend(crate::Identity {
        cluster_id: 32,
        member_id: 1,
        name: "no-backend".into(),
        client_url: url.clone(),
        peer_url: "http://peer".into(),
    }));
    let cancel = CancellationToken::new();
    let stop = cancel.clone();
    let task = tokio::spawn(async move {
        axum::serve(listener, crate::router(services, stop.clone()))
            .with_graceful_shutdown(stop.cancelled_owned())
            .await
            .unwrap();
    });
    let mut client = PdClient::connect(url).await.unwrap();
    let error = client
        .store_global_config(pdpb::StoreGlobalConfigRequest {
            config_path: "/outside".into(),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::Unavailable);
    assert_eq!(error.message(), "server is started, but etcd not started");
    let error = client
        .load_global_config(pdpb::LoadGlobalConfigRequest {
            config_path: "/outside".into(),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::Unavailable);
    assert_eq!(error.message(), "server is started, but etcd not started");
    drop(client);
    cancel.cancel();
    task.await.unwrap();
}
