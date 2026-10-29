// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use astersql_pd_storage::{MetaStore, Mutation};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_033_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 3300,
                member_id: 1,
                name: "watch".into(),
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
    let revision = store
        .transact(vec![], vec![Mutation::put(b"/global/config/a", &[0, 255])])
        .await
        .unwrap()
        .revision;
    let result = client
        .watch_global_config(pdpb::WatchGlobalConfigRequest {
            revision,
            ..Default::default()
        })
        .await;
    assert!(
        result.is_ok(),
        "watch must replay inclusive revision: {result:?}"
    );
    let mut stream = result.unwrap().into_inner();
    let response = tokio::time::timeout(std::time::Duration::from_secs(5), stream.message())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(response.revision, revision);
    assert_eq!(response.changes[0].payload, vec![0, 255]);
    drop(stream);
    super::grpc_global_watch_scenarios_test::observe(&mut client, store.as_ref()).await;
    assert!(
        std::process::Command::new("curl")
            .args(["-fsS", &format!("{url}/pd/api/v1/ping")])
            .output()
            .unwrap()
            .status
            .success()
    );
    node.shutdown().await.unwrap();
}

use astersql_pd_storage::{
    Commit, Compare, Consistency, Event, Lease, LeaseId, MetaWatch, RawTxnOp, RawTxnResponse, Read,
    Revision, Snapshot,
};
use std::sync::atomic::{AtomicUsize, Ordering};
struct ObservedStore {
    inner: astersql_pd_storage_kv::EtcdMetaStore,
    active: Arc<AtomicUsize>,
}
struct ObservedWatch {
    inner: Box<dyn MetaWatch>,
    active: Arc<AtomicUsize>,
}
impl Drop for ObservedWatch {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::SeqCst);
    }
}
#[async_trait::async_trait]
impl MetaWatch for ObservedWatch {
    fn previous_value(&self, event: &Event) -> Option<Vec<u8>> {
        self.inner.previous_value(event)
    }
    async fn next(
        &mut self,
        cancel: &CancellationToken,
    ) -> astersql_pd_storage::Result<astersql_pd_storage::WatchBatch> {
        self.inner.next(cancel).await
    }
}
#[async_trait::async_trait]
impl MetaStore for ObservedStore {
    async fn consensus_leader_id(&self) -> astersql_pd_storage::Result<Option<u64>> {
        self.inner.consensus_leader_id().await
    }
    async fn read(&self, r: Read) -> astersql_pd_storage::Result<Snapshot> {
        self.inner.read(r).await
    }
    async fn transact(
        &self,
        c: Vec<Compare>,
        w: Vec<Mutation>,
    ) -> astersql_pd_storage::Result<Commit> {
        self.inner.transact(c, w).await
    }
    async fn transact_branches(
        &self,
        c: Vec<Compare>,
        t: Vec<Mutation>,
        e: Vec<Mutation>,
    ) -> astersql_pd_storage::Result<Commit> {
        self.inner.transact_branches(c, t, e).await
    }
    async fn transact_raw(
        &self,
        c: Vec<Compare>,
        t: Vec<RawTxnOp>,
        e: Vec<RawTxnOp>,
    ) -> astersql_pd_storage::Result<RawTxnResponse> {
        self.inner.transact_raw(c, t, e).await
    }
    async fn watch(
        &self,
        p: Vec<u8>,
        r: Revision,
    ) -> astersql_pd_storage::Result<Box<dyn MetaWatch>> {
        let tracked = p.starts_with(b"/global/config/");
        let inner = self.inner.watch(p, r).await?;
        if !tracked {
            return Ok(inner);
        }
        self.active.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(ObservedWatch {
            inner,
            active: self.active.clone(),
        }))
    }
    async fn watch_with_previous(
        &self,
        p: Vec<u8>,
        r: Revision,
    ) -> astersql_pd_storage::Result<Box<dyn MetaWatch>> {
        let tracked = p.starts_with(b"/global/config/");
        let inner = self.inner.watch_with_previous(p, r).await?;
        if !tracked {
            return Ok(inner);
        }
        self.active.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(ObservedWatch {
            inner,
            active: self.active.clone(),
        }))
    }
    async fn grant(&self, t: i64) -> astersql_pd_storage::Result<Lease> {
        self.inner.grant(t).await
    }
    async fn keep_alive(&self, l: LeaseId) -> astersql_pd_storage::Result<Lease> {
        self.inner.keep_alive(l).await
    }
    async fn revoke(&self, l: LeaseId) -> astersql_pd_storage::Result<()> {
        self.inner.revoke(l).await
    }
    async fn ttl(&self, l: LeaseId) -> astersql_pd_storage::Result<i64> {
        self.inner.ttl(l).await
    }
    async fn compact(&self, r: Revision) -> astersql_pd_storage::Result<()> {
        self.inner.compact(r).await
    }
}
async fn active(store: &ObservedStore, count: usize) {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while store.active.load(Ordering::SeqCst) != count {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("watch resource ownership did not converge");
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_033_disconnect_deadline_shutdown() {
    let mut fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(ObservedStore {
        inner: fixture.store().await,
        active: Arc::new(AtomicUsize::new(0)),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let cancel = CancellationToken::new();
    let services = Arc::new(WatchServices {
        store: store.clone(),
        identity: crate::Identity {
            cluster_id: 3301,
            member_id: 1,
            name: "watch-fault".into(),
            client_url: url.clone(),
            peer_url: "http://peer".into(),
        },
    });
    let stop = cancel.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, crate::router(services, stop.clone()))
            .with_graceful_shutdown(stop.cancelled_owned())
            .await
            .unwrap();
    });
    let mut client = PdClient::connect(url).await.unwrap();
    let revision = store
        .transact(
            vec![],
            vec![Mutation::put(b"/global/config/fault/a", b"before")],
        )
        .await
        .unwrap()
        .revision;
    let mut stream = client
        .watch_global_config(pdpb::WatchGlobalConfigRequest {
            revision,
            config_path: "/global/config/fault/".into(),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(stream.message().await.unwrap().unwrap().revision, revision);
    active(&store, 1).await;
    fixture.command("stop", "stopped");
    let pending = tokio::spawn(async move {
        let response = tokio::time::timeout(std::time::Duration::from_secs(20), stream.message())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        (response, stream)
    });
    // A real unavailable read proves the endpoint is down; the watch remains open.
    assert!(
        store
            .read(Read::prefix(
                b"/global/config/fault/",
                Consistency::Linearizable
            ))
            .await
            .is_err()
    );
    assert!(!pending.is_finished());
    fixture.command("start", "restarted");
    let recovered = fixture.store().await;
    let commit = recovered
        .transact(
            vec![],
            vec![Mutation::put(b"/global/config/fault/a", b"after")],
        )
        .await
        .unwrap();
    let (response, stream) = pending.await.unwrap();
    assert_eq!(response.revision, commit.revision);
    assert_eq!(response.changes[0].payload, b"after");
    drop(stream);
    active(&store, 0).await;
    // Zero revision watches future changes and does not replay the old value.
    let mut stream = client
        .watch_global_config(pdpb::WatchGlobalConfigRequest::default())
        .await
        .unwrap()
        .into_inner();
    active(&store, 1).await;
    let fresh = store
        .transact(
            vec![],
            vec![Mutation::put(b"/global/config/zero", b"fresh")],
        )
        .await
        .unwrap();
    let batch = tokio::time::timeout(std::time::Duration::from_secs(5), stream.message())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(batch.revision, fresh.revision);
    assert_eq!(batch.changes.len(), 1);
    assert_eq!(batch.changes[0].payload, b"fresh");
    drop(stream);
    active(&store, 0).await;
    let mut request = tonic::Request::new(pdpb::WatchGlobalConfigRequest {
        revision: fresh.revision + 1,
        config_path: "/global/config/fault/".into(),
    });
    request.set_timeout(std::time::Duration::from_millis(300));
    let mut stream = client
        .watch_global_config(request)
        .await
        .unwrap()
        .into_inner();
    active(&store, 1).await;
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(5), stream.message())
            .await
            .unwrap()
            .is_err()
    );
    drop(stream);
    active(&store, 0).await;
    let mut stream = client
        .watch_global_config(pdpb::WatchGlobalConfigRequest {
            revision: fresh.revision + 1,
            config_path: "/global/config/fault/".into(),
        })
        .await
        .unwrap()
        .into_inner();
    active(&store, 1).await;
    cancel.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(10), server)
        .await
        .unwrap()
        .unwrap();
    assert!(stream.message().await.unwrap().is_none());
    active(&store, 0).await;
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
async fn task_033_missing_backend() {
    use astersql_pd_proto::pdpb::pd_server::Pd;
    let grpc = crate::grpc::Grpc {
        services: Arc::new(NoBackend(crate::Identity {
            cluster_id: 33,
            member_id: 1,
            name: "no-backend".into(),
            client_url: "http://local".into(),
            peer_url: "http://peer".into(),
        })),
        cancel: CancellationToken::new(),
    };
    let error = grpc
        .watch_global_config(tonic::Request::new(pdpb::WatchGlobalConfigRequest {
            config_path: "/invalid".into(),
            ..Default::default()
        }))
        .await
        .err()
        .unwrap();
    assert_eq!(error.code(), tonic::Code::Unavailable);
    assert_eq!(error.message(), "server is started, but etcd not started");
}

struct WatchServices {
    store: Arc<dyn MetaStore>,
    identity: crate::Identity,
}
#[async_trait::async_trait]
impl crate::PdServices for WatchServices {
    fn identity(&self) -> &crate::Identity {
        &self.identity
    }
    fn discovery_store(&self) -> Option<Arc<dyn MetaStore>> {
        Some(self.store.clone())
    }
    async fn healthy(&self) -> Result<(), String> {
        Ok(())
    }
    async fn alloc_id(&self) -> Result<u64, String> {
        Err("unused".into())
    }
    async fn timestamp(&self, _: u32) -> Result<astersql_pd_tso::Timestamp, String> {
        Err("unused".into())
    }
}

#[tokio::test(start_paused = true)]
async fn task_033_controlled_deadline() {
    use astersql_pd_proto::pdpb::pd_server::Pd;
    use futures_util::StreamExt;
    let store = Arc::new(astersql_pd_storage::MemoryStore::default());
    let grpc = crate::grpc::Grpc {
        services: Arc::new(WatchServices {
            store,
            identity: crate::Identity {
                cluster_id: 33,
                member_id: 1,
                name: "clock".into(),
                client_url: "http://local".into(),
                peer_url: "http://peer".into(),
            },
        }),
        cancel: CancellationToken::new(),
    };
    let mut request = tonic::Request::new(pdpb::WatchGlobalConfigRequest {
        revision: 2,
        ..Default::default()
    });
    request.set_timeout(std::time::Duration::from_secs(3600));
    let mut stream = grpc
        .watch_global_config(request)
        .await
        .unwrap()
        .into_inner();
    let began = tokio::time::Instant::now();
    tokio::time::advance(std::time::Duration::from_secs(3600)).await;
    let status = stream.next().await.unwrap().unwrap_err();
    assert_eq!(status.code(), tonic::Code::DeadlineExceeded);
    assert_eq!(
        tokio::time::Instant::now() - began,
        std::time::Duration::from_secs(3600)
    );
    assert!(stream.next().await.is_none());
}
