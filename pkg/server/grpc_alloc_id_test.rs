// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_runtime::{NodeConfig, PdNode, PdNodeBuilder};
use astersql_pd_storage::{
    Commit, Compare, Consistency, Lease, LeaseId, MetaStore, MetaWatch, Mutation, RawTxnOp,
    RawTxnResponse, Read, Revision, Snapshot,
};
use std::{collections::BTreeSet, sync::Arc};
use tokio_util::sync::CancellationToken;

// Only gates real backend IO: failures still come from the real etcd transaction.
struct ReservationStore {
    inner: Arc<dyn MetaStore>,
    fixture: std::sync::Mutex<crate::etcd_fixture::Fixture>,
    mode: std::sync::atomic::AtomicU8,
    entered: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl MetaStore for ReservationStore {
    async fn read(&self, request: Read) -> astersql_pd_storage::Result<Snapshot> {
        self.inner.read(request).await
    }
    async fn transact(
        &self,
        compares: Vec<Compare>,
        writes: Vec<Mutation>,
    ) -> astersql_pd_storage::Result<Commit> {
        if writes
            .iter()
            .any(|write| matches!(write, Mutation::Put {key, ..} if key.ends_with(b"/alloc_id")))
        {
            match self.mode.swap(0, std::sync::atomic::Ordering::SeqCst) {
                1 => self.fixture.lock().unwrap().command("stop", "stopped"),
                2 => {
                    self.entered.notify_one();
                    std::future::pending::<()>().await;
                }
                _ => {}
            }
        }
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
async fn start(store: Arc<dyn MetaStore>, url: Option<&str>) -> (PdNode, String) {
    let listener =
        tokio::net::TcpListener::bind(url.unwrap_or("127.0.0.1:0").trim_start_matches("http://"))
            .await
            .unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            NodeConfig {
                cluster_id: 2100,
                member_id: 1,
                name: "alloc-id".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    (node, url)
}
fn request(count: u32) -> pdpb::AllocIdRequest {
    pdpb::AllocIdRequest {
        header: Some(pdpb::RequestHeader {
            cluster_id: 2100,
            ..Default::default()
        }),
        count,
    }
}
async fn boundary(store: &dyn MetaStore) -> u64 {
    let read = store
        .read(Read::prefix(
            b"/pd/2100/alloc_id",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    u64::from_be_bytes(read.entries[0].value.as_slice().try_into().unwrap())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_021_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    // This test asserts an exact allocator sequence. Keep the real scheduling
    // worker from reserving its own 128-ID window when the host is slow enough
    // to pass its prepare gate between these RPCs.
    astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone())
        .save_config(
            &astersql_pd_keypath::ClusterPaths::new(2100),
            &serde_json::json!({"schedule":{"halt-scheduling":"true"}}),
        )
        .await
        .unwrap();
    let (mut node, url) = start(store.clone(), None).await;
    let mut client = PdClient::connect(url.clone()).await.unwrap();
    let result = client.alloc_id(request(5)).await;
    assert!(result.is_ok(), "batch AllocID must be served: {result:?}");
    let reply = result.unwrap().into_inner();
    assert_eq!((reply.id, reply.count), (5, 5));
    assert!(reply.header.unwrap().error.is_none());
    for (count, id) in [(0, 6), (1, 7), (1005, 1012)] {
        let reply = client.alloc_id(request(count)).await.unwrap().into_inner();
        assert_eq!(
            (reply.id, reply.count),
            (id, if count > 1 { count } else { 0 })
        );
    }
    let mut workers = Vec::new();
    for _ in 0..12 {
        let mut client = client.clone();
        workers.push(tokio::spawn(async move {
            let r = client.alloc_id(request(17)).await.unwrap().into_inner();
            assert_eq!(r.count, 17);
            r.id - 16..=r.id
        }));
    }
    let mut seen = BTreeSet::new();
    for worker in workers {
        for id in worker.await.unwrap() {
            assert!(seen.insert(id));
        }
    }
    assert_eq!(seen.len(), 204);
    assert!(*seen.first().unwrap() > 1012);
    assert!(*seen.last().unwrap() >= 1216);
    assert_eq!(boundary(store.as_ref()).await, 2000);
    assert_eq!(
        client
            .alloc_id(pdpb::AllocIdRequest::default())
            .await
            .unwrap_err()
            .code(),
        tonic::Code::FailedPrecondition
    );
    let ping = std::process::Command::new("curl")
        .args(["-fsS", &format!("{url}/pd/api/v1/ping")])
        .output()
        .unwrap();
    assert!(ping.status.success());
    node.shutdown().await.unwrap();
    let (mut node, _) = start(store.clone(), Some(&url)).await;
    let mut client = PdClient::connect(url.clone()).await.unwrap();
    let r = client.alloc_id(request(2)).await.unwrap().into_inner();
    assert_eq!((r.id, r.count), (2002, 2));
    node.shutdown().await.unwrap();
    // Inject a real durable boundary, then exercise exhaustion through TCP.
    store
        .transact(
            vec![],
            vec![Mutation::put(
                b"/pd/2100/alloc_id",
                &(u64::MAX - 1).to_be_bytes(),
            )],
        )
        .await
        .unwrap();
    let (mut node, _) = start(store.clone(), Some(&url)).await;
    let mut client = PdClient::connect(url).await.unwrap();
    let r = client.alloc_id(request(2)).await.unwrap().into_inner();
    assert_eq!((r.id, r.count), (0, 0));
    let error = r.header.unwrap().error.unwrap();
    assert_eq!(error.r#type, pdpb::ErrorType::Unknown as i32);
    assert_eq!(error.message, "[PD:idalloc:ErrIDExhausted]id exhausted");
    assert_eq!(boundary(store.as_ref()).await, u64::MAX);
    assert!(
        client
            .alloc_id(request(1))
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .error
            .is_some()
    );
    node.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_021_storage_error_and_cancel() {
    use crate::{Identity, PdServices};
    use astersql_pd_id::Allocator;
    use astersql_pd_member::Member;
    struct Host {
        identity: Identity,
        allocator: Allocator,
    }
    #[async_trait::async_trait]
    impl PdServices for Host {
        fn identity(&self) -> &Identity {
            &self.identity
        }
        async fn healthy(&self) -> Result<(), String> {
            Ok(())
        }
        async fn alloc_id(&self) -> Result<u64, String> {
            self.allocator.alloc().await.map_err(|e| e.to_string())
        }
        async fn alloc_ids(&self, count: u32) -> Result<(u64, u32), String> {
            self.allocator
                .alloc_count(count)
                .await
                .map_err(|e| e.to_string())
        }
        async fn timestamp(&self, _: u32) -> Result<astersql_pd_tso::Timestamp, String> {
            unreachable!()
        }
    }
    let fixture = crate::etcd_fixture::Fixture::start();
    let inner: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let store = Arc::new(ReservationStore {
        inner,
        fixture: std::sync::Mutex::new(fixture),
        mode: std::sync::atomic::AtomicU8::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let cancel = CancellationToken::new();
    let member = Member::bootstrap(store.clone(), cancel.clone(), 2100, 1, b"fault".to_vec())
        .await
        .unwrap();
    let leader = Arc::new(member.campaign(30).await.unwrap());
    let host = Host {
        identity: Identity {
            cluster_id: 2100,
            member_id: 1,
            name: "fault".into(),
            client_url: "".into(),
            peer_url: "".into(),
        },
        allocator: Allocator::new(store.clone(), leader.clone(), cancel.clone(), 1000).unwrap(),
    };
    store.mode.store(1, std::sync::atomic::Ordering::SeqCst);
    let response = crate::grpc_alloc_id::handle(&host, &cancel, tonic::Request::new(request(2)))
        .await
        .unwrap()
        .into_inner();
    assert_eq!((response.id, response.count), (0, 0));
    assert_eq!(
        response.header.unwrap().error.unwrap().r#type,
        pdpb::ErrorType::Unknown as i32
    );
    store.fixture.lock().unwrap().command("start", "restarted");
    assert!(
        store
            .read(Read::prefix(
                b"/pd/2100/alloc_id",
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    let response = crate::grpc_alloc_id::handle(&host, &cancel, tonic::Request::new(request(2)))
        .await
        .unwrap()
        .into_inner();
    assert_eq!((response.id, response.count), (2, 2));
    // Force a second reservation and cancel while the real transaction is pending.
    let request_cancel = CancellationToken::new();
    store.mode.store(2, std::sync::atomic::Ordering::SeqCst);
    let pending =
        crate::grpc_alloc_id::handle(&host, &request_cancel, tonic::Request::new(request(1001)));
    tokio::pin!(pending);
    tokio::select! {
        _ = store.entered.notified() => {},
        result = &mut pending => panic!("reservation should be pending: {result:?}"),
    }
    request_cancel.cancel();
    assert_eq!(pending.await.unwrap_err().code(), tonic::Code::Unavailable);
    assert_eq!(boundary(store.as_ref()).await, 1000);
    // IDs already consumed in the interrupted batch stay consumed, never reused.
    let r = crate::grpc_alloc_id::handle(&host, &cancel, tonic::Request::new(request(1)))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(r.id, 1001);
    cancel.cancel();
    assert_eq!(
        crate::grpc_alloc_id::handle(&host, &cancel, tonic::Request::new(request(1)))
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Unavailable
    );
    leader.resign().await.unwrap();
}
