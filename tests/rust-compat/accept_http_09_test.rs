// Copyright 2026 AsterSQL.
use crate::{harness::Server, http_regression::report_path};
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_009_behavior() {
    let rust = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut s = Server::start_rust(&rust).unwrap();
    s.ready().unwrap();
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "5",
            "-w",
            "\n%{http_code}",
            &format!("{}/pd/api/v2/keyspaces", s.url),
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
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_proto::{
    keyspacepb::{self, keyspace_client::KeyspaceClient},
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_storage_domain::{Consistency, MetaStore, Read};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
fn http(url: &str, method: &str, path: &str, body: &str) -> Value {
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "40",
            "-X",
            method,
            "--data-binary",
            body,
            "-H",
            "Content-Type: application/json",
            "-D",
            "-",
            "-w",
            "\n%{http_code}",
            &format!("{url}/pd/api/{path}"),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let raw = String::from_utf8(out.stdout).unwrap();
    let (headers, body) = raw.split_once("\r\n\r\n").unwrap();
    let (body, status) = body.rsplit_once('\n').unwrap();
    let ct = headers.lines().find_map(|l| {
        l.to_lowercase()
            .strip_prefix("content-type:")
            .map(|s| s.trim().to_owned())
    });
    let location = headers.lines().find_map(|l| {
        l.split_once(':')
            .filter(|(k, _)| k.eq_ignore_ascii_case("location"))
            .map(|(_, v)| v.trim().to_owned())
    });
    json!({"status":status.parse::<u16>().unwrap(),"content_type":ct,"location":location,"body":serde_json::from_str::<Value>(body).unwrap_or(json!(body))})
}
fn normalize(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.remove("created_at");
            m.remove("state_changed_at");
            for value in m.values_mut() {
                normalize(value)
            }
        }
        Value::Array(a) => {
            for value in a {
                normalize(value)
            }
        }
        _ => {}
    }
}
async fn rows(store: &dyn MetaStore, p: &ClusterPaths) -> Value {
    let s = store
        .read(Read::prefix(
            p.keyspace_meta_prefix().as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    json!({"revision":s.revision,"entries":s.entries.iter().map(|e|json!({"key":String::from_utf8_lossy(&e.key),"bytes":e.value,"create_revision":e.create_revision,"mod_revision":e.mod_revision})).collect::<Vec<_>>()})
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_009_dual_matrix() {
    assert_eq!(std::env::var("PD_COMPAT_MODE").as_deref(), Ok("dual"));
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut reports = vec![];
    for api in [false, true] {
        let mut compared = vec![];
        for is_rust in [false, true] {
            let config_path = std::env::temp_dir().join(format!(
                "sa009-config-{}-{is_rust}-{api}.toml",
                std::process::id()
            ));
            std::fs::write(&config_path, "[keyspace]\nwait-region-split=false\n").unwrap();
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
            let mut pd = PdClient::connect(s.url.clone()).await.unwrap();
            let id = pd
                .get_members(pdpb::GetMembersRequest::default())
                .await
                .unwrap()
                .into_inner()
                .header
                .unwrap()
                .cluster_id;
            let header = Some(pdpb::RequestHeader {
                cluster_id: id,
                ..Default::default()
            });
            let store: Arc<dyn MetaStore> = Arc::new(
                astersql_pd_storage_kv::EtcdMetaStore::connect(
                    &[s.metadata_url().to_string()],
                    Duration::from_secs(2),
                )
                .await
                .unwrap(),
            );
            let p = ClusterPaths::new(id);
            let mut all = vec![];
            let mut physical = vec![];
            all.push(http(&s.url, "GET", "v2/keyspaces", ""));
            let r = pd
                .bootstrap(pdpb::BootstrapRequest {
                    header: header.clone(),
                    store: Some(metapb::Store {
                        id: 1,
                        address: "127.0.0.1:20160".into(),
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
                })
                .await
                .unwrap()
                .into_inner();
            assert!(r.header.unwrap().error.is_none());
            for (method, path, body) in [
                ("GET", "v2/keyspaces", ""),
                ("POST", "v2/keyspaces", r#"{"name":"allocated_09"}"#),
                (
                    "POST",
                    "v2/keyspaces/id",
                    r#"{"id":10,"name":"accept_09","config":{"x":"before"}}"#,
                ),
                ("GET", "v2/keyspaces/accept_09", ""),
                ("GET", "v2/keyspaces/id/10", ""),
                (
                    "GET",
                    "v2/keyspaces/accept_09?force_refresh_group_id=true",
                    "",
                ),
                (
                    "PATCH",
                    "v2/keyspaces/accept_09/config",
                    r#"{"config":{"x":"after","empty":""},"preconditions":{"x":"before","absent":null}}"#,
                ),
                (
                    "PATCH",
                    "v2/keyspaces/accept_09/config",
                    r#"{"config":{"x":"bad"},"preconditions":{"x":"before"}}"#,
                ),
                (
                    "PATCH",
                    "v2/keyspaces/accept_09/config",
                    r#"{"config":{"empty":null}}"#,
                ),
                (
                    "PUT",
                    "v2/keyspaces/accept_09/state",
                    r#"{"state":"disabled"}"#,
                ),
                (
                    "PUT",
                    "v2/keyspaces/accept_09/state",
                    r#"{"state":"ARCHIVED"}"#,
                ),
                (
                    "PUT",
                    "v2/keyspaces/accept_09/state",
                    r#"{"state":"ENABLED"}"#,
                ),
                ("GET", "v2/keyspaces/id", ""),
                ("GET", "v2/keyspaces/missing", ""),
                ("GET", "v2/keyspaces/id/no", ""),
                ("GET", "v2/keyspaces/id/4294967306", ""),
                ("GET", "v2/keyspaces?page_token=10&limit=1", ""),
                ("GET", "v2/keyspaces?limit=1", ""),
                ("GET", "v2/keyspaces?page_token=99999", ""),
                ("GET", "v2/keyspaces?page_token=-1", ""),
                ("GET", "v2/keyspaces?limit=no", ""),
                ("POST", "v2/keyspaces/id", r#"{"id":10,"name":"duplicate"}"#),
                ("POST", "v2/keyspaces/id", r#"{"name":"no_id"}"#),
                (
                    "POST",
                    "v2/keyspaces/id",
                    r#"{"id":11,"name":"invalid name"}"#,
                ),
                ("POST", "v2/keyspaces/id", r#"{"id":11,"name":"accept_09"}"#),
                (
                    "PATCH",
                    "v2/keyspaces/accept_09/config",
                    r#"{"config":{"region_bound_type":"raw"}}"#,
                ),
                (
                    "PATCH",
                    "v2/keyspaces/accept_09/config",
                    r#"{"config":{"gc_management_type":"keyspace_level"}}"#,
                ),
                (
                    "PATCH",
                    "v2/keyspaces/accept_09/config",
                    r#"{"config":{"meta_service_group_id":"missing"}}"#,
                ),
                ("PUT", "v2/keyspaces/accept_09/state", r#"{"state":"nope"}"#),
                (
                    "PUT",
                    "v2/keyspaces/DEFAULT/state",
                    r#"{"state":"DISABLED"}"#,
                ),
                ("POST", "v2/keyspaces/id", "{"),
                ("POST", "v2/keyspaces/id", r#"{"id":12,"name":4}"#),
                ("GET", "v2/keyspaces/", ""),
                ("POST", "v2/keyspaces/id/", "{}"),
            ] {
                let before = rows(store.as_ref(), &p).await;
                let r = http(&s.url, method, path, body);
                let after = rows(store.as_ref(), &p).await;
                if r["status"].as_u64().unwrap() >= 400 {
                    assert_eq!(
                        before["entries"], after["entries"],
                        "failure changed metadata: {method} {path} {r}"
                    );
                }
                println!(
                    "sa009 {} {method} {path}: {r}",
                    if is_rust { "rust" } else { "go" }
                );
                all.push(json!({"method":method,"path":path,"input":body,"response":r}));
                physical.push(json!({"before":before,"after":after}));
                std::fs::write(
                    report_path(format!("/tmp/sa009-raw-{is_rust}.json")),
                    serde_json::to_vec_pretty(&json!({"http":all,"physical":physical})).unwrap(),
                )
                .unwrap();
            }
            // Derive every method/path from the read-only Go registration source.
            let mut registered = 0;
            for (method, suffix) in registered_routes() {
                let suffix = suffix.replace(":name", "accept_09").replace(":id", "10");
                let path = format!("v2/keyspaces{suffix}");
                let input = if method == "POST" {
                    if suffix == "/id" {
                        r#"{"id":20,"name":"inventory_id09"}"#
                    } else {
                        r#"{"name":"inventory_auto09"}"#
                    }
                } else {
                    "{}"
                };
                let response = http(&s.url, method, &path, input);
                assert_ne!(
                    response["status"], 503,
                    "registered keyspace route still denied"
                );
                all.push(json!({"registered_method":method,"path":path,"input":input,"response":response}));
                registered += 1;
            }
            assert!(registered > 0);
            let mut ks = KeyspaceClient::connect(s.url.clone()).await.unwrap();
            let rpc = ks
                .load_keyspace_by_id(keyspacepb::LoadKeyspaceByIdRequest {
                    header: header.clone(),
                    keyspace: Some(keyspacepb::load_keyspace_by_id_request::Keyspace::Id(10)),
                })
                .await
                .unwrap()
                .into_inner();
            assert!(rpc.header.as_ref().unwrap().error.is_none());
            assert_eq!(
                rpc.keyspace
                    .as_ref()
                    .unwrap()
                    .config
                    .get("x")
                    .map(String::as_str),
                Some("after")
            );
            let mut watch = ks
                .watch_keyspaces(keyspacepb::WatchKeyspacesRequest {
                    header: header.clone(),
                })
                .await
                .unwrap()
                .into_inner();
            let initial = tokio::time::timeout(Duration::from_secs(5), watch.message())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert!(
                initial
                    .keyspaces
                    .iter()
                    .any(|m| m.name == "accept_09" && m.state == 2)
            );
            let changed = ks
                .update_keyspace_state(keyspacepb::UpdateKeyspaceStateRequest {
                    header: header.clone(),
                    keyspace: Some(keyspacepb::update_keyspace_state_request::Keyspace::Id(10)),
                    state: 3,
                })
                .await
                .unwrap()
                .into_inner();
            assert!(changed.header.unwrap().error.is_none());
            let event = tokio::time::timeout(Duration::from_secs(5), watch.message())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert!(
                event
                    .keyspaces
                    .iter()
                    .any(|m| m.name == "accept_09" && m.state == 3)
            );
            all.push(http(&s.url, "GET", "v2/keyspaces/accept_09", ""));
            drop(watch);
            let canonical = astersql_pd_keyspace::KeyspaceManager::new(
                astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone())
                    .keyspace_storage(p.clone()),
            )
            .load_range(0, 0)
            .await
            .unwrap()
            .iter()
            .map(crate::keyspace_create_oracle_test::dto)
            .collect::<Vec<_>>();
            all.push(json!({"persisted":canonical}));
            let before = rows(store.as_ref(), &p).await;
            let labels_before = store
                .read(Read::prefix(
                    p.region_label_path_prefix().as_bytes(),
                    Consistency::Linearizable,
                ))
                .await
                .unwrap();
            s.stop().unwrap();
            if is_rust {
                s.restart_rust(&rust).unwrap();
            } else {
                s.restart_with_args(&go, &[]).unwrap();
            }
            s.ready().unwrap();
            let after = rows(store.as_ref(), &p).await;
            assert_eq!(
                labels_before.entries,
                store
                    .read(Read::prefix(
                        p.region_label_path_prefix().as_bytes(),
                        Consistency::Linearizable
                    ))
                    .await
                    .unwrap()
                    .entries,
                "ordinary restart must preserve label bytes and revisions"
            );
            assert_eq!(before["entries"], after["entries"]);
            all.push(http(&s.url, "GET", "v2/keyspaces/id/10", ""));
            let args = if is_rust {
                vec![
                    format!("--etcd-endpoints={}", s.metadata_url()),
                    "--name=follower09".into(),
                ]
            } else {
                vec![
                    format!("--join={}", s.url),
                    "--initial-cluster=".into(),
                    "--name=follower09".into(),
                ]
            };
            let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
            let mut follower =
                Server::start_with_environment(if is_rust { &rust } else { &go }, &refs, &env)
                    .unwrap();
            follower.ready().unwrap();
            let before_forward = rows(store.as_ref(), &p).await;
            for (method, suffix) in registered_routes() {
                let suffix = suffix.replace(":name", "accept_09").replace(":id", "10");
                let path = format!("v2/keyspaces{suffix}");
                let input = if method == "POST" {
                    r#"{"id":10,"name":"accept_09"}"#
                } else if method == "PUT" {
                    r#"{"state":"TOMBSTONE"}"#
                } else {
                    "{}"
                };
                let direct = http(&s.url, method, &path, input);
                let forwarded = http(&follower.url, method, &path, input);
                assert_eq!(direct, forwarded, "follower route {method} {path}");
                all.push(json!({"follower_method":method,"path":path,"response":forwarded}));
            }
            let after_forward = rows(store.as_ref(), &p).await;
            // Go deliberately commits an idempotent state PUT: bytes/timestamps
            // retain their meaning while the target's physical modification revision advances.
            let decoder = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone())
                .keyspace_storage(p.clone());
            for (before, after) in before_forward["entries"]
                .as_array()
                .unwrap()
                .iter()
                .zip(after_forward["entries"].as_array().unwrap())
            {
                for field in ["key", "create_revision"] {
                    assert_eq!(before[field], after[field]);
                }
                if before["key"].as_str().unwrap().ends_with("00000010") {
                    let bytes = |row: &Value| {
                        row["bytes"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| v.as_u64().unwrap() as u8)
                            .collect::<Vec<_>>()
                    };
                    assert_eq!(
                        decoder.decode_persisted(&bytes(before)).unwrap(),
                        decoder.decode_persisted(&bytes(after)).unwrap()
                    );
                    assert!(
                        after["mod_revision"].as_i64().unwrap()
                            > before["mod_revision"].as_i64().unwrap()
                    );
                } else {
                    assert_eq!(before["bytes"], after["bytes"]);
                    assert_eq!(before["mod_revision"], after["mod_revision"]);
                }
            }
            physical.push(json!({"follower_before":before_forward,"follower_after":after_forward}));
            let mut follower_client = KeyspaceClient::connect(follower.url.clone()).await.unwrap();
            let denied = follower_client
                .load_keyspace_by_id(keyspacepb::LoadKeyspaceByIdRequest {
                    header: header.clone(),
                    keyspace: Some(keyspacepb::load_keyspace_by_id_request::Keyspace::Id(10)),
                })
                .await
                .unwrap_err();
            assert_eq!(denied.code(), tonic::Code::Unavailable);
            assert_eq!(denied.message(), "not leader");
            follower.stop().unwrap();
            s.stop().unwrap();
            std::fs::remove_file(&config_path).unwrap();
            reports.push(json!({"rust":is_rust,"api":api,"http":all,"rpc":format!("{rpc:?}"),"before_restart":before,"after_restart":after,"physical":physical}));
            let mut normalized = json!(all);
            normalize(&mut normalized);
            compared.push(normalized);
        }
        assert_eq!(compared[0], compared[1]);
    }
    std::fs::write(
        std::env::var_os("SA009_RECEIPT")
            .map_or_else(|| report_path("/tmp/sa009-matrix.json"), Into::into),
        serde_json::to_vec_pretty(&reports).unwrap(),
    )
    .unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_009_split_rollback_and_cancel() {
    use std::io::Write;
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut reports = vec![];
    for is_rust in [false, true] {
        let path =
            std::env::temp_dir().join(format!("sa009-fault-{}-{is_rust}.toml", std::process::id()));
        std::fs::write(&path,"[keyspace]\nwait-region-split=true\nwait-region-split-timeout=\"100ms\"\ncheck-region-split-interval=\"10ms\"\n").unwrap();
        let arg = format!("--config={}", path.display());
        let mut s = if is_rust {
            Server::start_rust_with_args(&rust, &[&arg])
        } else {
            Server::start_with_args(&go, &[&arg])
        }
        .unwrap();
        s.ready().unwrap();
        let mut pd = PdClient::connect(s.url.clone()).await.unwrap();
        let id = pd
            .get_members(pdpb::GetMembersRequest::default())
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .cluster_id;
        let r = pd
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
            })
            .await
            .unwrap()
            .into_inner();
        assert!(r.header.unwrap().error.is_none());
        assert_eq!(http(&s.url, "GET", "v2/keyspaces", "")["status"], 200);
        let store = astersql_pd_storage_kv::EtcdMetaStore::connect(
            &[s.metadata_url().to_string()],
            Duration::from_secs(2),
        )
        .await
        .unwrap();
        let p = ClusterPaths::new(id);
        let before = rows(&store, &p).await;
        let result = http(
            &s.url,
            "POST",
            "v2/keyspaces/id",
            r#"{"id":100,"name":"rollback09"}"#,
        );
        assert_eq!(result["status"], 500);
        assert_eq!(
            result["body"],
            "[PD:keyspace:ErrRegionSplitTimeout]region split timeout"
        );
        let after = rows(&store, &p).await;
        assert_eq!(before["entries"], after["entries"]);
        let missing = store
            .read(Read::range(
                p.keyspace_id_path("rollback09").as_bytes(),
                b"",
                Consistency::Linearizable,
            ))
            .await
            .unwrap();
        assert!(missing.entries.is_empty());
        let rules = http(&s.url, "GET", "v1/config/region-label/rules", "");
        assert!(
            !rules["body"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["id"] == "keyspaces/100")
        );
        let mut tcp = std::net::TcpStream::connect(s.url.trim_start_matches("http://")).unwrap();
        tcp.write_all(b"POST /pd/api/v2/keyspaces/id HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: 999999\r\n\r\n{\"id\":101").unwrap();
        drop(tcp);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(after["entries"], rows(&store, &p).await["entries"]);
        let mut tcp = std::net::TcpStream::connect(s.url.trim_start_matches("http://")).unwrap();
        let input = r#"{"id":102,"name":"cancel_complete09"}"#;
        tcp.write_all(format!("POST /pd/api/v2/keyspaces/id HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{input}",input.len()).as_bytes()).unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        drop(tcp);
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(
            after["entries"],
            rows(&store, &p).await["entries"],
            "complete-body client cancellation must not strand metadata"
        );
        reports.push(
            json!({"rust":is_rust,"response":result,"before":before,"after":after,"rules":rules}),
        );
        s.stop().unwrap();
        std::fs::remove_file(path).unwrap();
    }
    assert_eq!(reports[0]["response"], reports[1]["response"]);
    std::fs::write(
        report_path("/tmp/sa009-fault.json"),
        serde_json::to_vec_pretty(&reports).unwrap(),
    )
    .unwrap();
}

fn registered_routes() -> Vec<(&'static str, &'static str)> {
    include_str!("../../server/apiv2/handlers/keyspace.go")
        .lines()
        .filter_map(|line| {
            let (method, rest) = line.trim().strip_prefix("router.")?.split_once("(\"")?;
            if !["GET", "POST", "PUT", "PATCH", "DELETE"].contains(&method) {
                return None;
            }
            Some((method, rest.split('"').next()?))
        })
        .collect()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_009_host_preallocation() {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut results = vec![];
    for is_rust in [false, true] {
        let path = std::env::temp_dir().join(format!(
            "sa009-prealloc-{}-{is_rust}.toml",
            std::process::id()
        ));
        std::fs::write(
            &path,
            "[keyspace]\nwait-region-split=false\npre-alloc=[\"preallocated09\"]\n",
        )
        .unwrap();
        let arg = format!("--config={}", path.display());
        let env = vec![("PD_SERVICE_MODE".into(), "api".into())];
        let mut s = if is_rust {
            Server::start_rust_with_environment(&rust, &[&arg], &env)
        } else {
            Server::start_with_environment(&go, &[&arg], &env)
        }
        .unwrap();
        s.ready().unwrap();
        let mut pd = PdClient::connect(s.url.clone()).await.unwrap();
        let id = pd
            .get_members(pdpb::GetMembersRequest::default())
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .cluster_id;
        let result = pd
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
            })
            .await
            .unwrap()
            .into_inner();
        assert!(result.header.unwrap().error.is_none());
        let mut found = None;
        for _ in 0..50 {
            let r = http(&s.url, "GET", "v2/keyspaces/preallocated09", "");
            if r["status"] == 200 && r["body"]["state"] == "ENABLED" {
                found = Some(r);
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let value = found
            .unwrap_or_else(|| panic!("host must drive configured preallocation, rust={is_rust}"));
        assert_eq!(value["body"]["id"], 1);
        let store = astersql_pd_storage_kv::EtcdMetaStore::connect(
            &[s.metadata_url().to_string()],
            Duration::from_secs(2),
        )
        .await
        .unwrap();
        let p = ClusterPaths::new(id);
        let before = rows(&store, &p).await;
        s.stop().unwrap();
        if is_rust {
            s.restart_rust(&rust).unwrap();
        } else {
            s.restart_with_args(&go, &[]).unwrap();
        }
        s.ready().unwrap();
        assert_eq!(before["entries"], rows(&store, &p).await["entries"]);
        let mut normalized = value.clone();
        normalize(&mut normalized);
        results.push(normalized);
        s.stop().unwrap();
        std::fs::remove_file(path).unwrap();
    }
    assert_eq!(results[0], results[1]);
    std::fs::write(
        report_path("/tmp/sa009-preallocation.json"),
        serde_json::to_vec_pretty(&results).unwrap(),
    )
    .unwrap();
}
