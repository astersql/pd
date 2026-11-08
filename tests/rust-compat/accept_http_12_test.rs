// Copyright 2026 AsterSQL.
use crate::{harness::Server, http_regression::report_path};
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_012_behavior() {
    let rust = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut s = Server::start_rust(&rust).unwrap();
    s.ready().unwrap();
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "5",
            "-X",
            "GET",
            "-w",
            "\n%{http_code}",
            &format!("{}/pd/api/v2/meta-service-groups", s.url),
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let body = String::from_utf8(out.stdout).unwrap();
    assert!(
        body.ends_with("\n500"),
        "unbootstrapped keyspace request must reach bootstrap gate: {body}"
    );
    s.stop().unwrap();
}
use crate::accept_http_10_test::http;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_storage_domain::{Consistency, MetaStore, Read};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
fn routes() -> Vec<(&'static str, &'static str)> {
    include_str!("../../server/apiv2/handlers/meta_service_group.go")
        .lines()
        .filter_map(|l| {
            let (m, r) = l.trim().strip_prefix("router.")?.split_once("(\"")?;
            Some((m, r.split('"').next()?))
        })
        .collect()
}
fn normalized(v: Value, endpoint: &str) -> Value {
    serde_json::from_str(
        &serde_json::to_string(&v)
            .unwrap()
            .replace(endpoint, "HEALTHY_ENDPOINT"),
    )
    .unwrap()
}
fn file_http(url: &str, body: &str) -> Value {
    let path = std::env::temp_dir().join(format!("sa012-body-{}.json", std::process::id()));
    std::fs::write(&path, body).unwrap();
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "40",
            "-X",
            "PATCH",
            "-H",
            "Content-Type: application/json",
            "-H",
            "Expect:",
            "--data-binary",
            &format!("@{}", path.display()),
            "-D",
            "-",
            "-w",
            "\n%{http_code}",
            &format!("{url}/pd/api/v2/meta-service-groups"),
        ])
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(out.status.success(), "{out:?}");
    let raw = String::from_utf8(out.stdout).unwrap();
    let (headers, body) = raw.split_once("\r\n\r\n").unwrap();
    let (body, status) = body.rsplit_once('\n').unwrap();
    let ct = headers.lines().find_map(|l| {
        l.to_lowercase()
            .strip_prefix("content-type:")
            .map(|s| s.trim().to_owned())
    });
    json!({"status":status.parse::<u16>().unwrap(),"content_type":ct,"location":null,"body":serde_json::from_str::<Value>(body).unwrap_or(json!(body)),"raw":raw})
}
async fn state(store: &dyn MetaStore, paths: &ClusterPaths) -> Value {
    let key = format!("/pd/{}/meta_service_groups/", paths.cluster_id());
    let entries = store
        .read(Read::prefix(key.as_bytes(), Consistency::Linearizable))
        .await
        .unwrap()
        .entries;
    json!(entries.into_iter().map(|e|json!({"key":String::from_utf8(e.key).unwrap().strip_prefix(&key).unwrap(),"bytes":e.value,"value":serde_json::from_slice::<Value>(&e.value).unwrap(),"create_revision":e.create_revision,"mod_revision":e.mod_revision})).collect::<Vec<_>>())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_012_dual_matrix_and_wal() {
    assert_eq!(std::env::var("PD_COMPAT_MODE").as_deref(), Ok("dual"));
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut reports = vec![];
    for api in [false, true] {
        let mut compared = vec![];
        for is_rust in [false, true] {
            let config_path = std::env::temp_dir()
                .join(format!("sa012-{}-{api}-{is_rust}.toml", std::process::id()));
            std::fs::write(
                &config_path,
                "max-request-bytes=1500000\n[keyspace]\nwait-region-split=false\n",
            )
            .unwrap();
            let config_arg = format!("--config={}", config_path.display());
            let env = if api {
                vec![("PD_SERVICE_MODE".into(), "api".into())]
            } else {
                vec![]
            };
            let mut s = if is_rust {
                Server::start_rust_with_environment(&rust, &[&config_arg], &env)
            } else {
                Server::start_with_environment(&go, &[&config_arg], &env)
            }
            .unwrap();
            s.ready().unwrap();
            let endpoint = s.metadata_url().to_owned();
            let mut pd = PdClient::connect(s.url.clone()).await.unwrap();
            let id = pd
                .get_members(pdpb::GetMembersRequest::default())
                .await
                .unwrap()
                .into_inner()
                .header
                .unwrap()
                .cluster_id;
            let store: Arc<dyn MetaStore> = Arc::new(
                astersql_pd_storage_kv::EtcdMetaStore::connect(
                    &[endpoint.clone()],
                    Duration::from_secs(2),
                )
                .await
                .unwrap(),
            );
            let paths = ClusterPaths::new(id);
            let mut observations = vec![];
            let mut physical = vec![];
            for (m, p) in routes() {
                let path = format!("v2/meta-service-groups{}", p.replace(":id", "g"));
                let r = http(&s.url, m, &path, "{}");
                assert_eq!(r["status"], 500);
                observations
                    .push(json!({"stage":"unbootstrapped","method":m,"path":path,"response":r}));
            }
            let boot = pd
                .bootstrap(pdpb::BootstrapRequest {
                    header: Some(pdpb::RequestHeader {
                        cluster_id: id,
                        ..Default::default()
                    }),
                    store: Some(metapb::Store {
                        id: 1,
                        ..Default::default()
                    }),
                    region: Some(metapb::Region {
                        id: 2,
                        peers: vec![metapb::Peer {
                            id: 3,
                            store_id: 1,
                            ..Default::default()
                        }],
                        ..Default::default()
                    }),
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner();
            assert!(boot.header.unwrap().error.is_none());
            let mut cases: Vec<(String, String, String)> = vec![];
            for (m, p) in routes() {
                let p = p.replace(":id", "missing");
                cases.push((m.into(), p.clone(), "{}".into()));
                cases.push((m.into(), format!("{p}/"), "{}".into()));
            }
            for body in [
                "",
                "{",
                "[]",
                "null",
                "1",
                "true",
                "\"x\"",
                r#"{"g":1}"#,
                r#"{"g":false}"#,
                r#"{"g":[]}"#,
                r#"{"g":{}}"#,
                r#"{"g":1,"g":null}"#,
                r#"{" ":"x"}"#,
                r#"{"a/b":"x"}"#,
                r#"{"g":" "}"#,
                r#"{" g ":"x","g":"x"}"#,
            ] {
                cases.push(("PATCH".into(), "".into(), body.into()));
            }
            cases.extend([
                (
                    "PATCH".into(),
                    "".into(),
                    json!({" g ":format!(" {endpoint} ")}).to_string(),
                ),
                (
                    "PATCH".into(),
                    "/g/status".into(),
                    r#"{"enabled":true,"assignment_count":5}"#.into(),
                ),
                (
                    "PATCH".into(),
                    "/g/status".into(),
                    r#"{"enabled":null,"assignment_count":null}"#.into(),
                ),
                ("PATCH".into(), "/g/status".into(), "null".into()),
                (
                    "PATCH".into(),
                    "/g/status".into(),
                    r#"{"assignment_count":-1}"#.into(),
                ),
                (
                    "PATCH".into(),
                    "/missing/status".into(),
                    r#"{"assignment_count":-1}"#.into(),
                ),
                (
                    "PATCH".into(),
                    "/g/status".into(),
                    r#"{"assignment_count":0}"#.into(),
                ),
                (
                    "PATCH".into(),
                    "".into(),
                    json!({"bad":"http://127.0.0.1:1","g":endpoint}).to_string(),
                ),
                ("GET".into(), "".into(), "".into()),
            ]);
            for body in [
                "",
                "{",
                "[]",
                "1",
                r#"{"enabled":"true"}"#,
                r#"{"assignment_count":"1"}"#,
                r#"{"assignment_count":1.5}"#,
                r#"{"assignment_count":9223372036854775808}"#,
                r#"{"ENABLED":false}"#,
                r#"{"assignment_count":9,"assignment_count":null}"#,
                r#"{"enabled":false,"enabled":null}"#,
                r#"{"enabled":"bad","enabled":true}"#,
                r#"{"assignment_count":false,"enabled":"bad"}"#,
                r#"{"enabled":true}"#,
            ] {
                cases.push(("PATCH".into(), "/g/status".into(), body.into()));
            }
            for (m, p, body) in cases {
                let before = state(store.as_ref(), &paths).await;
                let config_before = store
                    .read(Read::range(
                        paths.config_path().as_bytes(),
                        b"",
                        Consistency::Linearizable,
                    ))
                    .await
                    .unwrap()
                    .entries;
                let path = format!("v2/meta-service-groups{p}");
                let response = http(&s.url, &m, &path, &body);
                let after = state(store.as_ref(), &paths).await;
                let config_after = store
                    .read(Read::range(
                        paths.config_path().as_bytes(),
                        b"",
                        Consistency::Linearizable,
                    ))
                    .await
                    .unwrap()
                    .entries;
                if response["status"].as_u64().unwrap() >= 400 {
                    assert_eq!(before, after, "error mutates state {path} {body}");
                    assert_eq!(config_before, config_after, "error published config {body}");
                }
                physical.push(
                    json!({"method":m,"path":path,"input":body,"before":before,"after":after,"config_before":config_before.iter().map(|e|json!({"bytes":e.value,"create_revision":e.create_revision,"mod_revision":e.mod_revision})).collect::<Vec<_>>(),"config_after":config_after.iter().map(|e|json!({"bytes":e.value,"create_revision":e.create_revision,"mod_revision":e.mod_revision})).collect::<Vec<_>>() }),
                );
                observations.push(json!({"method":m,"path":path,"input":normalized(json!(body),&endpoint),"response":normalized(response,&endpoint)}));
            }
            // Reject a real oversized etcd configuration write after healthy endpoint admission.
            let before_config = store
                .read(Read::range(
                    paths.config_path().as_bytes(),
                    b"",
                    Consistency::Linearizable,
                ))
                .await
                .unwrap()
                .entries;
            let before_groups = http(&s.url, "GET", "v2/meta-service-groups", "");
            let oversized = json!({"x".repeat(1_600_000):endpoint}).to_string();
            let failed = file_http(&s.url, &oversized);
            assert_eq!(
                failed["status"], 500,
                "configuration must exceed real etcd request budget"
            );
            assert_eq!(
                before_groups,
                http(&s.url, "GET", "v2/meta-service-groups", "")
            );
            assert_eq!(
                before_config,
                store
                    .read(Read::range(
                        paths.config_path().as_bytes(),
                        b"",
                        Consistency::Linearizable
                    ))
                    .await
                    .unwrap()
                    .entries
            );
            std::fs::write(
                report_path(format!("/tmp/sa012-config-failure-{api}-{is_rust}.json")),
                serde_json::to_vec_pretty(&failed).unwrap(),
            )
            .unwrap();
            observations.push(json!({"stage":"configuration-failure","response": {"status":failed["status"],"content_type":failed["content_type"],"body":failed["body"],"location":failed["location"]}}));
            // The same manager supplies HTTP group counts and actual keyspace creation.
            let created = http(
                &s.url,
                "POST",
                "v2/keyspaces",
                r#"{"name":"meta-count","config":{"user_kind":"basic"}}"#,
            );
            assert_eq!(created["status"], 200, "{created}");
            assert!(created["body"]["id"].as_u64().is_some());
            assert_eq!(created["body"]["config"]["meta_service_group_id"], "g");
            // Updating existing endpoints must decorate the same persisted keyspace without changing its count.
            let changed_addresses = format!("{endpoint},{endpoint}");
            let changed = http(
                &s.url,
                "PATCH",
                "v2/meta-service-groups",
                &json!({"g":changed_addresses}).to_string(),
            );
            assert_eq!(changed["status"], 200);
            assert_eq!(changed["body"][0]["assigned_keyspaces"], 1);
            let loaded = http(&s.url, "GET", "v2/keyspaces/meta-count", "");
            assert_eq!(
                loaded["body"]["config"]["meta_service_group_addrs"],
                changed_addresses
            );
            observations.push(json!({"stage":"updated-endpoints","response":normalized(changed,&endpoint),"addresses":normalized(loaded["body"]["config"]["meta_service_group_addrs"].clone(),&endpoint)}));
            let restored = http(
                &s.url,
                "PATCH",
                "v2/meta-service-groups",
                &json!({"g":endpoint}).to_string(),
            );
            assert_eq!(restored["status"], 200);
            observations.push(
                json!({"stage":"restored-endpoints","response":normalized(restored,&endpoint)}),
            );
            for (m, p, b) in [
                ("GET", "", String::new()),
                ("PATCH", "/g/status", r#"{"assignment_count":0}"#.into()),
                ("PATCH", "", r#"{"g":null}"#.into()),
            ] {
                let r = http(&s.url, m, &format!("v2/meta-service-groups{p}"), &b);
                if b.contains("null") {
                    assert_eq!(
                        r["status"], 400,
                        "real assignments must protect deletion: {r}"
                    );
                }
                observations.push(json!({"stage":"assigned","method":m,"path":p,"response":normalized(r,&endpoint)}));
            }
            let before_restart = state(store.as_ref(), &paths).await;
            assert!(!before_restart.as_array().unwrap().is_empty());
            let config_entries = store
                .read(Read::range(
                    paths.config_path().as_bytes(),
                    b"",
                    Consistency::Linearizable,
                ))
                .await
                .unwrap()
                .entries;
            let saved: Value = serde_json::from_slice(&config_entries[0].value).unwrap();
            assert_eq!(saved["keyspace"]["wait-region-split"], false);
            assert_eq!(saved["keyspace"]["meta-service-groups"]["g"], endpoint);
            drop(pd);
            drop(store);
            if is_rust {
                s.restart_rust(&rust).unwrap();
            } else {
                s.restart_with_args(&go, &[]).unwrap();
            }
            s.ready().unwrap();
            let store: Arc<dyn MetaStore> = Arc::new(
                astersql_pd_storage_kv::EtcdMetaStore::connect(
                    &[s.metadata_url().into()],
                    Duration::from_secs(2),
                )
                .await
                .unwrap(),
            );
            assert_eq!(
                before_restart,
                state(store.as_ref(), &paths).await,
                "WAL changed persisted status"
            );
            let response = http(&s.url, "GET", "v2/meta-service-groups", "");
            assert_eq!(response["body"][0]["id"], "g");
            observations.push(json!({"stage":"restart","response":normalized(response,&endpoint)}));
            for status in ["DISABLED", "ARCHIVED", "TOMBSTONE"] {
                let r = http(
                    &s.url,
                    "PUT",
                    "v2/keyspaces/meta-count/state",
                    &json!({"state":status}).to_string(),
                );
                assert_eq!(r["status"], 200, "{r}");
            }
            for (m, p, b) in [
                ("PATCH", "", r#"{"g":null}"#.to_owned()),
                ("PATCH", "", json!({"g":endpoint}).to_string()),
                ("GET", "", String::new()),
            ] {
                let r = http(&s.url, m, &format!("v2/meta-service-groups{p}"), &b);
                assert_eq!(r["status"], 200, "{r}");
                observations.push(
                    json!({"stage":"delete-readd","method":m,"response":normalized(r,&endpoint)}),
                );
            }
            // An incomplete request body must never reach a domain write.
            let before_cancel = state(store.as_ref(), &paths).await;
            let config_before_cancel = store
                .read(Read::range(
                    paths.config_path().as_bytes(),
                    b"",
                    Consistency::Linearizable,
                ))
                .await
                .unwrap()
                .entries;
            for suffix in ["", "/g/status"] {
                use std::io::Write;
                let mut tcp =
                    std::net::TcpStream::connect(s.url.trim_start_matches("http://")).unwrap();
                write!(tcp,"PATCH /pd/api/v2/meta-service-groups{suffix} HTTP/1.1\r\nHost: localhost\r\nContent-Length: 999999\r\n\r\n{{\"g\":").unwrap();
                drop(tcp);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
            assert_eq!(before_cancel, state(store.as_ref(), &paths).await);
            assert_eq!(
                config_before_cancel,
                store
                    .read(Read::range(
                        paths.config_path().as_bytes(),
                        b"",
                        Consistency::Linearizable
                    ))
                    .await
                    .unwrap()
                    .entries
            );
            let args = if is_rust {
                vec![
                    format!("--etcd-endpoints={}", s.metadata_url()),
                    "--name=follower12".into(),
                ]
            } else {
                vec![
                    format!("--join={}", s.url),
                    "--initial-cluster=".into(),
                    "--name=follower12".into(),
                ]
            };
            let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
            let mut follower =
                Server::start_with_environment(if is_rust { &rust } else { &go }, &refs, &env)
                    .unwrap();
            follower.ready().unwrap();
            for (m, p) in routes() {
                let path = format!("v2/meta-service-groups{}", p.replace(":id", "missing"));
                let direct = http(&s.url, m, &path, "{}");
                let forward = http(&follower.url, m, &path, "{}");
                assert_eq!(direct, forward);
                observations.push(
                    json!({"stage":"follower","method":m,"response":normalized(forward,&endpoint)}),
                );
            }
            follower.stop().unwrap();
            s.stop().unwrap();
            reports.push(json!({"api":api,"rust":is_rust,"observations":observations,"physical":physical,"wal_before":before_restart}));
            compared.push(observations);
            std::fs::remove_file(config_path).unwrap();
            std::fs::write(
                report_path("/tmp/sa012-matrix.json"),
                serde_json::to_vec_pretty(&reports).unwrap(),
            )
            .unwrap();
        }
        std::fs::write(
            report_path("/tmp/sa012-matrix.json"),
            serde_json::to_vec_pretty(&reports).unwrap(),
        )
        .unwrap();
        let pair = &reports[reports.len() - 2..];
        let signature = |report: &Value| {
            json!(report["physical"].as_array().unwrap().iter().map(|p|json!({"before":p["before"].as_array().unwrap().iter().map(|r|json!({"key":r["key"],"bytes":r["bytes"]})).collect::<Vec<_>>(),"after":p["after"].as_array().unwrap().iter().map(|r|json!({"key":r["key"],"bytes":r["bytes"]})).collect::<Vec<_>>() })).collect::<Vec<_>>())
        };
        assert_eq!(
            signature(&pair[0]),
            signature(&pair[1]),
            "persistent bytes api={api}"
        );
        for (a, b) in compared[0].iter().zip(&compared[1]) {
            assert_eq!(a, b, "HTTP differential api={api}");
        }
        assert_eq!(compared[0].len(), compared[1].len());
    }
}
