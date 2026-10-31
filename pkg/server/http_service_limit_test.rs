// Copyright 2026 AsterSQL.
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

fn call(url: &str, method: &str, path: &str, body: &str) -> (u16, String) {
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "5",
            "-X",
            method,
            "--data-binary",
            body,
            "-w",
            "\n%{http_code}",
            &format!("{url}/pd/api/v1/{path}"),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    let (body, status) = text.rsplit_once('\n').unwrap();
    (status.parse().unwrap(), body.into())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_079_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 779,
                member_id: 1,
                name: "limit".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let response = call(
        &url,
        "POST",
        "service-middleware/config/rate-limit",
        r#"{"type":"label","label":"GetHealthStatus","qps":0.0001}"#,
    );
    assert_eq!(response, (200, "Rate limiter is updated.".into()));
    assert_eq!(call(&url, "GET", "health", "").0, 200);
    assert_eq!(call(&url, "GET", "health", "").0, 429);
    // HTTP label limits do not intercept the co-hosted real tonic service.
    let mut client = astersql_pd_proto::pdpb::pd_client::PdClient::connect(url.clone())
        .await
        .unwrap();
    let members = client
        .get_members(astersql_pd_proto::pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(members.members.len(), 1);
    node.shutdown().await.unwrap();
}

/// Shared wire observations also run against an isolated, pre-existing Go binary.
fn observe(url: &str) -> Vec<serde_json::Value> {
    use serde_json::{Value, json};
    let mut out = vec![];
    let mut run = |method: &str, path: &str, body: &str| {
        let (status, body) = call(url, method, path, body);
        let body = serde_json::from_str::<Value>(&body).unwrap_or(json!(body));
        out.push(json!([status, body]));
        status
    };
    let rate = "service-middleware/config/rate-limit";
    for body in [
        r#"{}"#,
        r#"{"type":"x"}"#,
        r#"{"type":"label"}"#,
        r#"{"type":"label","label":"missing"}"#,
        r#"{"type":"path"}"#,
        r#"{"type":"path","path":"/pd/api/v1/health"}"#,
        r#"{"type":"label","label":"SetRateLimitConfig","qps":1}"#,
        r#"{"type":"label","label":"SetGRPCRateLimitConfig","concurrency":1}"#,
    ] {
        assert_eq!(run("POST", rate, body), 400);
    }
    assert_eq!(
        run(
            "POST",
            rate,
            r#"{"type":"label","label":"GetHealthStatus"}"#
        ),
        200
    );
    assert_eq!(
        run(
            "POST",
            rate,
            r#"{"type":"path","path":"/pd/api/v1/health","method":"GET","qps":0.0001}"#
        ),
        200
    );
    // Do not compare member identities: observe the independent ping endpoint and
    // rate-limit errors, while actual successful health is verified separately.
    let _ = call(url, "GET", "health", "");
    assert_eq!(run("GET", "health", ""), 429);
    assert_eq!(
        run(
            "POST",
            "service-middleware/config",
            r#"{"enable-rate-limit":"false"}"#
        ),
        200
    );
    assert_eq!(call(url, "GET", "health", "").0, 200);
    assert_eq!(
        run(
            "POST",
            "service-middleware/config",
            r#"{"rate-limit.enable-rate-limit":"true"}"#
        ),
        200
    );
    assert_eq!(run("GET", "health", ""), 429);
    assert_eq!(
        run(
            "POST",
            rate,
            r#"{"type":"label","label":"GetHealthStatus","qps":0}"#
        ),
        200
    );
    assert_eq!(call(url, "GET", "health", "").0, 200);
    assert_eq!(
        run(
            "POST",
            rate,
            r#"{"type":"label","label":"SetServiceMiddlewareConfig","concurrency":1}"#
        ),
        200
    );
    assert_eq!(
        run(
            "POST",
            "service-middleware/config",
            r#"{"enable-audit":"false"}"#
        ),
        200
    );
    assert_eq!(run("POST", "service-middleware/config", "{}"), 200);
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_079_wire_and_cancellation() {
    use astersql_pd_storage::{Consistency, MetaStore, Read};
    use tokio::io::AsyncWriteExt;
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");
    let cfg = NodeConfig {
        cluster_id: 7791,
        member_id: 1,
        name: "wire-limit".into(),
        client_url: url.clone(),
        peer_url: "http://peer".into(),
    };
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            cfg.clone(),
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let observations = observe(&url);
    if let Ok(path) = std::env::var("PD_HTTP_SERVICE_LIMIT_ORACLE") {
        let go: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(serde_json::json!(observations), go["responses"]);
    }
    let mut hanging = tokio::net::TcpStream::connect(addr).await.unwrap();
    hanging.write_all(b"POST /pd/api/v1/service-middleware/config HTTP/1.1\r\nHost: localhost\r\nContent-Length: 999999\r\n\r\n").await.unwrap();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if call(&url, "POST", "service-middleware/config", "{}").0 == 429 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "pending HTTP request was not admitted"
        );
        tokio::task::yield_now().await;
    }
    drop(hanging);
    loop {
        if call(&url, "POST", "service-middleware/config", "{}").0 == 200 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "cancelled request leaked its concurrency slot"
        );
        tokio::task::yield_now().await;
    }
    let persisted = store
        .read(Read::prefix(
            b"/pd/7791/service_middleware",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert_eq!(persisted.entries.len(), 1);
    let value: serde_json::Value = serde_json::from_slice(&persisted.entries[0].value).unwrap();
    assert_eq!(
        value["rate-limit"]["limiter-config"]["SetServiceMiddlewareConfig"]["ConcurrencyLimit"],
        1
    );
    if let Ok(path) = std::env::var("PD_HTTP_SERVICE_LIMIT_ORACLE") {
        let go: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(value, go["config"][1]);
    }
    node.shutdown().await.unwrap();
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(store, cfg, listener, Arc::new(prometheus::Registry::new()))
        .await
        .unwrap();
    node.ready().await.unwrap();
    let (_, body) = call(&url, "GET", "service-middleware/config", "");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).unwrap(),
        value
    );
    node.shutdown().await.unwrap();
}

use astersql_pd_storage::{
    Commit, Compare, Lease, LeaseId, MetaStore, MetaWatch, Mutation, RawTxnOp, RawTxnResponse,
    Read, Revision, Snapshot,
};
pub(super) struct LimitStore {
    pub(super) inner: Arc<dyn MetaStore>,
    pub(super) mode: std::sync::atomic::AtomicU8,
}
#[async_trait::async_trait]
impl MetaStore for LimitStore {
    async fn consensus_cluster_id(&self) -> astersql_pd_storage::Result<Option<u64>> {
        self.inner.consensus_cluster_id().await
    }
    async fn consensus_leader_id(&self) -> astersql_pd_storage::Result<Option<u64>> {
        self.inner.consensus_leader_id().await
    }
    async fn read(&self, request: Read) -> astersql_pd_storage::Result<Snapshot> {
        self.inner.read(request).await
    }
    async fn transact(
        &self,
        compares: Vec<Compare>,
        mut writes: Vec<Mutation>,
    ) -> astersql_pd_storage::Result<Commit> {
        for write in &mut writes {
            if let Mutation::Put { key, lease, .. } = write {
                if key.ends_with(b"/service_middleware")
                    && self.mode.swap(0, std::sync::atomic::Ordering::SeqCst) == 1
                {
                    *lease = Some(i64::MAX);
                }
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
async fn task_079_transaction_failure() {
    use std::sync::atomic::Ordering;
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(LimitStore {
        inner: Arc::new(fixture.store().await),
        mode: std::sync::atomic::AtomicU8::new(0),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 7792,
                member_id: 1,
                name: "fault-limit".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let rate = "service-middleware/config/rate-limit";
    assert_eq!(
        call(
            &url,
            "POST",
            rate,
            r#"{"type":"label","label":"GetHealthStatus","qps":0.0001}"#
        )
        .0,
        200
    );
    let before = call(&url, "GET", "service-middleware/config", "");
    let _ = call(&url, "GET", "health", "");
    store.mode.store(1, Ordering::SeqCst);
    assert_eq!(
        call(
            &url,
            "POST",
            rate,
            r#"{"type":"label","label":"GetHealthStatus","qps":0}"#
        )
        .0,
        500
    );
    assert_eq!(call(&url, "GET", "service-middleware/config", ""), before);
    // A failed deletion recreates Go's bucket with a fresh burst token.
    assert_eq!(call(&url, "GET", "health", "").0, 200);
    assert_eq!(call(&url, "GET", "health", "").0, 429);
    store.mode.store(1, Ordering::SeqCst);
    assert_eq!(
        call(
            &url,
            "POST",
            "service-middleware/config",
            r#"{"enable-rate-limit":"false"}"#
        )
        .0,
        500
    );
    assert_eq!(call(&url, "GET", "service-middleware/config", ""), before);
    assert_eq!(call(&url, "GET", "health", "").0, 429);
    node.shutdown().await.unwrap();
}
