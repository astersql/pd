// Copyright 2026 AsterSQL.
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_070_behavior() {
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
        crate::http_member_test::request(
            &url,
            "PATCH",
            "/pd/api/v1/config/region-label/rules",
            "{}"
        ),
        (
            500,
            serde_json::json!(
                "[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first"
            )
        )
    );
    node.shutdown().await.unwrap();
}

use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use serde_json::{Value, json};
fn call(url: &str, method: &str, path: &str, body: &str) -> (u16, String, Value) {
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
            &format!("{url}/pd/api/v1/{path}"),
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    let (text, content) = text.rsplit_once('\n').unwrap();
    let (body, code) = text.rsplit_once('\n').unwrap();
    let mut value: Value = serde_json::from_str(body).unwrap_or_else(|_| json!(body));
    // Go map iteration is unordered for all-rules and all-labels responses.
    if let Some(a) = value.as_array_mut() {
        if path.ends_with("/rules") {
            a.sort_by_key(|v| v["id"].as_str().unwrap_or("").to_owned());
        }
        if path.ends_with("/labels") {
            a.sort_by_key(|v| v["key"].as_str().unwrap_or("").to_owned());
        }
    }
    (code.parse().unwrap(), content.into(), value)
}
fn rule(id: &str, value: &str) -> Value {
    json!({"id":id,"index":if id == "new" {3} else {1},"labels":[{"key":"zone","value":value}],"rule_type":"key-range","data":[{"start_key":"61","end_key":"6D"}]})
}
async fn observe(url: &str, id: u64) -> Value {
    let mut out = vec![];
    for (method, path, body) in [
        ("GET", "config/region-label/rules", ""),
        ("POST", "config/region-label/rule", "{"),
        ("PATCH", "config/region-label/rules", "{}"),
        ("GET", "region/id/10/labels", ""),
    ] {
        let r = call(url, method, path, body);
        assert_eq!(r.0, 500);
        out.push(json!(["cold", path, r]));
    }
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    super::grpc_region_lookup_scenarios_test::prepare(&mut client, id).await;
    // Establish the same explicit rules baseline; default keyspace creation is
    // owned by keyspace bootstrap, outside this region-label API contract.
    let reset = call(
        url,
        "PATCH",
        "config/region-label/rules",
        r#"{"deletes":["keyspaces/0"]}"#,
    );
    assert_eq!(reset.0, 200);
    out.push(json!(["reset-default-keyspace", reset]));
    let good = rule("a/b+c%20", "west").to_string();
    let mut high = rule("high", "east");
    high["index"] = json!(8);
    high["labels"].as_array_mut().unwrap().push(
        json!({"key":"tier","value":"hot","ttl":"1h","start_at":"Wed Jan  1 00:00:00 UTC 2098"}),
    );
    let mut crossing = rule("crossing", "no");
    crossing["data"] = json!([{"start_key":"62","end_key":"6D"}]);
    let mut ttl = rule("expired", "old");
    ttl["labels"][0]["ttl"] = json!("1s");
    ttl["labels"][0]["start_at"] = json!("Mon Jan  1 00:00:00 UTC 2001");
    let mut cases=vec![
        ("GET","config/region-label/rules", "".into(),200),
        ("GET","config/region-label/rule/missing","".into(),404),
        ("DELETE","config/region-label/rule/missing","".into(),404),
        ("POST","config/region-label/rule",good,200),
        ("GET","config/region-label/rule/a%2Fb+c%2520","".into(),200),
        ("GET","config/region-label/rules/ids",r#"["missing","a/b+c%20","a/b+c%20"]"#.into(),200),
        ("GET","region/id/10/labels","".into(),200),
        ("GET","region/id/10/label/zone","".into(),200),
        ("GET","region/id/11/labels","".into(),200),
        ("GET","region/id/999/labels","".into(),404),
        ("GET","region/id/no/labels","".into(),400),
        ("POST","config/region-label/rule",crossing.to_string(),200),
        ("GET","region/id/10/labels","".into(),200),
        ("POST","config/region-label/rule",high.to_string(),200),
        ("DELETE","config/region-label/rule/crossing","".into(),200),
        ("GET","region/id/10/labels","".into(),200),
        ("GET","region/id/10/label/zone","".into(),200),
        ("POST","config/region-label/rule",crossing.to_string(),200),
        ("POST","config/region-label/rule",ttl.to_string(),400),
        ("POST","config/region-label/rule","{}".into(),400),
        ("POST","config/region-label/rule","null".into(),400),
        ("POST","config/region-label/rule","{".into(),400),
        ("POST","config/region-label/rule",r#"{"id":3}"#.into(),400),
        ("POST","config/region-label/rule",r#"{"labels":[{"key":false}]}"#.into(),400),
        ("GET","config/region-label/rules/ids","[1]".into(),400),
        ("GET","config/region-label/rules/ids","[null]".into(),200),
        ("GET","config/region-label/rules/ids","null".into(),200),
        ("PATCH","config/region-label/rules",json!({"sets":[rule("new","new"),{"id":"bad"}],"deletes":["high"]}).to_string(),400),
        ("GET","config/region-label/rule/new","".into(),404),
        ("GET","region/id/10/label/zone","".into(),200),
        ("PATCH","config/region-label/rules",json!({"sets":[rule("new","first"),rule("new","last")],"deletes":["high","new","absent"]}).to_string(),200),
        ("GET","config/region-label/rule/new","".into(),200),
        ("GET","region/id/10/labels","".into(),200),
        ("DELETE","config/region-label/rule/a%2Fb+c%2520","".into(),200),
        ("DELETE","config/region-label/rule/crossing","".into(),200),
        ("GET","region/id/10/label/zone","".into(),200),
        ("PATCH","config/region-label/rules","null".into(),200),
        ("PATCH","config/region-label/rules",r#"{"sets":[{"ID":"CASE","id":null,"labels":[{"KEY":"k","value":"v"}],"RULE_TYPE":"key-range","data":[{"start_key":"","end_key":""}]}]}"#.into(),200),
    ];
    for data in [
        json!([]),
        json!([{"start_key":"zz","end_key":""}]),
        json!([{"start_key":"f","end_key":""}]),
        json!([{"start_key":"62","end_key":"61"}]),
        json!([{"start_key":"","end_key":"" ,"extra":1}]),
    ] {
        let mut body = rule("ranges", "v");
        body["data"] = data;
        let status = if body["data"][0]["extra"] == 1 {
            200
        } else {
            400
        };
        cases.push(("POST", "config/region-label/rule", body.to_string(), status));
    }
    cases.push(("DELETE", "config/region-label/rule/ranges", "".into(), 200));
    for body in [
        r#"{"sets":[{"labels":[{"value":3}]}]}"#,
        r#"{"deletes":[1]}"#,
        r#"{"sets":{}}"#,
        "[]",
        r#"{"index":1.1}"#,
    ] {
        cases.push((
            "PATCH",
            "config/region-label/rules",
            body.into(),
            if body.contains("index") { 200 } else { 400 },
        ));
    }
    for (method, path, body, status) in cases {
        let result = call(url, method, path, &body);
        println!("{method} {path} {body}: {result:?}");
        assert_eq!(result.0, status, "{method} {path}: {result:?}");
        out.push(json!([method, path, result]));
        out.push(json!([
            "state",
            call(url, "GET", "config/region-label/rules", "")
        ]));
    }
    json!(out)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_070_rules_and_restart() {
    run_rust(false).await;
}
async fn run_rust(differential: bool) {
    let oracle = if differential {
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
        let result = observe(&go.url, id).await;
        let before = call(&go.url, "GET", "config/region-label/rules", "");
        go.restart_with_args(&binary, &[]).unwrap();
        go.ready().unwrap();
        let after = call(&go.url, "GET", "config/region-label/rules", "");
        go.stop().unwrap();
        Some((result, before, after))
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
        cluster_id: 770,
        member_id: 1,
        name: "labels".into(),
        client_url: url.clone(),
        peer_url: "http://peer".into(),
    };
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            config.clone(),
            tokio::net::TcpListener::from_std(listener).unwrap(),
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let result = observe(&url, 770).await;
    let before = call(&url, "GET", "config/region-label/rules", "");
    let endpoint = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
    let mut saved = vec![];
    endpoint
        .load_region_rules(&astersql_pd_keypath::ClusterPaths::new(770), |_, v| {
            saved.push(serde_json::from_slice::<Value>(v).unwrap())
        })
        .await
        .unwrap();
    assert_eq!(saved.len(), 2);
    assert!(saved.iter().any(|r| r["labels"][0]["value"] == "last"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", listener.local_addr().unwrap());
    let mut follower =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store.clone(),
                NodeConfig {
                    member_id: 2,
                    name: "follower".into(),
                    client_url: follower_url.clone(),
                    ..config.clone()
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    follower.ready().await.unwrap();
    assert_eq!(
        call(&follower_url, "GET", "config/region-label/rules", ""),
        before
    );
    assert_eq!(
        call(
            &follower_url,
            "POST",
            "config/region-label/rule",
            &rule("new", "last").to_string()
        )
        .0,
        200
    );
    assert_eq!(
        call(&url, "GET", "region/id/10/label/zone", "").2,
        json!("last")
    );
    follower.shutdown().await.unwrap();
    node.shutdown().await.unwrap();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            config,
            tokio::net::TcpListener::from_std(keep).unwrap(),
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let after = call(&url, "GET", "config/region-label/rules", "");
    node.shutdown().await.unwrap();
    assert_eq!(before, after);
    if let Some((go, go_before, go_after)) = oracle {
        println!("Go: {go}\nRust: {result}");
        assert_eq!(go, result);
        println!(
            "task070 differential observations: {} plus 2 restart snapshots",
            result.as_array().unwrap().len()
        );
        assert_eq!(go_before, before);
        assert_eq!(go_after, after);
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_070_differential() {
    run_rust(true).await;
}

// Copyright 2026 AsterSQL.
use astersql_pd_storage::{
    Commit, Compare, Lease, LeaseId, MetaStore, MetaWatch, Mutation, RawTxnOp, RawTxnResponse,
    Read, Revision, Snapshot,
};

// Only gates real backend IO: failures still come from the real etcd transaction.
pub(crate) struct LabelStore {
    pub(crate) inner: Arc<dyn MetaStore>,
    pub(crate) mode: std::sync::atomic::AtomicU8,
    pub(crate) entered: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl MetaStore for LabelStore {
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
                if key.windows(14).any(|w| w == b"/region_label/") {
                    match self.mode.swap(0, std::sync::atomic::Ordering::SeqCst) {
                        1 => *lease = Some(i64::MAX),
                        2 => {
                            self.entered.notify_one();
                            std::future::pending::<()>().await;
                        }
                        _ => {}
                    }
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
async fn task_070_storage_failure_and_cancel() {
    use std::sync::atomic::{AtomicU8, Ordering};
    let fixture = crate::etcd_fixture::Fixture::start();
    let inner = Arc::new(fixture.store().await);
    let store = Arc::new(LabelStore {
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
                cluster_id: 7704,
                member_id: 1,
                name: "labels-fault".into(),
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
    super::grpc_region_lookup_scenarios_test::prepare(&mut client, 7704).await;
    let endpoint = astersql_pd_storage_endpoint::StorageEndpoint::new(inner);
    let paths = astersql_pd_keypath::ClusterPaths::new(7704);
    let body = json!({"sets":[rule("one","first"),rule("two","second")]}).to_string();
    let snapshot = || call(&url, "GET", "config/region-label/rules", "");
    let before = snapshot();
    store.mode.store(1, Ordering::SeqCst);
    assert_eq!(
        call(&url, "PATCH", "config/region-label/rules", &body).0,
        500
    );
    assert_eq!(snapshot(), before);
    let mut saved = vec![];
    endpoint
        .load_region_rules(&paths, |_, v| saved.push(v.to_vec()))
        .await
        .unwrap();
    assert!(saved.is_empty());
    assert_eq!(
        call(&url, "PATCH", "config/region-label/rules", &body).0,
        200
    );
    let before = snapshot();
    let mut saved = vec![];
    endpoint
        .load_region_rules(&paths, |_, v| saved.push(v.to_vec()))
        .await
        .unwrap();
    assert_eq!(saved.len(), 2);
    store.mode.store(1, Ordering::SeqCst);
    assert_eq!(
        call(
            &url,
            "POST",
            "config/region-label/rule",
            &rule("one", "changed").to_string()
        )
        .0,
        500
    );
    assert_eq!(snapshot(), before);
    store.mode.store(2, Ordering::SeqCst);
    let target = url.clone();
    let request = tokio::task::spawn_blocking(move || {
        call(
            &target,
            "PATCH",
            "config/region-label/rules",
            &json!({"sets":[rule("one","changed"),rule("two","changed")]}).to_string(),
        )
    });
    tokio::time::timeout(std::time::Duration::from_secs(10), store.entered.notified())
        .await
        .unwrap();
    cancel.cancel();
    assert_eq!(request.await.unwrap().0, 500);
    node.shutdown().await.unwrap();
    let mut after = vec![];
    endpoint
        .load_region_rules(&paths, |_, v| after.push(v.to_vec()))
        .await
        .unwrap();
    assert_eq!(saved, after);
}
#[tokio::test]
async fn task_070_ttl_exact_boundary() {
    use astersql_pd_schedule_labeler::{LabelRule, RegionLabeler};
    let fixture = crate::etcd_fixture::Fixture::start();
    let endpoint =
        astersql_pd_storage_endpoint::StorageEndpoint::new(Arc::new(fixture.store().await));
    let paths = astersql_pd_keypath::ClusterPaths::new(7705);
    // Explicit host time, no wall-clock sleeps or startup-time fault replay.
    let mut labels = RegionLabeler::load(endpoint.clone(), paths, 0)
        .await
        .unwrap();
    let mut body = rule("ttl", "live");
    body["labels"][0]["ttl"] = json!("1s");
    labels
        .set_label_rule(
            LabelRule::from_json_without_check(body.to_string().as_bytes()).unwrap(),
            0,
        )
        .await
        .unwrap();
    assert_eq!(
        labels.get_label_rule("ttl", 1_000_000_000).unwrap().labels[0].start_at,
        "Thu Jan  1 00:00:00 UTC 1970"
    );
    assert!(labels.get_label_rule("ttl", 1_000_000_001).is_none());
    // Filtering must not silently delete persistence or the authoritative rule.
    assert!(labels.index().rule("ttl").is_some());
    let mut persisted = vec![];
    endpoint
        .load_region_rules(&paths, |_, v| {
            persisted.push(serde_json::from_slice::<Value>(v).unwrap())
        })
        .await
        .unwrap();
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0]["labels"][0]["ttl"], "1s");
}
