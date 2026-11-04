// Copyright 2026 AsterSQL.
use crate::{NodeConfig, PdNodeBuilder, node_test::fixture, recovery_start_test::seed};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_keyspace::{
    BoundType, Kernel, build_label_rule, make_region_bound, parse_keyspace_id_from_label_rule,
};
use astersql_pd_schedule_labeler::LabelRule;
use astersql_pd_storage::{Consistency, MetaStore, Read};
use astersql_pd_storage_endpoint::StorageEndpoint;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
fn request(url: &str, version: u8, path: &str, body: Option<&Value>) -> (u16, Value) {
    let mut cmd = std::process::Command::new("curl");
    cmd.args(["-sS", "--max-time", "8", "-w", "\n%{http_code}"]);
    if let Some(body) = body {
        cmd.args([
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "--data-binary",
            &body.to_string(),
        ]);
    }
    let out = cmd
        .arg(format!("{url}/pd/api/v{version}/{path}"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    let (body, status) = text.rsplit_once('\n').unwrap();
    (status.parse().unwrap(), serde_json::from_str(body).unwrap())
}
fn rules(url: &str) -> Vec<Value> {
    let (status, v) = request(url, 1, "config/region-label/rules", None);
    assert_eq!(status, 200);
    v.as_array().unwrap().clone()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_025_real_go_keyspace_rules_names_and_rust_labeler_persistence() {
    use astersql_pd_compat_tests::{harness::Server, readonly_oracle::pinned_go_binary};
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap())
        .join(format!("mcs025-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _directory = Directory(root.clone());
    for (key_type, bound) in [("raw", BoundType::Raw), ("table", BoundType::Txn)] {
        let config = root.join("pd.toml");
        std::fs::write(
            &config,
            format!("[pd-server]\nkey-type=\"{key_type}\"\n[keyspace]\nwait-region-split=false\n"),
        )
        .unwrap();
        let mut go = Server::start_with_args(
            &pinned_go_binary().unwrap(),
            &[&format!("--config={}", config.display())],
        )
        .unwrap();
        go.ready_with_timeout(Duration::from_secs(12)).unwrap();
        let (_, go_cluster, _) = seed(&go.url).await;
        let fixture = fixture::Fixture::start();
        let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
        StorageEndpoint::new(store.clone())
            .save_config(
                &ClusterPaths::new(2500),
                &json!({"pd-server":{"key-type":key_type}}),
            )
            .await
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mut node =
            PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
                .start(
                    store.clone(),
                    NodeConfig {
                        cluster_id: 2500,
                        member_id: 25,
                        name: "bounds".into(),
                        client_url: url.clone(),
                        peer_url: "http://127.0.0.1:2380".into(),
                    },
                    listener,
                    Arc::new(prometheus::Registry::new()),
                )
                .await
                .unwrap();
        node.ready().await.unwrap();
        seed(&url).await;
        for (position, name) in [
            "keyspaceName1",
            "keyspace_name_1",
            "10",
            "789z-_",
            "18446744073709551615",
            "-----",
            "SYSTEM",
            "",
            "DEFAULT",
            "bad/name",
            "789z-_)",
            "18446744073709551615a",
            "中",
        ]
        .into_iter()
        .enumerate()
        {
            let response = request(&go.url, 2, "keyspaces", Some(&json!({"name":name})));
            match Kernel::Classic.validate_name(name) {
                Ok(()) => assert_eq!(response.0, 200, "{name}: {response:?}"),
                Err(error) => {
                    assert_eq!(
                        response,
                        (500, json!(error.to_string())),
                        "name case {position}"
                    );
                }
            }
            println!("key_type={key_type},Go name={name:?},actual result={response:?}");
        }
        let go_store = astersql_pd_storage_kv::EtcdMetaStore::connect(
            &[go.url.clone()],
            Duration::from_secs(2),
        )
        .await
        .unwrap();
        for id in [1000, 0x010203, 0x0102ff, 0xffffff] {
            let response = request(
                &go.url,
                2,
                "keyspaces/id",
                Some(&json!({"id":id,"name":format!("bound_{id}")})),
            );
            assert_eq!(response.0, 200, "{response:?}");
            let rule = build_label_rule(id, bound);
            let expected = serde_json::to_value(&rule).unwrap();
            let actual = rules(&go.url)
                .into_iter()
                .find(|r| r["id"] == rule.id)
                .expect("real Go create must persist its actual keyspace label rule");
            assert_eq!(actual, expected, "{key_type},id={id}");
            assert_eq!(
                request(&url, 1, "config/region-label/rule", Some(&expected)).0,
                200
            );
            let actual_rust = rules(&url)
                .into_iter()
                .find(|r| r["id"] == rule.id)
                .unwrap();
            assert_eq!(actual_rust, actual);
            let decoded = LabelRule::from_json(&serde_json::to_vec(&actual).unwrap(), 0).unwrap();
            assert_eq!(parse_keyspace_id_from_label_rule(Some(&decoded)), Some(id));
            let b = make_region_bound(id);
            let (start, end) = bound.bounds(&b);
            assert_eq!(decoded.key_ranges().unwrap()[0].start_key, start);
            assert_eq!(decoded.key_ranges().unwrap()[0].end_key, end);
            for (name, backend, cluster) in [
                ("Go", &go_store as &dyn MetaStore, go_cluster),
                ("Rust", store.as_ref(), 2500),
            ] {
                let prefix = ClusterPaths::new(cluster).region_label_path_prefix();
                let read = backend
                    .read(Read::prefix(prefix.as_bytes(), Consistency::Linearizable))
                    .await
                    .unwrap();
                let persisted = read
                    .entries
                    .iter()
                    .find(|entry| entry.key.ends_with(rule.id.as_bytes()))
                    .expect("real persistent key");
                let value: Value = serde_json::from_slice(&persisted.value).unwrap();
                assert_eq!(value, expected);
                println!(
                    "{name},key_type={key_type},id={id},header revision={},key mod_revision={},persisted={value}",
                    read.revision, persisted.mod_revision
                );
            }
        }
        node.shutdown().await.unwrap();
        go.stop().unwrap();
    }
}
