// Copyright 2026 AsterSQL.
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_071_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            NodeConfig {
                cluster_id: 768,
                member_id: 1,
                name: "rule".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    assert_eq!(
        call(&url, ""),
        (
            500,
            serde_json::json!(
                "[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first"
            )
        )
    );
    assert_eq!(call(&url, "?ids=a%2Fb").0, 500);
    assert_eq!(call(&url, "/a+b").0, 500);
    node.shutdown().await.unwrap();
}
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_storage::{Consistency, MetaStore, Mutation, Read};
use serde_json::{Value, json};
use std::time::Duration;
fn wire(url: &str, suffix: &str) -> (u16, String, Value) {
    let output = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "10",
            "--data-binary",
            "unused body",
            "-X",
            "GET",
            "-w",
            "\n%{http_code}\n%{content_type}",
            &format!("{url}/pd/api/v2/affinity-groups{suffix}"),
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    let (text, content) = text.rsplit_once('\n').unwrap();
    let (body, status) = text.rsplit_once('\n').unwrap();
    (
        status.parse().unwrap(),
        content.into(),
        serde_json::from_str(body).unwrap_or_else(|_| json!(body)),
    )
}
fn call(url: &str, suffix: &str) -> (u16, Value) {
    let (code, content, value) = wire(url, suffix);
    assert_eq!(content, "application/json; charset=utf-8");
    (code, value)
}

fn observe(url: &str) -> Value {
    let mut out = Vec::new();
    for (suffix, status) in [
        ("", 200),
        ("?ids=alpha&ids=alpha&ids=missing", 200),
        ("?ids=%20alpha%20&ids=empty&ids=", 200),
        ("?ids=&ids=%20%09", 200),
        ("?ids=missing", 200),
        ("?ids%5B%5D=alpha", 200),
        ("?ids=%ZZ&ids=empty", 200),
        ("?ids=alpha;bad&ids=empty", 200),
        ("?ids=%C2%A0alpha%C2%A0", 200),
        ("?ids=alpha&ids=a%2Fb", 400),
        ("?ids=a+b", 400),
        ("/alpha", 200),
        ("/empty", 200),
        ("/missing", 404),
        ("/a+b", 400),
        ("/%20alpha", 400),
        ("/%2520", 400),
    ] {
        let result = call(url, suffix);
        assert_eq!(result.0, status, "{suffix}: {result:?}");
        if suffix == "" {
            assert_eq!(result.1["affinity_groups"]["alpha"]["range_count"], 2);
            assert_eq!(result.1["affinity_groups"]["empty"]["range_count"], 0);
            assert_eq!(result.1["affinity_groups"]["alpha"]["phase"], "pending");
            assert!(result.1["affinity_groups"]["alpha"].get("ranges").is_none());
        }
        if suffix == "?ids=&ids=%20%09" || suffix == "?ids=missing" {
            assert_eq!(result.1, json!({"affinity_groups":{}}));
        }
        out.push(json!([suffix, result]));
    }
    let too_long = format!("/{}", "x".repeat(65));
    let result = call(url, &too_long);
    assert_eq!(result.0, 400);
    out.push(json!([too_long, result]));
    let slash = wire(url, "/a%2Fb");
    assert_eq!(slash.0, 404);
    assert_eq!(slash.2, json!("404 page not found"));
    out.push(json!(["/a%2Fb", slash]));
    json!(out)
}
// Fixed persisted metadata is also replayed into the real Go manager. No HTTP
// write implementation is introduced ahead of task 072.
fn seed(id: u64) -> Vec<Mutation> {
    let prefix = format!("/pd/{id}");
    let mut writes = vec![];
    for (key, value) in [
        (
            "affinity_groups/alpha",
            json!({"id":"alpha","create_timestamp":123}),
        ),
        (
            "affinity_groups/empty",
            json!({"id":"empty","create_timestamp":124}),
        ),
        ("affinity_groups/null", Value::Null),
        (
            "region_label/affinity_group/alpha",
            json!({"id":"affinity_group/alpha","index":0,"labels":[{"key":"affinity_group","value":"alpha"}],"rule_type":"key-range","data":[{"start_key":"00FF","end_key":"6100"},{"start_key":"6100","end_key":"FF00"}]}),
        ),
    ] {
        writes.push(Mutation::put(
            format!("{prefix}/{key}").as_bytes(),
            &serde_json::to_vec(&value).unwrap(),
        ));
    }
    writes
}
async fn persisted(store: &dyn MetaStore, id: u64) -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut out = Vec::new();
    for prefix in [
        format!("/pd/{id}/affinity_groups/"),
        format!("/pd/{id}/region_label/affinity_group/"),
    ] {
        out.extend(
            store
                .read(Read::prefix(prefix.as_bytes(), Consistency::Linearizable))
                .await
                .unwrap()
                .entries
                .into_iter()
                .map(|e| (e.key, e.value)),
        );
    }
    out
}
async fn oracle() -> Value {
    let binary = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    let mut go = astersql_pd_compat_tests::harness::Server::start(&binary).unwrap();
    go.ready().unwrap();
    let mut client = PdClient::connect(go.url.clone()).await.unwrap();
    let id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    assert_eq!(call(&go.url, "").0, 500);
    crate::grpc_region_lookup_scenarios_test::prepare(&mut client, id).await;
    assert_eq!(call(&go.url, "").1, json!({"affinity_groups":{}}));
    let store =
        astersql_pd_storage_kv::EtcdMetaStore::connect(&[go.url.clone()], Duration::from_secs(5))
            .await
            .unwrap();
    store.transact(vec![], seed(id)).await.unwrap();
    go.restart_with_args(&binary, &[]).unwrap();
    go.ready().unwrap();
    let before = persisted(&store, id).await;
    let result = observe(&go.url);
    assert_eq!(persisted(&store, id).await, before);
    go.restart_with_args(&binary, &[]).unwrap();
    go.ready().unwrap();
    assert_eq!(observe(&go.url), result);
    // Restore assigned peers into the real Go manager, then observe its own
    // ten-second availability loop. This is a bounded live oracle observation,
    // independent from the controlled Tokio clock used by the Rust test.
    store.transact(vec![],vec![Mutation::put(format!("/pd/{id}/affinity_groups/alpha").as_bytes(),br#"{"id":"alpha","create_timestamp":123,"leader_store_id":999,"voter_store_ids":[999]}"#)]).await.unwrap();
    go.restart_with_args(&binary, &[]).unwrap();
    go.ready().unwrap();
    assert_eq!(call(&go.url, "/alpha").1["phase"], "pending");
    let assigned = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let value = call(&go.url, "/alpha").1;
            if value["phase"] == "preparing" {
                break value;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(assigned, assigned_state());
    println!("task071 Go availability: {assigned}");
    go.stop().unwrap();
    result
}
async fn run(differential: bool) {
    let go = if differential {
        Some(oracle().await)
    } else {
        None
    };
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let keep = listener.try_clone().unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = NodeConfig {
        cluster_id: 771,
        member_id: 1,
        name: "affinity".into(),
        client_url: url.clone(),
        peer_url: "http://peer".into(),
    };
    let start = |listener, config| {
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new()).start(
            store.clone(),
            config,
            listener,
            Arc::new(prometheus::Registry::new()),
        )
    };
    let mut node = start(
        tokio::net::TcpListener::from_std(listener).unwrap(),
        config.clone(),
    )
    .await
    .unwrap();
    node.ready().await.unwrap();
    let mut client = PdClient::connect(url.clone()).await.unwrap();
    crate::grpc_region_lookup_scenarios_test::prepare(&mut client, 771).await;
    assert_eq!(call(&url, "").1, json!({"affinity_groups":{}}));
    node.shutdown().await.unwrap();
    store.transact(vec![], seed(771)).await.unwrap();
    let before = persisted(store.as_ref(), 771).await;
    let keep2 = keep.try_clone().unwrap();
    let mut node = start(
        tokio::net::TcpListener::from_std(keep).unwrap(),
        config.clone(),
    )
    .await
    .unwrap();
    node.ready().await.unwrap();
    let result = observe(&url);
    assert_eq!(persisted(store.as_ref(), 771).await, before);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", listener.local_addr().unwrap());
    let mut follower = start(
        listener,
        NodeConfig {
            member_id: 2,
            name: "follower".into(),
            client_url: follower_url.clone(),
            ..config.clone()
        },
    )
    .await
    .unwrap();
    follower.ready().await.unwrap();
    assert_eq!(observe(&follower_url), result);
    follower.shutdown().await.unwrap();
    node.shutdown().await.unwrap();
    let mut node = start(tokio::net::TcpListener::from_std(keep2).unwrap(), config)
        .await
        .unwrap();
    node.ready().await.unwrap();
    assert_eq!(observe(&url), result);
    assert_eq!(persisted(store.as_ref(), 771).await, before);
    node.shutdown().await.unwrap();
    if let Some(go) = go {
        println!("Go: {go}\nRust: {result}");
        assert_eq!(go, result);
        println!(
            "task071 differential: {} observations and restart snapshots",
            result.as_array().unwrap().len()
        );
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_071_reads_and_restart() {
    run(false).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_071_differential() {
    run(true).await;
}

use astersql_pd_storage::{
    Commit, Compare, Lease, LeaseId, MetaWatch, RawTxnOp, RawTxnResponse, Revision, Snapshot,
};

// Gate the existing bootstrap read; errors still come from real etcd.
pub(crate) struct ReadStore {
    pub(crate) inner: Arc<dyn MetaStore>,
    pub(crate) mode: std::sync::atomic::AtomicU8,
    pub(crate) entered: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl MetaStore for ReadStore {
    async fn consensus_cluster_id(&self) -> astersql_pd_storage::Result<Option<u64>> {
        self.inner.consensus_cluster_id().await
    }
    async fn consensus_leader_id(&self) -> astersql_pd_storage::Result<Option<u64>> {
        self.inner.consensus_leader_id().await
    }
    async fn read(&self, mut request: Read) -> astersql_pd_storage::Result<Snapshot> {
        if request.start.as_deref() == Some(b"/pd/7714/raft") {
            match self.mode.swap(0, std::sync::atomic::Ordering::SeqCst) {
                // The real etcd read rejects a revision that has not happened.
                1 => request.revision = Some(i64::MAX),
                2 => {
                    self.entered.notify_one();
                    std::future::pending::<()>().await;
                }
                _ => {}
            }
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
async fn task_071_storage_failure_and_cancel() {
    use std::sync::atomic::{AtomicU8, Ordering};
    let fixture = crate::etcd_fixture::Fixture::start();
    let inner = Arc::new(fixture.store().await);
    inner.transact(vec![], seed(7714)).await.unwrap();
    let store = Arc::new(ReadStore {
        inner: inner.clone(),
        mode: AtomicU8::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let cancel = CancellationToken::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 7714,
                member_id: 1,
                name: "affinity-fault".into(),
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
    crate::grpc_region_lookup_scenarios_test::prepare(&mut client, 7714).await;
    let before = call(&url, "");
    assert_eq!(before.0, 200);
    let saved = persisted(inner.as_ref(), 7714).await;
    store.mode.store(1, Ordering::SeqCst);
    assert_eq!(call(&url, "").0, 500);
    assert_eq!(call(&url, ""), before);
    assert_eq!(persisted(inner.as_ref(), 7714).await, saved);
    store.mode.store(2, Ordering::SeqCst);
    let target = url.clone();
    let request = tokio::task::spawn_blocking(move || call(&target, "/alpha"));
    tokio::time::timeout(Duration::from_secs(10), store.entered.notified())
        .await
        .unwrap();
    cancel.cancel();
    assert_eq!(request.await.unwrap().0, 500);
    tokio::time::timeout(Duration::from_secs(10), node.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted(inner.as_ref(), 7714).await, saved);
}

#[tokio::test]
async fn task_071_availability_tick() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let mut writes = seed(7715);
    writes[0] = Mutation::put(
        b"/pd/7715/affinity_groups/alpha",
        br#"{"id":"alpha","create_timestamp":123,"leader_store_id":999,"voter_store_ids":[999]}"#,
    );
    store.transact(vec![], writes).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            NodeConfig {
                cluster_id: 7715,
                member_id: 1,
                name: "availability".into(),
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
    crate::grpc_region_lookup_scenarios_test::prepare(&mut client, 7715).await;
    let target = url.clone();
    let before = tokio::task::spawn_blocking(move || call(&target, "/alpha"))
        .await
        .unwrap();
    assert_eq!(before.1["phase"], "pending");
    // Advance in small steps, letting the real etcd lease heartbeat finish
    // between steps instead of jumping past the host's leadership deadline.
    let mut after = before.clone();
    for _ in 0..120 {
        tokio::time::pause();
        tokio::time::advance(Duration::from_millis(100)).await;
        tokio::time::resume();
        let target = url.clone();
        after = tokio::task::spawn_blocking(move || call(&target, "/alpha"))
            .await
            .unwrap();
        assert_eq!(after.0, 200, "{after:?}");
        if after.1["phase"] == "preparing" {
            break;
        }
    }
    node.shutdown().await.unwrap();
    assert_eq!(
        after.1,
        assigned_state(),
        "host must drive the canonical manager's availability policy"
    );
    println!("task071 Rust availability: {}", after.1);
}

fn assigned_state() -> Value {
    json!({"id":"alpha","create_timestamp":123,"leader_store_id":999,"voter_store_ids":[999],"phase":"preparing","range_count":2,"region_count":0,"affinity_region_count":0})
}
