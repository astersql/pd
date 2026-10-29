// Copyright 2026 AsterSQL.
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
fn header() -> Option<pdpb::RequestHeader> {
    Some(pdpb::RequestHeader {
        cluster_id: 2800,
        ..Default::default()
    })
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_028_behavior() {
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
                    cluster_id: 2800,
                    member_id: 1,
                    name: "config".into(),
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
        let response = client
            .get_cluster_config(pdpb::GetClusterConfigRequest { header: header() })
            .await;
        assert!(
            response.is_ok(),
            "cluster config RPC must serve requests: {response:?}"
        );
        let response = response.unwrap().into_inner();
        if !restart {
            assert_eq!(
                response.header.unwrap().error.unwrap().r#type,
                pdpb::ErrorType::NotBootstrapped as i32
            );
            let put = client
                .put_cluster_config(pdpb::PutClusterConfigRequest {
                    header: header(),
                    cluster: None,
                })
                .await
                .unwrap()
                .into_inner();
            assert_eq!(
                put.header.unwrap().error.unwrap().r#type,
                pdpb::ErrorType::NotBootstrapped as i32
            );
            super::grpc_region_lookup_scenarios_test::prepare(&mut client, 2800).await;
            for count in [0, 1, 5, u32::MAX] {
                let response = client
                    .put_cluster_config(pdpb::PutClusterConfigRequest {
                        header: header(),
                        cluster: Some(metapb::Cluster {
                            id: 2800,
                            max_peer_count: count,
                        }),
                    })
                    .await
                    .unwrap()
                    .into_inner();
                assert!(response.header.unwrap().error.is_none());
                assert_eq!(
                    client
                        .get_cluster_config(pdpb::GetClusterConfigRequest { header: header() })
                        .await
                        .unwrap()
                        .into_inner()
                        .cluster
                        .unwrap()
                        .max_peer_count,
                    count
                );
            }
            for cluster in [
                None,
                Some(metapb::Cluster {
                    id: 7,
                    max_peer_count: 9,
                }),
            ] {
                let response = client
                    .put_cluster_config(pdpb::PutClusterConfigRequest {
                        header: header(),
                        cluster,
                    })
                    .await
                    .unwrap()
                    .into_inner();
                assert_eq!(
                    response.header.unwrap().error.unwrap().r#type,
                    pdpb::ErrorType::Unknown as i32
                );
            }
            assert!(
                client
                    .get_cluster_config(pdpb::GetClusterConfigRequest {
                        header: Some(pdpb::RequestHeader {
                            cluster_id: 7,
                            ..Default::default()
                        })
                    })
                    .await
                    .is_err()
            );
        } else {
            assert_eq!(
                response.cluster.unwrap(),
                metapb::Cluster {
                    id: 2800,
                    max_peer_count: u32::MAX
                }
            );
        }
        let endpoint = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
        assert_eq!(
            endpoint
                .load_meta(&astersql_pd_keypath::ClusterPaths::new(2800))
                .await
                .unwrap()
                .unwrap()
                .max_peer_count,
            u32::MAX
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
                .get_cluster_config(pdpb::GetClusterConfigRequest { header: header() })
                .await
                .is_err()
        );
        node.shutdown().await.unwrap();
    }
}

use astersql_pd_storage::{
    Commit, Compare, Lease, LeaseId, MetaStore, MetaWatch, Mutation, RawTxnOp, RawTxnResponse,
    Read, Revision, Snapshot,
};

// Only gates real backend IO: failures still come from the real etcd transaction.
struct RegistrationStore {
    inner: Arc<dyn MetaStore>,
    _fixture: std::sync::Mutex<crate::etcd_fixture::Fixture>,
    mode: std::sync::atomic::AtomicU8,
    entered: tokio::sync::Notify,
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
        if writes.iter().any(
            |write| matches!(write, Mutation::Put {key, ..} if key.as_slice() == b"/pd/2800/raft"),
        ) {
            match self.mode.swap(0, std::sync::atomic::Ordering::SeqCst) {
                1 => writes.push(Mutation::put(
                    b"/task028/oversized",
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_028_transaction_failure_and_cancel() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let backend = Arc::new(RegistrationStore {
        inner: Arc::new(fixture.store().await),
        _fixture: std::sync::Mutex::new(fixture),
        mode: std::sync::atomic::AtomicU8::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let cancel = CancellationToken::new();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .start(
            backend.clone(),
            NodeConfig {
                cluster_id: 2800,
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
    let mut client = PdClient::connect(url).await.unwrap();
    super::grpc_region_lookup_scenarios_test::prepare(&mut client, 2800).await;
    let before = client
        .get_cluster_config(pdpb::GetClusterConfigRequest { header: header() })
        .await
        .unwrap()
        .into_inner();
    backend.mode.store(1, std::sync::atomic::Ordering::SeqCst);
    let response = client
        .put_cluster_config(pdpb::PutClusterConfigRequest {
            header: header(),
            cluster: Some(metapb::Cluster {
                id: 2800,
                max_peer_count: 99,
            }),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(
        response.header.unwrap().error.is_some(),
        "real etcd must reject oversized transaction"
    );
    assert_eq!(
        before,
        client
            .get_cluster_config(pdpb::GetClusterConfigRequest { header: header() })
            .await
            .unwrap()
            .into_inner()
    );
    backend.mode.store(2, std::sync::atomic::Ordering::SeqCst);
    let pending = tokio::spawn(async move {
        client
            .put_cluster_config(pdpb::PutClusterConfigRequest {
                header: header(),
                cluster: Some(metapb::Cluster {
                    id: 2800,
                    max_peer_count: 99,
                }),
            })
            .await
    });
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        backend.entered.notified(),
    )
    .await
    .unwrap();
    cancel.cancel();
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(5), pending)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    node.shutdown().await.unwrap();
    let endpoint = astersql_pd_storage_endpoint::StorageEndpoint::new(backend.inner.clone());
    assert_eq!(
        endpoint
            .load_meta(&astersql_pd_keypath::ClusterPaths::new(2800))
            .await
            .unwrap()
            .unwrap()
            .max_peer_count,
        before.cluster.unwrap().max_peer_count
    );
}
