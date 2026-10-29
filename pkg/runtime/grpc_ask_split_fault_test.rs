// Copyright 2026 AsterSQL.
use crate::{NodeConfig, PdNodeBuilder};
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_storage::{
    Commit, Compare, Consistency, Lease, LeaseId, MetaStore, MetaWatch, Mutation, RawTxnOp,
    RawTxnResponse, Read, Revision, Snapshot,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
// Only gates real backend IO: failures still come from the real etcd transaction.
pub(super) struct ReservationStore {
    pub(super) inner: Arc<dyn MetaStore>,
    pub(super) _fixture: std::sync::Mutex<crate::node_test::fixture::Fixture>,
    pub(super) mode: std::sync::atomic::AtomicU8,
    pub(super) entered: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl MetaStore for ReservationStore {
    async fn read(&self, request: Read) -> astersql_pd_storage::Result<Snapshot> {
        self.inner.read(request).await
    }
    async fn transact(
        &self,
        compares: Vec<Compare>,
        mut writes: Vec<Mutation>,
    ) -> astersql_pd_storage::Result<Commit> {
        if writes
            .iter()
            .any(|write| matches!(write, Mutation::Put {key, ..} if key.ends_with(b"/alloc_id")))
        {
            match self.mode.swap(0, std::sync::atomic::Ordering::SeqCst) {
                1 => writes.push(Mutation::put(
                    b"/task026/oversized",
                    &vec![0; 2 * 1024 * 1024],
                )),
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

#[path = "../server/grpc_region_lookup_scenarios_test.rs"]
mod prepare;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_026_storage_error_and_cancel() {
    let fixture = crate::node_test::fixture::Fixture::start();
    let store = Arc::new(ReservationStore {
        inner: Arc::new(fixture.store().await),
        _fixture: std::sync::Mutex::new(fixture),
        mode: std::sync::atomic::AtomicU8::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let cancel = CancellationToken::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 2602,
                member_id: 1,
                name: "split-fault".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let mut client = PdClient::connect(url).await.unwrap();
    prepare::prepare(&mut client, 2602).await;
    client
        .put_store(pdpb::PutStoreRequest {
            header: prepare::header(2602),
            store: Some(astersql_pd_proto::metapb::Store {
                id: 1,
                version: "8.5.0".into(),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap();
    let region = client
        .get_region_by_id(pdpb::GetRegionByIdRequest {
            header: prepare::header(2602),
            region_id: 10,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .region;
    let request = pdpb::AskBatchSplitRequest {
        header: prepare::header(2602),
        region,
        split_count: 2,
        ..Default::default()
    };
    // The coordinator may already have reserved IDs. Drain the current real
    // allocation window so this split necessarily crosses a durable boundary.
    let allocated = client
        .alloc_id(pdpb::AllocIdRequest {
            header: prepare::header(2602),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .id;
    let remaining = (1000 - allocated % 1000) % 1000;
    if remaining > 0 {
        client
            .alloc_id(pdpb::AllocIdRequest {
                header: prepare::header(2602),
                count: remaining as u32,
            })
            .await
            .unwrap();
    }
    let key = b"/pd/2602/alloc_id";
    let before = store
        .read(Read::prefix(key, Consistency::Linearizable))
        .await
        .unwrap()
        .entries;
    store.mode.store(1, std::sync::atomic::Ordering::SeqCst);
    let r = client
        .ask_batch_split(request.clone())
        .await
        .unwrap()
        .into_inner();
    assert!(r.ids.is_empty());
    assert_eq!(
        r.header.unwrap().error.unwrap().r#type,
        pdpb::ErrorType::Unknown as i32
    );
    assert_eq!(
        store
            .read(Read::prefix(key, Consistency::Linearizable))
            .await
            .unwrap()
            .entries,
        before
    );
    let r = client
        .ask_batch_split(request.clone())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(r.ids.len(), 2);
    assert!(r.header.unwrap().error.is_none());
    let first = r.ids[0].new_region_id;
    assert!(first > allocated);
    assert_eq!(r.ids[1].new_peer_ids.last(), Some(&(first + 7)));
    let boundary = store
        .read(Read::prefix(key, Consistency::Linearizable))
        .await
        .unwrap()
        .entries;
    let mut large = request.clone();
    large.split_count = 300;
    store.mode.store(2, std::sync::atomic::Ordering::SeqCst);
    let mut c = client.clone();
    let pending = tokio::spawn(async move { c.ask_batch_split(large).await });
    tokio::time::timeout(std::time::Duration::from_secs(20), store.entered.notified())
        .await
        .unwrap();
    cancel.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), pending)
        .await
        .unwrap()
        .unwrap();
    assert!(result.is_err());
    node.shutdown().await.unwrap();
    assert_eq!(
        store
            .read(Read::prefix(key, Consistency::Linearizable))
            .await
            .unwrap()
            .entries,
        boundary
    );
    assert!(
        store
            .read(Read::prefix(
                b"/task026/oversized",
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
}
