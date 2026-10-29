// Copyright 2026 AsterSQL.
use super::grpc_store_differential_test::scenarios::{get, header, meta, put};
use crate::{NodeConfig, PdNodeBuilder};
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_storage::{
    Commit, Compare, Consistency, Lease, LeaseId, MetaStore, MetaWatch, Mutation, RawTxnOp,
    RawTxnResponse, Read, Revision, Snapshot,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
// Only gates real backend IO: failures still come from the real etcd transaction.
pub(super) struct RegistrationStore {
    pub(super) inner: Arc<dyn MetaStore>,
    pub(super) _fixture: std::sync::Mutex<crate::node_test::fixture::Fixture>,
    pub(super) mode: std::sync::atomic::AtomicU8,
    pub(super) entered: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl MetaStore for RegistrationStore {
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
            .any(|write| matches!(write, Mutation::Put {key, ..} if key.windows(8).any(|w| w == b"/raft/s/")))
        {
            match self.mode.swap(0, std::sync::atomic::Ordering::SeqCst) {
                1 => writes.push(Mutation::put(b"/task022/oversized", &vec![0; 2 * 1024 * 1024])),
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

pub(super) struct Host {
    pub(super) cluster: Arc<crate::cluster_host::ClusterHost>,
    pub(super) identity: astersql_pd_server::Identity,
}
#[async_trait::async_trait]
impl astersql_pd_server::PdServices for Host {
    fn identity(&self) -> &astersql_pd_server::Identity {
        &self.identity
    }
    fn region_buckets_enabled(&self) -> bool {
        true
    }
    fn cluster_read_service(&self) -> Option<&dyn astersql_pd_server::ClusterReadService> {
        Some(self.cluster.as_ref())
    }
    async fn put_store(
        &self,
        store: astersql_pd_core::StoreMeta,
    ) -> Result<(), astersql_pd_server::grpc_store::StoreError> {
        self.cluster.register_store(store).await
    }
    async fn stores(&self) -> Result<Vec<astersql_pd_core::StoreMeta>, String> {
        self.cluster.registered_stores().await
    }
    async fn healthy(&self) -> Result<(), String> {
        Ok(())
    }
    async fn alloc_id(&self) -> Result<u64, String> {
        unreachable!()
    }
    async fn timestamp(&self, _: u32) -> Result<astersql_pd_tso::Timestamp, String> {
        unreachable!()
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_022_transaction_failure_and_cancel() {
    let fixture = crate::node_test::fixture::Fixture::start();
    let inner: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let backend = Arc::new(RegistrationStore {
        inner,
        _fixture: std::sync::Mutex::new(fixture),
        mode: std::sync::atomic::AtomicU8::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            backend.clone(),
            NodeConfig {
                cluster_id: 2201,
                member_id: 1,
                name: "store-fault".into(),
                client_url: url,
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let cancel = CancellationToken::new();
    let host = Arc::new(Host {
        cluster: node.cluster(),
        identity: astersql_pd_server::Identity {
            cluster_id: 2201,
            member_id: 1,
            name: "store-fault".into(),
            client_url: url.clone(),
            peer_url: "http://peer".into(),
        },
    });
    let router = astersql_pd_server::router(host, cancel.clone());
    let shutdown = CancellationToken::new();
    let stopped = shutdown.clone();
    let serving = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(stopped.cancelled_owned())
            .await
            .unwrap();
    });
    let mut client = PdClient::connect(url).await.unwrap();
    assert!(
        put(&mut client, 2201, meta(4, "before"))
            .await
            .header
            .unwrap()
            .error
            .is_none()
    );
    backend.mode.store(1, std::sync::atomic::Ordering::SeqCst);
    let error = put(&mut client, 2201, meta(4, "failed"))
        .await
        .header
        .unwrap()
        .error
        .unwrap();
    assert_eq!(error.r#type, pdpb::ErrorType::Unknown as i32);
    assert_eq!(
        get(&mut client, 2201, 4).await.store.unwrap().address,
        "before"
    );
    let key = b"/pd/2201/raft/s/00000000000000000004";
    use prost::Message;
    let saved = backend
        .read(Read::prefix(key, Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(
        astersql_pd_proto::metapb::Store::decode(saved.entries[0].value.as_slice())
            .unwrap()
            .address,
        "before"
    );
    backend.mode.store(2, std::sync::atomic::Ordering::SeqCst);
    let mut pending_client = client.clone();
    let pending = tokio::spawn(async move {
        pending_client
            .put_store(pdpb::PutStoreRequest {
                header: header(2201),
                store: Some(meta(5, "cancelled")),
            })
            .await
    });
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        backend.entered.notified(),
    )
    .await
    .unwrap();
    cancel.cancel();
    assert_eq!(
        pending.await.unwrap().unwrap_err().code(),
        tonic::Code::Unavailable
    );
    assert!(
        backend
            .read(Read::prefix(
                b"/pd/2201/raft/s/00000000000000000005",
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    assert!(
        node.cluster()
            .registered_stores()
            .await
            .unwrap()
            .iter()
            .all(|s| s.id != 5)
    );
    assert!(
        backend
            .read(Read::prefix(
                b"/task022/oversized",
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    assert_eq!(
        client
            .get_all_stores(pdpb::GetAllStoresRequest {
                header: header(2201),
                ..Default::default()
            })
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Unavailable
    );
    // Dropping the interrupted handler releases admission; a later host can retry.
    node.cluster()
        .register_store(
            astersql_pd_server::core_store_adapter::store_from_proto(Some(meta(5, "retry")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        node.cluster()
            .registered_stores()
            .await
            .unwrap()
            .iter()
            .any(|s| s.id == 5 && s.address == "retry")
    );
    shutdown.cancel();
    serving.await.unwrap();
    node.shutdown().await.unwrap();
}
