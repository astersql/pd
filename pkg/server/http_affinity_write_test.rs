// Copyright 2026 AsterSQL.
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_072_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            Arc::new(fixture.store().await),
            NodeConfig {
                cluster_id: 772,
                member_id: 1,
                name: "write".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let output = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "10",
            "-X",
            "POST",
            "-d",
            "{}",
            "-w",
            "\n%{http_code}",
            &format!("{url}/pd/api/v2/affinity-groups"),
        ])
        .output()
        .unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    node.shutdown().await.unwrap();
    assert!(
        text.ends_with("\n500"),
        "bootstrap must precede body validation: {text}"
    );
}
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_storage::{Consistency, MetaStore, Read};
use serde_json::{Value, json};
use std::time::Duration;
fn wire(url: &str, method: &str, suffix: &str, body: &str) -> (u16, Value) {
    let output = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "10",
            "-X",
            method,
            "--data-binary",
            body,
            "-w",
            "\n%{http_code}\n%{content_type}",
            &format!("{url}/pd/api/v2/affinity-groups{suffix}"),
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    let (text, content) = text.rsplit_once('\n').unwrap();
    let (text, status) = text.rsplit_once('\n').unwrap();
    let value: Value = serde_json::from_str(text).unwrap();
    let expected = if method != "DELETE"
        && value
            .as_str()
            .is_some_and(|s| s.starts_with("[PD:gin:ErrBindJSON]"))
    {
        "text/plain; charset=utf-8"
    } else {
        "application/json; charset=utf-8"
    };
    assert_eq!(content, expected, "{text}");
    (status.parse().unwrap(), value)
}
async fn saved(store: &dyn MetaStore, id: u64) -> Value {
    let mut out = serde_json::Map::new();
    for suffix in ["affinity_groups/", "region_label/affinity_group/"] {
        for entry in store
            .read(Read::prefix(
                format!("/pd/{id}/{suffix}").as_bytes(),
                Consistency::Linearizable,
            ))
            .await
            .unwrap()
            .entries
        {
            out.insert(
                String::from_utf8(entry.key)
                    .unwrap()
                    .trim_start_matches(&format!("/pd/{id}/"))
                    .into(),
                serde_json::from_slice::<Value>(&entry.value).unwrap(),
            );
        }
    }
    json!(out)
}
fn normalize(mut v: Value) -> Value {
    fn clean(v: &mut Value) {
        match v {
            Value::Object(m) => {
                if let Some(t) = m.get_mut("create_timestamp") {
                    assert!(t.as_u64().is_some_and(|n| n > 0) || t == "unix seconds");
                    *t = json!("unix seconds");
                }
                for v in m.values_mut() {
                    clean(v)
                }
            }
            Value::Array(a) => {
                for v in a {
                    clean(v)
                }
            }
            _ => {}
        }
    }
    clean(&mut v);
    v
}
async fn observe(url: &str, store: &dyn MetaStore, id: u64) -> Value {
    let mut out = vec![];
    for (method, suffix, body, status) in [
        ("POST", "", "{}", 400),
        (
            "POST",
            "",
            r#"{"affinity_groups":{"a":{"ranges":[{"start_key":"YQ==","end_key":"bQ=="}]},"b":{"ranges":[{"start_key":"bg==","end_key":"eg=="}]}}}"#,
            200,
        ),
        (
            "POST",
            "",
            r#"{"affinity_groups":{"a":{"ranges":[]}}}"#,
            409,
        ),
        (
            "POST",
            "?skip_exist_check=true",
            r#"{"affinity_groups":{"a":{"ranges":[]}}}"#,
            200,
        ),
        (
            "POST",
            "",
            r#"{"affinity_groups":{"c":{"ranges":[{"start_key":"eg==","end_key":"YQ=="}]},"d":{"ranges":[{"start_key":"eg==","end_key":"eno="}]}}}"#,
            400,
        ),
        (
            "POST",
            "",
            r#"{"affinity_groups":{"bad/id":{"ranges":[{}]},"d":{"ranges":[{"start_key":"eg==","end_key":"eno="}]}}}"#,
            400,
        ),
        (
            "POST",
            "",
            r#"{"affinity_groups":{"c":{"ranges":[{"start_key":"","end_key":"YQ=="}]}}}"#,
            400,
        ),
        (
            "POST",
            "",
            r#"{"affinity_groups":{"c":{"ranges":[{"start_key":"Yg==","end_key":"Yw=="}]}}}"#,
            400,
        ),
        ("PATCH", "", "{}", 400),
        (
            "PATCH",
            "",
            r#"{"add":[{"id":"a","ranges":[{"start_key":"eg==","end_key":"eno="}]}],"remove":[{"id":"a","ranges":[{}]}]}"#,
            400,
        ),
        (
            "PATCH",
            "",
            r#"{"add":[{"id":"a","ranges":[{"start_key":"eg==","end_key":"eno="}]},{"id":"missing","ranges":[{}]}]}"#,
            404,
        ),
        (
            "PATCH",
            "",
            r#"{"remove":[{"id":"a","ranges":[{"start_key":"Yg==","end_key":"ZA=="}]}]}"#,
            404,
        ),
        (
            "PATCH",
            "",
            r#"{"remove":[{"id":"a","ranges":[{"start_key":"YQ==","end_key":"bQ=="}]}]}"#,
            200,
        ),
        (
            "PATCH",
            "",
            r#"{"add":[{"id":"a","ranges":[{"start_key":"Yg==","end_key":"Yw=="}]},{"id":"a","ranges":[{"start_key":"Yw==","end_key":"ZA=="}]}]}"#,
            200,
        ),
        ("PATCH", "", r#"{"add":[{"id":"a","ranges":[]}]}"#, 400),
        ("PUT", "/a", "{}", 400),
        (
            "PUT",
            "/a",
            r#"{"leader_store_id":1,"voter_store_ids":[1,1]}"#,
            400,
        ),
        (
            "PUT",
            "/a",
            r#"{"leader_store_id":2,"voter_store_ids":[1]}"#,
            400,
        ),
        (
            "PUT",
            "/a",
            r#"{"leader_store_id":99,"voter_store_ids":[99]}"#,
            400,
        ),
        (
            "PUT",
            "/a",
            r#"{"leader_store_id":1,"voter_store_ids":[1]}"#,
            200,
        ),
        (
            "PUT",
            "/missing",
            r#"{"leader_store_id":1,"voter_store_ids":[1]}"#,
            404,
        ),
        ("DELETE", "/a", "", 400),
        (
            "POST",
            "?delete=1",
            r#"{"ids":["a","missing"],"force":false}"#,
            400,
        ),
        (
            "POST",
            "?delete=1",
            r#"{"ids":["a","bad/id"],"force":true}"#,
            400,
        ),
        (
            "POST",
            "?delete=1",
            r#"{"ids":["b","b"],"force":true}"#,
            200,
        ),
        ("DELETE", "/a?force=true", "", 200),
        ("DELETE", "/missing?force=true", "ignored", 200),
        ("DELETE", "/missing", "", 404),
        ("DELETE", "/missing?force=bad", "", 400),
        (
            "POST",
            "?delete",
            r#"{"ids":["missing"],"force":true}"#,
            200,
        ),
        ("POST", "?delete=1", "{}", 400),
        ("POST", "", "null", 400),
        (
            "POST",
            "?skip_exist_check=",
            r#"{"affinity_groups":{"x":{"ranges":[{}]}}}"#,
            400,
        ),
        (
            "POST",
            "",
            r#"{"affinity_groups":{"x":{"ranges":[{"start_key":"!"}]}}}"#,
            400,
        ),
        ("PUT", "/x", r#"{"leader_store_id":"x"}"#, 400),
        ("POST", "", "{", 400),
        ("PATCH", "", r#"{"add":false}"#, 400),
        ("PATCH", "", "null", 400),
        ("POST", "", "", 400),
        (
            "POST",
            "",
            r#"{"affinity_groups":{"whole":{"ranges":[{}]}}}"#,
            200,
        ),
        ("DELETE", "/whole?force=true", "", 200),
    ] {
        let before = saved(store, id).await;
        let state = wire(url, "GET", "", "");
        let result = wire(url, method, suffix, body);
        println!(
            "{method} {suffix}: {result:?}; durable {}",
            saved(store, id).await
        );
        assert_eq!(result.0, status, "{method} {suffix} {body}: {result:?}");
        if status >= 400 {
            assert_eq!(
                saved(store, id).await,
                before,
                "persistent partial mutation: {body}"
            );
            assert_eq!(
                wire(url, "GET", "", ""),
                state,
                "memory partial mutation: {body}"
            );
        }
        out.push(json!([
            method,
            suffix,
            result,
            normalize(saved(store, id).await)
        ]));
    }
    json!(out)
}
async fn run(differential: bool) {
    let go = if differential {
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
        crate::grpc_region_lookup_scenarios_test::prepare(&mut client, id).await;
        let store = astersql_pd_storage_kv::EtcdMetaStore::connect(
            &[go.url.clone()],
            Duration::from_secs(5),
        )
        .await
        .unwrap();
        let mut result = observe(&go.url, &store, id).await;
        roundtrip_seed(&go.url);
        let before = saved(&store, id).await;
        go.restart_with_args(&binary, &[]).unwrap();
        go.ready().unwrap();
        assert_eq!(saved(&store, id).await, before);
        result
            .as_array_mut()
            .unwrap()
            .push(roundtrip_restored(&go.url, &store, id).await);
        go.stop().unwrap();
        Some(result)
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
        cluster_id: 772,
        member_id: 1,
        name: "write".into(),
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
    crate::grpc_region_lookup_scenarios_test::prepare(&mut client, 772).await;
    let mut result = observe(&url, store.as_ref(), 772).await;
    roundtrip_seed(&url);
    let before = saved(store.as_ref(), 772).await;
    node.shutdown().await.unwrap();
    let mut node = start(tokio::net::TcpListener::from_std(keep).unwrap(), config)
        .await
        .unwrap();
    node.ready().await.unwrap();
    assert_eq!(saved(store.as_ref(), 772).await, before);
    result
        .as_array_mut()
        .unwrap()
        .push(roundtrip_restored(&url, store.as_ref(), 772).await);
    node.shutdown().await.unwrap();
    if let Some(go) = go {
        for (index, (actual, expected)) in normalize(result.clone())
            .as_array()
            .unwrap()
            .iter()
            .zip(normalize(go.clone()).as_array().unwrap())
            .enumerate()
        {
            assert_eq!(actual, expected, "observation {index}");
        }
        assert_eq!(
            result.as_array().unwrap().len(),
            go.as_array().unwrap().len()
        );
        println!(
            "task072 differential {} observations plus durable snapshots",
            result.as_array().unwrap().len()
        );
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_072_writes_and_atomic_validation() {
    run(false).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_072_differential() {
    run(true).await;
}

use astersql_pd_storage::{
    Commit, Compare, Lease, LeaseId, MetaWatch, Mutation, RawTxnOp, RawTxnResponse, Revision,
    Snapshot,
};
struct WriteStore {
    pub(crate) inner: Arc<dyn MetaStore>,
    pub(crate) mode: std::sync::atomic::AtomicU8,
    pub(crate) entered: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl MetaStore for WriteStore {
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
        mut compares: Vec<Compare>,
        writes: Vec<Mutation>,
    ) -> astersql_pd_storage::Result<Commit> {
        if writes.iter().any(|w| match w {
            Mutation::Put { key, .. } | Mutation::Delete { key } => {
                key.starts_with(b"/pd/7728/affinity_groups/")
                    || key.starts_with(b"/pd/7728/region_label/affinity_group/")
            }
        }) {
            match self.mode.swap(0, std::sync::atomic::Ordering::SeqCst) {
                1 => compares.push(Compare::CreateRevision(b"/task072/missing".to_vec(), 1)),
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
async fn task_072_transaction_failure_and_cancel() {
    use std::sync::atomic::{AtomicU8, Ordering};
    let fixture = crate::etcd_fixture::Fixture::start();
    let inner = Arc::new(fixture.store().await);
    let store = Arc::new(WriteStore {
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
                cluster_id: 7728,
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
    crate::grpc_region_lookup_scenarios_test::prepare(&mut client, 7728).await;
    let create = r#"{"affinity_groups":{"a":{"ranges":[{"start_key":"YQ==","end_key":"Yg=="}]}}}"#;
    let before = saved(inner.as_ref(), 7728).await;
    store.mode.store(1, Ordering::SeqCst);
    assert_eq!(wire(&url, "POST", "", create).0, 500);
    assert_eq!(saved(inner.as_ref(), 7728).await, before);
    assert_eq!(wire(&url, "GET", "", "").1, json!({"affinity_groups":{}}));
    assert_eq!(wire(&url, "POST", "", create).0, 200);
    let state = wire(&url, "GET", "/a", "");
    let before = saved(inner.as_ref(), 7728).await;
    for (method, suffix, body) in [
        (
            "PATCH",
            "",
            r#"{"remove":[{"id":"a","ranges":[{"start_key":"YQ==","end_key":"Yg=="}]}]}"#,
        ),
        (
            "PUT",
            "/a",
            r#"{"leader_store_id":1,"voter_store_ids":[1]}"#,
        ),
        ("DELETE", "/a?force=true", ""),
    ] {
        store.mode.store(1, Ordering::SeqCst);
        assert_eq!(wire(&url, method, suffix, body).0, 500);
        assert_eq!(wire(&url, "GET", "/a", ""), state);
        assert_eq!(saved(inner.as_ref(), 7728).await, before);
    }
    store.mode.store(2, Ordering::SeqCst);
    let target = url.clone();
    let request = tokio::task::spawn_blocking(move || wire(&target, "DELETE", "/a?force=true", ""));
    tokio::time::timeout(Duration::from_secs(10), store.entered.notified())
        .await
        .unwrap();
    cancel.cancel();
    assert_eq!(request.await.unwrap().0, 500);
    tokio::time::timeout(Duration::from_secs(10), node.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved(inner.as_ref(), 7728).await, before);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_072_restart_and_follower() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let keep = listener.try_clone().unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = NodeConfig {
        cluster_id: 7729,
        member_id: 1,
        name: "restart".into(),
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
    crate::grpc_region_lookup_scenarios_test::prepare(&mut client, 7729).await;
    let follower_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", follower_listener.local_addr().unwrap());
    let mut follower = start(
        follower_listener,
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
    assert_eq!(
        wire(
            &follower_url,
            "POST",
            "",
            r#"{"affinity_groups":{"keep":{"ranges":[{"start_key":"AA==","end_key":"/w=="}]}}}"#
        )
        .0,
        200
    );
    assert_eq!(
        wire(
            &follower_url,
            "PUT",
            "/keep",
            r#"{"leader_store_id":1,"voter_store_ids":[1]}"#
        )
        .0,
        200
    );
    let before = saved(store.as_ref(), 7729).await;
    assert_eq!(
        wire(&follower_url, "GET", "/keep", ""),
        wire(&url, "GET", "/keep", "")
    );
    follower.shutdown().await.unwrap();
    node.shutdown().await.unwrap();
    let mut node = start(tokio::net::TcpListener::from_std(keep).unwrap(), config)
        .await
        .unwrap();
    node.ready().await.unwrap();
    assert_eq!(saved(store.as_ref(), 7729).await, before);
    let state = wire(&url, "GET", "/keep", "");
    assert_eq!(state.0, 200);
    assert_eq!(state.1["range_count"], 1);
    assert_eq!(state.1["leader_store_id"], 1);
    assert_eq!(state.1["voter_store_ids"], json!([1]));
    assert_eq!(wire(&url, "DELETE", "/keep?force=true", "").0, 200);
    assert_eq!(saved(store.as_ref(), 7729).await, json!({}));
    node.shutdown().await.unwrap();
}

fn roundtrip_seed(url: &str) {
    assert_eq!(
        wire(
            url,
            "POST",
            "",
            r#"{"affinity_groups":{"keep":{"ranges":[{"start_key":"AA==","end_key":"/w=="}]}}}"#
        )
        .0,
        200
    );
    assert_eq!(
        wire(
            url,
            "PUT",
            "/keep",
            r#"{"leader_store_id":1,"voter_store_ids":[1]}"#
        )
        .0,
        200
    );
}
async fn roundtrip_restored(url: &str, store: &dyn MetaStore, id: u64) -> Value {
    let state = wire(url, "GET", "/keep", "");
    assert_eq!(state.0, 200);
    assert_eq!(state.1["range_count"], 1);
    assert_eq!(state.1["leader_store_id"], 1);
    assert_eq!(state.1["voter_store_ids"], json!([1]));
    let durable = saved(store, id).await;
    assert_eq!(wire(url, "DELETE", "/keep?force=true", "").0, 200);
    assert_eq!(saved(store, id).await, json!({}));
    json!(["restart", state, normalize(durable)])
}
