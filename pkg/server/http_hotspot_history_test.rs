// Copyright 2026 AsterSQL.
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
const PATH: &str = "/pd/api/v1/hotspot/regions/history";
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_061_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            NodeConfig {
                cluster_id: 761,
                member_id: 1,
                name: "history".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let actual = crate::http_member_test::request(&url, "GET", PATH, "{}");
    assert_eq!(input_cases(&url).as_array().unwrap().len(), 39);
    node.shutdown().await.unwrap();
    assert_eq!(
        actual,
        (200, serde_json::json!({"history_hot_region":null}))
    );
}

use crate::http_member_test::request;
use astersql_pd_storage::{
    Consistency, HistoryHotRegion, HistoryQuery, HotHistory, MetaStore, Mutation, Read,
};
use serde_json::{Value, json};
async fn start(
    store: Arc<dyn MetaStore>,
    address: Option<&str>,
) -> (astersql_pd_runtime::PdNode, String, CancellationToken) {
    if address.is_none() {
        astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone()).save_config(&astersql_pd_keypath::ClusterPaths::new(761),&json!({"schedule":{"hot-regions-write-interval":"100ms"},"replication":{"enable-placement-rules":"false"}})).await.unwrap();
    }
    let listener = tokio::net::TcpListener::bind(address.unwrap_or("127.0.0.1:0"))
        .await
        .unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let cancel = CancellationToken::new();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .start(
            store,
            NodeConfig {
                cluster_id: 761,
                member_id: 1,
                name: "history".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    (node, url, cancel)
}
fn row(n: u64) -> HistoryHotRegion {
    HistoryHotRegion {
        update_time: 1000 + (n / 4) as i64,
        region_id: n,
        peer_id: n + 200,
        store_id: n % 3 + 1,
        is_leader: n % 2 == 0,
        is_learner: n % 3 == 0,
        hot_region_type: if n % 2 == 0 { "read" } else { "write" }.into(),
        hot_degree: 5,
        flow_bytes: 1.25,
        flow_cpu: 0.,
        key_rate: 2.,
        query_rate: 3.,
        start_key: "00FF".into(),
        end_key: "ABCD".into(),
        encryption_meta: None,
    }
}
fn select() -> Value {
    json!({"start_time":1000,"end_time":1100,"is_leaders":[false,true],"is_learners":[false,true]})
}
fn query(url: &str, q: &Value) -> (u16, Value) {
    request(url, "GET", PATH, &q.to_string())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_061_persistence_filters_pages_and_errors() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let cancel = CancellationToken::new();
    let history = HotHistory::new(store.clone(), 761, 1);
    let rows = (1..=140).map(row).collect::<Vec<_>>();
    history.append(&rows, &cancel).await.unwrap();
    // Per-member histories and unrelated metadata are isolated.
    HotHistory::new(store.clone(), 761, 2)
        .append(&[row(999)], &cancel)
        .await
        .unwrap();
    let (mut node, url, _) = start(store.clone(), None).await;
    let before = store
        .read(Read::prefix(
            b"/pd/761/rust/hot-history/",
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .entries;
    let all = query(&url, &select());
    assert_eq!(all.0, 200);
    let all = all.1;
    let actual: Vec<HistoryHotRegion> =
        serde_json::from_value(all["history_hot_region"].clone()).unwrap();
    let mut expected = rows.clone();
    expected.sort_by_key(|r| (r.hot_region_type.clone(), r.update_time, r.region_id));
    assert_eq!(actual, expected);
    for key in [
        "region_ids",
        "store_ids",
        "peer_ids",
        "is_leaders",
        "is_learners",
    ] {
        let mut q = select();
        let value = match key {
            "region_ids" => json!(60),
            "store_ids" => json!(2),
            "peer_ids" => json!(260),
            _ => json!(true),
        };
        q[key] = json!([value.clone()]);
        let response = query(&url, &q);
        assert_eq!(response.0, 200);
        let field = match key {
            "region_ids" => "region_id",
            "store_ids" => "store_id",
            "peer_ids" => "peer_id",
            "is_leaders" => "is_leader",
            _ => "is_learner",
        };
        let expected = actual
            .iter()
            .filter(|r| serde_json::to_value(r).unwrap()[field] == value)
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            response.1,
            json!({"history_hot_region":if expected.is_empty(){None}else{Some(expected)}}),
            "{key}"
        );
    }
    for patch in [
        json!({"is_leaders":[]}),
        json!({"is_learners":[]}),
        json!({"start_time":1101}),
        json!({"end_time":999}),
        json!({"region_ids":[99999]}),
        json!({"hot_region_type":["unknown"]}),
    ] {
        let mut q = select();
        for (k, v) in patch.as_object().unwrap() {
            q[k] = v.clone();
        }
        assert_eq!(query(&url, &q), (200, json!({"history_hot_region":null})));
    }
    let mut q = select();
    q["hot_region_type"] = json!(["WRITE", "read", "write"]);
    let ordered = query(&url, &q).1;
    let result: Vec<HistoryHotRegion> =
        serde_json::from_value(ordered["history_hot_region"].clone()).unwrap();
    assert_eq!(result.len(), 210);
    assert_eq!(result[0].hot_region_type, "write");
    assert_eq!(
        before,
        store
            .read(Read::prefix(
                b"/pd/761/rust/hot-history/",
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries
    );
    node.shutdown().await.unwrap();
    let (mut restored, url, _) =
        start(store.clone(), Some(url.strip_prefix("http://").unwrap())).await;
    assert_eq!(query(&url, &select()), (200, all));
    // Retention is driven by a supplied cutoff, independent of wall-clock waiting.
    history.purge_before(1020, &cancel).await.unwrap();
    let retained = query(&url, &select());
    assert_eq!(retained.0, 200);
    assert!(
        retained.1["history_hot_region"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["update_time"].as_i64().unwrap() >= 1020)
    );
    let mutation = history.encode(&[row(2)]).unwrap().pop().unwrap();
    let Mutation::Put { key, .. } = mutation else {
        unreachable!()
    };
    history.append(&[row(2)], &cancel).await.unwrap();
    let key = store
        .read(Read::prefix(
            &[key, b"/".to_vec()].concat(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .entries
        .last()
        .unwrap()
        .key
        .clone();
    store
        .transact(vec![], vec![Mutation::put(&key, b"bad json")])
        .await
        .unwrap();
    assert_eq!(query(&url, &select()).0, 500);
    store
        .transact(vec![], vec![Mutation::Delete { key }])
        .await
        .unwrap();
    let mut encrypted = row(3);
    encrypted.encryption_meta = Some(json!({"key_id":1,"iv":"AA=="}));
    assert!(history.append(&[encrypted.clone()], &cancel).await.is_err());
    let Mutation::Put { key, .. } = history.encode(&[row(3)]).unwrap().pop().unwrap() else {
        unreachable!()
    };
    history.append(&[row(3)], &cancel).await.unwrap();
    let key = store
        .read(Read::prefix(
            &[key, b"/".to_vec()].concat(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .entries
        .last()
        .unwrap()
        .key
        .clone();
    store
        .transact(
            vec![],
            vec![Mutation::put(
                &key,
                &serde_json::to_vec(&encrypted).unwrap(),
            )],
        )
        .await
        .unwrap();
    assert_eq!(
        query(&url, &select()),
        (
            500,
            json!("history region encryption key manager is not configured")
        )
    );
    restored.shutdown().await.unwrap();
}

async fn hot_sample(url: &str, id: u64) -> Value {
    use crate::grpc_region_lookup_scenarios_test::header;
    use astersql_pd_proto::{
        metapb,
        pdpb::{self, pd_client::PdClient},
    };
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    let peer = metapb::Peer {
        id: 3,
        store_id: 1,
        ..Default::default()
    };
    let mut region = metapb::Region {
        id: 2,
        start_key: vec![],
        end_key: vec![],
        region_epoch: Some(metapb::RegionEpoch {
            version: 1,
            conf_ver: 1,
        }),
        peers: vec![peer.clone()],
        ..Default::default()
    };
    assert!(
        client
            .bootstrap(pdpb::BootstrapRequest {
                header: header(id),
                store: Some(metapb::Store {
                    id: 1,
                    version: "8.5.0".into(),
                    ..Default::default()
                }),
                region: Some(region.clone())
            })
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .error
            .is_none()
    );
    region.end_key = vec![0xff, 0];
    region.region_epoch.as_mut().unwrap().version = 2;
    let (tx, rx) = tokio::sync::mpsc::channel(32);
    let report = |n: u64| pdpb::RegionHeartbeatRequest {
        header: header(id),
        region: Some(region.clone()),
        leader: Some(peer.clone()),
        interval: Some(pdpb::TimeInterval {
            start_timestamp: 1000,
            end_timestamp: 1060,
        }),
        approximate_size: 1048576,
        approximate_keys: n,
        bytes_written: 60 * 1048576,
        keys_written: 60000,
        query_stats: Some(pdpb::QueryStats {
            put: 6000,
            ..Default::default()
        }),
        ..Default::default()
    };
    tx.send(report(1)).await.unwrap();
    let stream = client
        .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap();
    for n in 2..=20 {
        tx.send(report(n)).await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let out = std::process::Command::new("curl")
                    .args([
                        "-sS",
                        "--max-time",
                        "5",
                        &format!("{url}/pd/api/v1/region/id/2"),
                    ])
                    .output()
                    .unwrap();
                assert!(out.status.success());
                let r = (200, serde_json::from_slice::<Value>(&out.stdout).unwrap());
                if r.1["approximate_keys"] == n {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(
            client
                .store_heartbeat(pdpb::StoreHeartbeatRequest {
                    header: header(id),
                    stats: Some(pdpb::StoreStats {
                        store_id: 1,
                        interval: Some(pdpb::TimeInterval {
                            start_timestamp: n * 60,
                            end_timestamp: (n + 1) * 60
                        }),
                        capacity: 1000000000,
                        available: 900000000,
                        bytes_read: 60 * 1048576,
                        keys_read: 60000,
                        peer_stats: vec![pdpb::PeerStat {
                            region_id: 2,
                            read_bytes: 60 * 1048576,
                            read_keys: 60000,
                            query_stats: Some(pdpb::QueryStats {
                                get: 6000,
                                ..Default::default()
                            }),
                            ..Default::default()
                        }],
                        ..Default::default()
                    }),
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner()
                .header
                .unwrap()
                .error
                .is_none()
        );
    }
    // Wait until the hot-cache worker has consumed the final report before enabling sampling.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let live = request(url, "GET", "/pd/api/v1/hotspot/regions/write", "");
    assert_eq!(live.0, 200);
    assert_eq!(live.1["as_peer"]["1"]["regions_count"], 1);
    assert_eq!(
        request(
            url,
            "POST",
            "/pd/api/v1/config",
            r#"{"hot-regions-write-interval":"100ms"}"#
        )
        .0,
        200
    );
    let q = json!({"start_time":1000000,"end_time":1100000,"hot_region_type":["write"],"is_leaders":[true],"is_learners":[false]});
    let response = tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            let r = query(url, &q);
            assert_eq!(r.0, 200, "{r:?}");
            if r.1["history_hot_region"].as_array().is_some_and(|a| {
                a.len() == 1
                    && a[0]["hot_degree"] == live.1["as_peer"]["1"]["statistics"][0]["hot_degree"]
            }) {
                break r.1;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(response["history_hot_region"][0]["flow_bytes"], 1048576.);
    assert_eq!(response["history_hot_region"][0]["peer_id"], 3);
    assert_eq!(response["history_hot_region"][0]["update_time"], 1060000);
    let read_q = json!({"start_time":1000000,"end_time":1100000,"hot_region_type":["read"],"is_leaders":[true],"is_learners":[false]});
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let r = query(url, &read_q);
            assert_eq!(r.0, 200);
            if r.1["history_hot_region"]
                .as_array()
                .is_some_and(|a| a.len() == 1 && a[0]["hot_degree"] == 19)
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    drop(tx);
    drop(stream);
    response
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_061_sampler_and_restart() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let (mut node, url, _) = start(store.clone(), None).await;
    let sample = hot_sample(&url, 761).await;
    let saved = store
        .read(Read::prefix(
            b"/pd/761/rust/hot-history/1/",
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .entries;
    assert!(!saved.is_empty());
    node.shutdown().await.unwrap();
    let (mut node, url, _) = start(store.clone(), Some(url.strip_prefix("http://").unwrap())).await;
    let q = json!({"start_time":1000000,"end_time":1100000,"hot_region_type":["write"],"is_leaders":[true],"is_learners":[false]});
    assert_eq!(query(&url, &q), (200, sample));
    node.shutdown().await.unwrap();
    assert_eq!(
        saved,
        store
            .read(Read::prefix(
                b"/pd/761/rust/hot-history/1/",
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries
    );
}
fn input_cases(url: &str) -> Value {
    let cases = [
        "{}",
        "null",
        "",
        "{",
        "x",
        "{]",
        r#"{"start_time" 1}"#,
        r#"{"start_time":1 2}"#,
        "[1 2]",
        "{}x",
        r#"{"a":"\x"}"#,
        r#"{"start_time":9223372036854775808}"#,
        r#"{"end_time":-9223372036854775809}"#,
        r#"{"start_time":1.0}"#,
        r#"{"start_time":1e309}"#,
        r#"{"region_ids":[-1]}"#,
        r#"{"region_ids":[18446744073709551616]}"#,
        r#"{"region_ids":[1e2]}"#,
        r#"{"unknown":1e999}"#,
        "[]",
        "true",
        "1",
        "\"s\"",
        r#"{"start_time":"bad"}"#,
        r#"{"end_time":true}"#,
        r#"{"region_ids":1}"#,
        r#"{"region_ids":["1"]}"#,
        r#"{"store_ids":null,"peer_ids":[null],"is_leaders":[null],"is_learners":[null],"hot_region_type":[null]}"#,
        r#"{"is_leaders":[1]}"#,
        r#"{"hot_region_type":[false]}"#,
        r#"{"START_TIME":0,"start_time":1,"end_time":2}"#,
        r#"{"start_time":2,"start_time":1,"end_time":2}"#,
        r#"{"unknown":{},"region_ids":[18446744073709551615]}"#,
        r#"{"region_ids":[],"is_learners":[false,true],"is_leaders":[false,true],"start_time":-1,"end_time":-2}"#,
    ];
    let mut output = cases
        .iter()
        .map(|body| json!([body, request(url, "GET", PATH, body)]))
        .collect::<Vec<_>>();
    for method in ["HEAD", "POST", "PUT", "DELETE", "OPTIONS"] {
        let out = std::process::Command::new("curl")
            .args([
                "-sS",
                "--max-time",
                "5",
                "-H",
                "Connection: close",
                "-X",
                method,
                "--data-binary",
                "{}",
                "-w",
                "\n%{http_code}\n%{content_type}",
                &format!("{url}{PATH}"),
            ])
            .output()
            .unwrap();
        assert!(out.status.success(), "{method}: {out:?}");
        let text = String::from_utf8(out.stdout).unwrap();
        let (text, content) = text.rsplit_once('\n').unwrap();
        let (body, code) = text.rsplit_once('\n').unwrap();
        let code: u16 = code.parse().unwrap();
        assert_eq!(
            code,
            if method == "OPTIONS" { 200 } else { 405 },
            "{method}"
        );
        assert_eq!(body, "");
        output.push(json!([method, [code, content, body]]));
    }
    json!(output)
}
fn history_cases(url: &str) -> Value {
    let base = json!({"start_time":1000000,"end_time":1100000,"is_leaders":[false,true],"is_learners":[false,true]});
    let patches = [
        json!({}),
        json!({"region_ids":[2]}),
        json!({"region_ids":[3]}),
        json!({"store_ids":[1]}),
        json!({"store_ids":[3]}),
        json!({"peer_ids":[3]}),
        json!({"peer_ids":[1]}),
        json!({"is_leaders":[false]}),
        json!({"is_leaders":[]}),
        json!({"is_learners":[true]}),
        json!({"is_learners":[]}),
        json!({"hot_region_type":["WRITE","read","write"]}),
        json!({"hot_region_type":["no-such-type"]}),
        json!({"start_time":1060000,"end_time":1060000}),
        json!({"start_time":1060001}),
        json!({"end_time":1059999}),
    ];
    let mut out = Vec::new();
    for patch in patches {
        let mut q = base.clone();
        for (k, v) in patch.as_object().unwrap() {
            q[k] = v.clone();
        }
        let (code, value) = query(url, &q);
        assert_eq!(code, 200);
        let rows: Option<Vec<HistoryHotRegion>> =
            serde_json::from_value(value["history_hot_region"].clone()).unwrap();
        out.push(json!([q, rows]));
    }
    for fields in [
        r#""is_leaders":[true],"IS_LEADERS":[false]"#,
        r#""is_leaders":[false],"IS_LEADERS":[true]"#,
        r#""is_leaders":[true],"is_leaders":null"#,
        r#""start_time":999999999,"START_TIME":1000000"#,
        r#""ſtart_time":999999999"#,
    ] {
        let body = format!(
            "{{\"start_time\":1000000,\"end_time\":1100000,\"is_learners\":[false,true],\"is_leaders\":[true],{fields}}}"
        );
        let (code, value) = request(url, "GET", PATH, &body);
        assert_eq!(code, 200);
        let rows: Option<Vec<HistoryHotRegion>> =
            serde_json::from_value(value["history_hot_region"].clone()).unwrap();
        out.push(json!([body, rows]));
    }
    assert_eq!(
        out[0][1].as_array().unwrap().len(),
        2,
        "must compare real read and write history"
    );
    json!(out)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_061_differential() {
    use astersql_pd_proto::pdpb::{GetMembersRequest, pd_client::PdClient};
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let (mut node, url, _) = start(store, None).await;
    let rust_inputs = input_cases(&url);
    let rust = hot_sample(&url, 761).await;
    let rust_cases = history_cases(&url);
    node.shutdown().await.unwrap();
    let binary = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    let path = std::path::PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap())
        .join(format!("task061-{}.toml", std::process::id()));
    std::fs::write(
        &path,
        "[schedule]\nhot-regions-write-interval = \"100ms\"\n",
    )
    .unwrap();
    let result = astersql_pd_compat_tests::harness::Server::start_with_args(
        &binary,
        &[&format!("--config={}", path.display())],
    );
    let mut go = result.unwrap();
    go.ready().unwrap();
    std::fs::remove_file(path).unwrap();
    let id = PdClient::connect(go.url.clone())
        .await
        .unwrap()
        .get_members(GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let go_inputs = input_cases(&go.url);
    let go_history = hot_sample(&go.url, id).await;
    let go_cases = history_cases(&go.url);
    let q = json!({"start_time":1000000,"end_time":1100000,"hot_region_type":["write"],"is_leaders":[true],"is_learners":[false]});
    go.restart_with_args(&binary, &[]).unwrap();
    go.ready().unwrap();
    assert_eq!(query(&go.url, &q), (200, go_history.clone()));
    go.stop().unwrap();
    println!(
        "Go inputs: {go_inputs}\nRust inputs: {rust_inputs}\nGo history: {go_history}\nRust history: {rust}"
    );
    assert_eq!(rust_inputs, go_inputs);
    println!("Go history cases: {go_cases}\nRust history cases: {rust_cases}");
    assert_eq!(rust_cases, go_cases);
    let a: Vec<HistoryHotRegion> =
        serde_json::from_value(rust["history_hot_region"].clone()).unwrap();
    let b: Vec<HistoryHotRegion> =
        serde_json::from_value(go_history["history_hot_region"].clone()).unwrap();
    assert_eq!(a, b);
}

use astersql_pd_storage::{
    Commit, Compare, Lease, LeaseId, MetaWatch, RawTxnOp, RawTxnResponse, Revision, Snapshot,
};
struct HistoryStore {
    inner: Arc<dyn MetaStore>,
    mode: std::sync::atomic::AtomicU8,
    entered: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl MetaStore for HistoryStore {
    async fn consensus_cluster_id(&self) -> astersql_pd_storage::Result<Option<u64>> {
        self.inner.consensus_cluster_id().await
    }
    async fn consensus_leader_id(&self) -> astersql_pd_storage::Result<Option<u64>> {
        self.inner.consensus_leader_id().await
    }
    async fn read(&self, mut request: Read) -> astersql_pd_storage::Result<Snapshot> {
        if request
            .start
            .as_ref()
            .is_some_and(|k| String::from_utf8_lossy(k).contains("/schedule/hot_region/"))
        {
            match self.mode.load(std::sync::atomic::Ordering::SeqCst) {
                1 => {
                    self.mode.store(0, std::sync::atomic::Ordering::SeqCst);
                    request.revision = Some(i64::MAX);
                }
                6 if request
                    .start
                    .as_ref()
                    .is_some_and(|key| key.last() == Some(&0)) =>
                {
                    self.mode.store(0, std::sync::atomic::Ordering::SeqCst);
                    let mut row = row(3);
                    row.flow_bytes = 999.;
                    HotHistory::new(self.inner.clone(), 761, 1)
                        .append(&[row], &CancellationToken::new())
                        .await
                        .unwrap();
                }
                5 => {
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
        mut writes: Vec<Mutation>,
    ) -> astersql_pd_storage::Result<Commit> {
        let mode = self.mode.load(std::sync::atomic::Ordering::SeqCst);
        if writes.iter().any(|w| match w {
            Mutation::Put { key, .. } => {
                let s = String::from_utf8_lossy(key);
                (mode == 3 && s.contains("/schedule/hot_region/"))
                    || (mode == 4 && s.contains("/published/"))
            }
            _ => false,
        }) {
            self.mode.store(0, std::sync::atomic::Ordering::SeqCst);
            // Ask the real etcd transaction to bind a nonexistent lease.
            writes.push(Mutation::Put {
                key: b"/task061/nonexistent-lease".to_vec(),
                value: vec![],
                lease: Some(i64::MAX),
            });
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
async fn task_061_real_faults_atomic_publication_and_cancel() {
    use std::sync::atomic::{AtomicU8, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let fixture = crate::etcd_fixture::Fixture::start();
    let inner = Arc::new(fixture.store().await);
    let store = Arc::new(HistoryStore {
        inner: inner.clone(),
        mode: AtomicU8::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let history = HotHistory::new(store.clone(), 761, 1);
    let cancel = CancellationToken::new();
    history.append(&[row(2)], &cancel).await.unwrap();
    let q = HistoryQuery {
        start_time: 1000,
        end_time: 1100,
        is_leaders: vec![false, true],
        is_learners: vec![false, true],
        ..Default::default()
    };
    let before = history.query(&q, &cancel).await.unwrap();
    for mode in [3, 4] {
        store.mode.store(mode, Ordering::SeqCst);
        assert!(
            history
                .append(&(3..=144).map(row).collect::<Vec<_>>(), &cancel)
                .await
                .is_err()
        );
        assert_eq!(
            history.query(&q, &cancel).await.unwrap(),
            before,
            "partial/unpublished flush must be invisible: {mode}"
        );
    }
    // A later complete flush succeeds beyond MetaStore's single-transaction budget.
    history
        .append(&(3..=144).map(row).collect::<Vec<_>>(), &cancel)
        .await
        .unwrap();
    assert_eq!(
        history.query(&q, &cancel).await.unwrap().unwrap().len(),
        143
    );
    let snapshot = history.query(&q, &cancel).await.unwrap();
    store.mode.store(6, Ordering::SeqCst);
    assert_eq!(
        history.query(&q, &cancel).await.unwrap(),
        snapshot,
        "a historical read header must not advance the pinned snapshot"
    );
    assert_eq!(
        store.mode.load(Ordering::SeqCst),
        0,
        "must cross a backend page boundary"
    );
    assert_ne!(
        history.query(&q, &cancel).await.unwrap(),
        snapshot,
        "the concurrent write really committed"
    );
    let (mut node, url, shutdown) = start(store.clone(), None).await;
    store.mode.store(1, Ordering::SeqCst);
    assert_eq!(
        query(&url, &select()),
        (500, json!("revision is newer than the store"))
    );
    let before = inner
        .read(Read::prefix(
            b"/pd/761/rust/hot-history/",
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .entries;
    store.mode.store(5, Ordering::SeqCst);
    let mut socket = tokio::net::TcpStream::connect(url.strip_prefix("http://").unwrap())
        .await
        .unwrap();
    let body = select().to_string();
    socket.write_all(format!("GET {PATH} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), store.entered.notified())
        .await
        .unwrap();
    shutdown.cancel();
    let mut response = Vec::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        socket.read_to_end(&mut response),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(String::from_utf8_lossy(&response).starts_with("HTTP/1.1 500"));
    tokio::time::timeout(std::time::Duration::from_secs(10), node.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        before,
        inner
            .read(Read::prefix(
                b"/pd/761/rust/hot-history/",
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries
    );
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(history.query(&q, &cancelled).await.is_err());
    assert!(history.append(&[row(300)], &cancelled).await.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_061_follower_local_history_and_forwarding() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let cancel = CancellationToken::new();
    HotHistory::new(store.clone(), 761, 1)
        .append(&[row(2)], &cancel)
        .await
        .unwrap();
    HotHistory::new(store.clone(), 761, 2)
        .append(&[row(4)], &cancel)
        .await
        .unwrap();
    let (mut leader, url, _) = start(store.clone(), None).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", listener.local_addr().unwrap());
    let mut follower =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 761,
                    member_id: 2,
                    name: "history-follower".into(),
                    client_url: follower_url.clone(),
                    peer_url: "http://peer2".into(),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    follower.ready().await.unwrap();
    assert_eq!(query(&follower_url, &select()), query(&url, &select()));
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "5",
            "-H",
            "pd-allow-follower-handle: true",
            "-X",
            "GET",
            "--data-binary",
            &select().to_string(),
            &format!("{follower_url}{PATH}"),
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["history_hot_region"][0]["region_id"], 4);
    leader.shutdown().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            let result = query(&follower_url, &select());
            if result.0 == 200 && result.1["history_hot_region"][0]["region_id"] == 4 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    follower.shutdown().await.unwrap();
    assert_eq!(
        HotHistory::new(store, 761, 1)
            .query(
                &HistoryQuery {
                    start_time: 1000,
                    end_time: 1100,
                    is_leaders: vec![false, true],
                    is_learners: vec![false, true],
                    ..Default::default()
                },
                &cancel
            )
            .await
            .unwrap()
            .unwrap()[0]
            .region_id,
        2
    );
}
