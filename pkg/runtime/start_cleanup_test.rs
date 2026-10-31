// Copyright 2026 AsterSQL.
use crate::{NodeConfig, PdNodeBuilder};
use astersql_pd_storage::{
    Commit, Compare, Lease, LeaseId, MetaStore, MetaWatch, Mutation, RawTxnOp, RawTxnResponse,
    Read, Revision, Snapshot,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
// Only gates real backend IO: failures still come from the real etcd transaction.
pub(super) struct StartupStore {
    pub(super) inner: Arc<dyn MetaStore>,
    pub(super) mode: std::sync::atomic::AtomicU8,
    pub(super) entered: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl MetaStore for StartupStore {
    async fn read(&self, request: Read) -> astersql_pd_storage::Result<Snapshot> {
        self.inner.read(request).await
    }
    async fn transact(
        &self,
        compares: Vec<Compare>,
        mut writes: Vec<Mutation>,
    ) -> astersql_pd_storage::Result<Commit> {
        let mode = self.mode.load(std::sync::atomic::Ordering::SeqCst);
        let hit = writes.iter().any(|write| match write {
            Mutation::Put { key, .. } => {
                (mode == 1 && key.ends_with(b"/timestamp"))
                    || (mode == 3 && key.windows(13).any(|v| v == b"/rust/members"))
            }
            Mutation::Delete { key } => mode == 2 && key.windows(13).any(|v| v == b"/join-pending"),
            _ => false,
        });
        if hit {
            self.mode.store(0, std::sync::atomic::Ordering::SeqCst);
            writes.push(Mutation::put(
                b"/task093/oversized",
                &vec![0; 2 * 1024 * 1024],
            ));
            self.entered.notify_one();
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
async fn task_093_behavior() {
    use astersql_pd_storage::{Consistency, Read};
    use std::sync::atomic::AtomicU8;
    for mode in [
        "storage", "role", "registry", "listener", "worker", "cancel",
    ] {
        let fixture = crate::node_test::fixture::Fixture::start();
        let inner: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
        let store = Arc::new(StartupStore {
            inner: inner.clone(),
            mode: AtomicU8::new(match mode {
                "role" => 1,
                "worker" => 2,
                "storage" => 3,
                _ => 0,
            }),
            entered: tokio::sync::Notify::new(),
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let config = NodeConfig {
            cluster_id: 9300,
            member_id: 93,
            name: "cleanup".into(),
            client_url: format!("http://{address}"),
            peer_url: "http://127.0.0.1:2380".into(),
        };
        let registry = Arc::new(prometheus::Registry::new());
        let existing = prometheus::IntCounter::new(
            "pd_rust_requests_total",
            "Minimal Rust PD domain requests",
        )
        .unwrap();
        if mode == "registry" {
            registry.register(Box::new(existing.clone())).unwrap();
        }
        if mode == "worker" {
            inner
                .transact(
                    vec![],
                    vec![Mutation::put(
                        b"/pd/9300/rust/join-pending/93",
                        &crate::join_resume::value(&config),
                    )],
                )
                .await
                .unwrap();
        }
        let parent = CancellationToken::new();
        if mode == "cancel" {
            parent.cancel();
        }
        let mut builder = PdNodeBuilder::new(tokio::runtime::Handle::current(), parent.clone());
        if mode == "listener" {
            let mut tls = astersql_pd_server_config::Config::default();
            tls.security.tls.cert_path = "missing-cert".into();
            builder = builder.with_listener_config(tls);
        }
        let result = builder
            .start(store.clone(), config.clone(), listener, registry.clone())
            .await;
        let error = match result {
            Err(e) => e,
            Ok(mut node) => {
                let error = node
                    .ready()
                    .await
                    .expect_err("worker startup must fail readiness");
                assert!(
                    error
                        .to_string()
                        .contains("unexpected metadata backend response"),
                    "original backend error lost: {error}"
                );
                error
            }
        };
        println!("{mode}: {error}");
        assert!(!parent.is_cancelled() || mode == "cancel");
        if mode == "registry" {
            registry.unregister(Box::new(existing)).unwrap();
        }
        assert!(
            registry.gather().is_empty(),
            "{mode}: startup collectors leaked"
        );
        assert!(
            inner
                .read(Read::prefix(b"/pd/9300/leader", Consistency::Linearizable))
                .await
                .unwrap()
                .entries
                .is_empty(),
            "{mode}: lease leaked"
        );
        assert!(
            inner
                .read(Read::prefix(
                    b"/task093/oversized",
                    Consistency::Linearizable
                ))
                .await
                .unwrap()
                .entries
                .is_empty()
        );
        let listener = tokio::net::TcpListener::bind(address).await.unwrap();
        let mut node =
            PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
                .start(inner.clone(), config.clone(), listener, registry.clone())
                .await
                .unwrap();
        node.ready().await.unwrap();
        let mut client =
            astersql_pd_proto::pdpb::pd_client::PdClient::connect(config.client_url.clone())
                .await
                .unwrap();
        assert_eq!(
            client
                .get_members(astersql_pd_proto::pdpb::GetMembersRequest::default())
                .await
                .unwrap()
                .into_inner()
                .leader
                .unwrap()
                .member_id,
            93
        );
        let response = std::process::Command::new("curl")
            .args(["-fsS", &format!("{}/pd/api/v1/health", config.client_url)])
            .output()
            .unwrap();
        assert!(response.status.success());
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&response.stdout).unwrap()[0]["health"],
            true
        );
        drop(client);
        node.shutdown().await.unwrap();
        assert!(registry.gather().is_empty());
        assert!(tokio::net::TcpListener::bind(address).await.is_ok());
    }
}

#[test]
fn task_093_cleanup_error_keeps_original_source() {
    let original: crate::NodeError = astersql_pd_storage::StoreError::RequestTooLarge.into();
    let error = crate::start_cleanup::preserve(
        original,
        Err(astersql_pd_storage::StoreError::Unavailable.into()),
    );
    assert_eq!(
        error
            .source()
            .unwrap()
            .downcast_ref::<astersql_pd_storage::StoreError>(),
        Some(&astersql_pd_storage::StoreError::RequestTooLarge)
    );
    assert!(error.to_string().contains("startup cleanup failed"));
}
