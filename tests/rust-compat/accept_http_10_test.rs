// Copyright 2026 AsterSQL.
use crate::harness::Server;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_010_behavior() {
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
            &format!("{}/pd/api/v2/tso/keyspace-groups", s.url),
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
async fn rows(store: &dyn MetaStore, p: &ClusterPaths) -> Value {
    let s = store
        .read(Read::prefix(
            p.keyspace_group_id_prefix().as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    json!(s.entries.iter().map(|e|json!({"key":String::from_utf8_lossy(&e.key),"bytes":e.value,"create_revision":e.create_revision,"mod_revision":e.mod_revision})).collect::<Vec<_>>())
}
fn routes() -> Vec<(&'static str, &'static str)> {
    include_str!("../../server/apiv2/handlers/tso_keyspace_group.go")
        .lines()
        .filter_map(|l| {
            let (m, r) = l.trim().strip_prefix("router.")?.split_once("(\"")?;
            let p = r.split('"').next()?;
            if p.contains("split") || p.contains("merge") || p.ends_with("keyspaces") {
                None
            } else {
                Some((m, p))
            }
        })
        .collect()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_010_dual_matrix() {
    assert_eq!(std::env::var("PD_COMPAT_MODE").as_deref(), Ok("dual"));
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut reports = vec![];
    for api in [false, true] {
        let listeners = if api {
            (0..3)
                .map(|_| std::net::TcpListener::bind("127.0.0.1:0").unwrap())
                .collect::<Vec<_>>()
        } else {
            vec![]
        };
        let nodes = listeners
            .iter()
            .map(|l| format!("http://{}", l.local_addr().unwrap()))
            .collect::<Vec<_>>();
        let mut service_compared = Vec::new();
        let mut compared = vec![];
        for is_rust in [false, true] {
            let env = if api {
                vec![("PD_SERVICE_MODE".into(), "api".into())]
            } else {
                vec![]
            };
            let mut s = if is_rust {
                Server::start_rust_with_environment(&rust, &[], &env)
            } else {
                Server::start_with_environment(&go, &[], &env)
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
            let store: Arc<dyn MetaStore> = Arc::new(
                astersql_pd_storage_kv::EtcdMetaStore::connect(
                    &[s.metadata_url().into()],
                    Duration::from_secs(2),
                )
                .await
                .unwrap(),
            );
            let p = ClusterPaths::new(id);
            let mut all = vec![http(&s.url, "GET", "v2/tso/keyspace-groups", "")];
            let mut physical = vec![];
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
            let mut cases = vec![
                (
                    "POST",
                    "",
                    r#"{"keyspace-groups":[{"id":10,"keyspaces":[11,12]},{"id":11,"user-kind":"standard"},{"id":12,"user-kind":"enterprise"}]}"#,
                ),
                ("GET", "", ""),
                ("GET", "?limit=2", ""),
                ("GET", "?page_token=11", ""),
                ("GET", "?page_token=100", ""),
                ("GET", "?hide_keyspaces=TRUE", ""),
                ("GET", "?state=split", ""),
                ("GET", "?state=bad", ""),
                ("GET", "?state=", ""),
                ("GET", "/10", ""),
                ("GET", "/10?hide_keyspaces=true", ""),
                ("GET", "/10?fields=primary", ""),
                ("GET", "/999", ""),
                ("GET", "/4294967306", ""),
                ("POST", "", r#"{"keyspace-groups":[{"id":13},{"id":10}]}"#),
                ("GET", "/13", ""),
                ("POST", "", r#"{"keyspace-groups":[{"id":5000}]}"#),
                (
                    "POST",
                    "",
                    r#"{"keyspace-groups":[{"id":15,"user-kind":"BAD"}]}"#,
                ),
                ("POST", "", r#"{"keyspace-groups":[{"id":"bad"}]}"#),
                ("GET", "/bad", ""),
                ("GET", "/4097", ""),
                ("GET", "?page_token=bad", ""),
                ("GET", "?limit=bad", ""),
                ("POST", "/10/alloc", r#"{"replica":2}"#),
                ("PATCH", "/10", r#"{"nodes":["http://unregistered"]}"#),
                ("DELETE", "/0", ""),
                ("DELETE", "/999", ""),
                ("DELETE", "/11", ""),
                ("GET", "/11", ""),
                ("POST", "", "{"),
                ("POST", "", "[]"),
                ("POST", "", r#"{"keyspace-groups":2}"#),
                ("POST", "/10/alloc", r#"{"replica":"2"}"#),
                ("PATCH", "/10", r#"{"nodes":42}"#),
                ("GET", "/", ""),
                ("POST", "/", "{}"),
                ("GET", "/10/", ""),
                ("POST", "/10/alloc/", "{}"),
            ];
            for (method, suffix, input) in cases.drain(..) {
                let before = rows(store.as_ref(), &p).await;
                let path = format!("v2/tso/keyspace-groups{suffix}");
                let response = http(&s.url, method, &path, input);
                let after = rows(store.as_ref(), &p).await;
                if response["status"].as_u64().unwrap() >= 400 {
                    assert_eq!(before, after, "failure mutated {method} {path}");
                }
                all.push(
                    json!({"method":method,"suffix":suffix,"input":input,"response":response}),
                );
                physical.push(json!({"before":before,"after":after}));
                std::fs::write(
                    format!("/tmp/sa010-raw-{is_rust}-{api}.json"),
                    serde_json::to_vec_pretty(&json!({"http":all,"physical":physical})).unwrap(),
                )
                .unwrap();
            }
            if api {
                use astersql_pd_storage_domain::Mutation;
                for node in &nodes {
                    store
                        .transact(
                            vec![],
                            vec![Mutation::put(
                                p.registry_path("tso", node).as_bytes(),
                                &serde_json::to_vec(&json!({"service-addr":node})).unwrap(),
                            )],
                        )
                        .await
                        .unwrap();
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
                let input = serde_json::to_string(&json!({"nodes":nodes[..2]})).unwrap();
                let set_nodes = http(&s.url, "PATCH", "v2/tso/keyspace-groups/10", &input);
                assert_eq!(set_nodes["status"], 200);
                let allocated = http(
                    &s.url,
                    "POST",
                    "v2/tso/keyspace-groups/10/alloc",
                    r#"{"replica":3}"#,
                );
                assert_eq!(allocated["status"], 200, "{allocated}");
                let members = allocated["body"].as_array().unwrap();
                assert_eq!(members.len(), 3);
                for n in &nodes {
                    assert!(members.iter().any(|m| m["address"] == *n));
                }
                let escaped = nodes[0].replace(":", "%3A").replace("/", "%2F");
                let priority = http(
                    &s.url,
                    "PATCH",
                    &format!("v2/tso/keyspace-groups/10/{escaped}"),
                    r#"{"priority":-7}"#,
                );
                assert_eq!(priority["status"], 200, "{priority}");
                use prost::Message;
                let election = p.election_path(Some(&astersql_pd_keypath::MsParam {
                    service_name: "tso".into(),
                    group_id: 10,
                }));
                let participant = astersql_pd_proto::tsopb::Participant {
                    name: "http://primary-10".into(),
                    ..Default::default()
                }
                .encode_to_vec();
                store
                    .transact(
                        vec![],
                        vec![Mutation::put(election.as_bytes(), &participant)],
                    )
                    .await
                    .unwrap();
                let primary = http(
                    &s.url,
                    "GET",
                    "v2/tso/keyspace-groups/10?fields=primary",
                    "",
                );
                assert_eq!(primary["body"], json!({"id":10,"primary":"http://primary"}));
                all.push(primary);
                let dto = http(&s.url, "GET", "v2/tso/keyspace-groups/10", "");
                assert_eq!(dto["body"]["members"].as_array().unwrap().len(), 3);
                assert_eq!(
                    dto["body"]["members"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|m| m["address"] == nodes[0])
                        .unwrap()["priority"],
                    -7
                );
                // Go iterates an existing-members map; array order is nondeterministic.
                let mut semantic = json!({"set_nodes":set_nodes,"allocated":allocated,"priority":priority,"directory":dto});
                semantic["allocated"]["body"]
                    .as_array_mut()
                    .unwrap()
                    .sort_by_key(|m| m["address"].as_str().unwrap().to_owned());
                semantic["directory"]["body"]["members"]
                    .as_array_mut()
                    .unwrap()
                    .sort_by_key(|m| m["address"].as_str().unwrap().to_owned());
                service_compared.push(semantic);
                // Compare independent physical persisted DTO with the HTTP response, then reconstruct the user-kind index.
                let manager =
                    astersql_pd_keyspace::KeyspaceGroupManager::new(store.clone(), p.clone());
                manager.bootstrap().await.unwrap();
                assert_eq!(
                    serde_json::to_value(manager.get_group(10).await.unwrap()).unwrap(),
                    dto["body"]
                );
                assert_eq!(
                    manager
                        .indexed_group_ids(astersql_pd_storage_domain::UserKind::ENTERPRISE)
                        .await
                        .unwrap(),
                    vec![12]
                );
                // Real host reconciliation must also populate groups not touched by HTTP allocation.
                for group_id in [0, 12] {
                    let mut found = false;
                    for _ in 0..60 {
                        let current = http(
                            &s.url,
                            "GET",
                            &format!("v2/tso/keyspace-groups/{group_id}"),
                            "",
                        );
                        if current["body"]["members"]
                            .as_array()
                            .is_some_and(|m| m.len() == 2)
                        {
                            found = true;
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    assert!(
                        found,
                        "host did not reconcile group {group_id}, rust={is_rust}"
                    );
                }
                // The service-side watcher owns real per-group election leases and Oracles.
                // No RPC transport or fixed responder is substituted for those resources.
                store
                    .transact(
                        vec![],
                        vec![Mutation::Delete {
                            key: election.into_bytes(),
                        }],
                    )
                    .await
                    .unwrap();
                let mut service_watches = Vec::new();
                for node in &nodes {
                    let name = node.clone();
                    let cfg = astersql_pd_tso::GroupWatchConfig {
                        store: store.clone(),
                        runtime: tokio::runtime::Handle::current(),
                        cancel: tokio_util::sync::CancellationToken::new(),
                        cluster_id: id,
                        address: node.clone(),
                        bootstrap_keyspace: 0,
                        participant: Arc::new(move |group| {
                            astersql_pd_proto::tsopb::Participant {
                                name: format!("{name}-{group:05}"),
                                listen_urls: vec![name.clone()],
                                ..Default::default()
                            }
                            .encode_to_vec()
                        }),
                        clock: Arc::new(astersql_pd_tso::SystemClock),
                        lease_seconds: 3,
                        save_interval_millis: 1000,
                    };
                    service_watches.push(
                        astersql_pd_tso::GroupAllocatorWatch::load(cfg)
                            .await
                            .unwrap(),
                    );
                }
                assert!(service_watches.iter().all(|w| w.allocator(10).is_some()));
                let mut issued = None;
                for _ in 0..100 {
                    for (index, w) in service_watches.iter().enumerate() {
                        if let Ok(t) = w.allocator(10).unwrap().allocate(1).await {
                            issued = Some((index, t));
                            break;
                        }
                    }
                    if issued.is_some() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                let (issuer, stamp) = issued.expect("real group must issue a fenced timestamp");
                let serving = http(
                    &s.url,
                    "GET",
                    "v2/tso/keyspace-groups/10?fields=primary",
                    "",
                );
                assert_eq!(serving["body"]["primary"], nodes[issuer], "{serving}");
                let raw_service = json!({"nodes":nodes,"set_nodes_input":input,"set_nodes":set_nodes,"allocated":allocated,"priority":priority,"directory":dto,"physical":rows(store.as_ref(),&p).await,"primary":serving,"timestamp":format!("{stamp:?}"),"assigned":service_watches.iter().map(|w|w.assigned_ids()).collect::<Vec<_>>()});
                std::fs::write(
                    format!("/tmp/sa010-service-{is_rust}.json"),
                    serde_json::to_vec_pretty(&raw_service).unwrap(),
                )
                .unwrap();
                for node in &nodes {
                    store
                        .transact(
                            vec![],
                            vec![Mutation::Delete {
                                key: p.registry_path("tso", node).into_bytes(),
                            }],
                        )
                        .await
                        .unwrap();
                }
                let before = rows(store.as_ref(), &p).await;
                let no_nodes = http(&s.url, "PATCH", "v2/tso/keyspace-groups/10", &input);
                assert_eq!(no_nodes["status"], 400);
                assert_eq!(before, rows(store.as_ref(), &p).await);
                // Clear candidates through the public API after discovery removal, retaining no process-specific addresses.
                for group_id in [0, 10, 12] {
                    assert_eq!(
                        http(
                            &s.url,
                            "PATCH",
                            &format!("v2/tso/keyspace-groups/{group_id}"),
                            r#"{"nodes":[]}"#
                        )["status"],
                        200
                    );
                }
                for w in &mut service_watches {
                    w.reload().await.unwrap();
                    assert!(w.allocator(10).is_none());
                    assert!(w.allocator(12).is_none());
                }
                for w in service_watches {
                    w.shutdown().await.unwrap();
                }
            }
            use std::io::Write;
            let before_cancel = rows(store.as_ref(), &p).await;
            let mut tcp =
                std::net::TcpStream::connect(s.url.trim_start_matches("http://")).unwrap();
            tcp.write_all(b"POST /pd/api/v2/tso/keyspace-groups HTTP/1.1\r\nHost: localhost\r\nContent-Length: 999999\r\n\r\n{\"keyspace-groups\":[{\"id\":100").unwrap();
            drop(tcp);
            tokio::time::sleep(Duration::from_millis(100)).await;
            assert_eq!(before_cancel, rows(store.as_ref(), &p).await);
            let before = rows(store.as_ref(), &p).await;
            s.stop().unwrap();
            if is_rust {
                s.restart_rust(&rust).unwrap()
            } else {
                s.restart_with_args(&go, &[]).unwrap()
            };
            s.ready().unwrap();
            assert_eq!(
                before,
                rows(store.as_ref(), &p).await,
                "WAL restart changed bytes/revisions"
            );
            all.push(http(&s.url, "GET", "v2/tso/keyspace-groups", ""));
            let args = if is_rust {
                vec![
                    format!("--etcd-endpoints={}", s.metadata_url()),
                    "--name=follower10".into(),
                ]
            } else {
                vec![
                    format!("--join={}", s.url),
                    "--initial-cluster=".into(),
                    "--name=follower10".into(),
                ]
            };
            let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
            let mut follower =
                Server::start_with_environment(if is_rust { &rust } else { &go }, &refs, &env)
                    .unwrap();
            follower.ready().unwrap();
            for (method, suffix) in routes() {
                let suffix = suffix.replace(":id", "999").replace("*node", "unknown");
                let path = format!("v2/tso/keyspace-groups{suffix}");
                let direct = http(&s.url, method, &path, "{}");
                let forwarded = http(&follower.url, method, &path, "{}");
                assert_eq!(direct, forwarded, "forward {method} {path}");
                all.push(json!({"registered_method":method,"suffix":suffix,"response":forwarded}));
            }
            follower.stop().unwrap();
            s.stop().unwrap();
            reports.push(
                json!({"rust":is_rust,"api":api,"http":all,"physical":physical,"restart":before}),
            );
            compared.push(all);
            std::fs::write(
                "/tmp/sa010-progress.json",
                serde_json::to_vec_pretty(&reports).unwrap(),
            )
            .unwrap();
        }
        assert_eq!(compared[0], compared[1], "api={api}");
        if api {
            assert_eq!(service_compared[0], service_compared[1]);
        }
    }
    assert_eq!(routes().len(), 7);
    std::fs::write(
        "/tmp/sa010-matrix.json",
        serde_json::to_vec_pretty(&reports).unwrap(),
    )
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_010_shared_manager_and_priority_error() {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let env = vec![("PD_SERVICE_MODE".into(), "api".into())];
    let mut responses = Vec::new();
    let mut bridges = Vec::new();
    let mut group_results = Vec::new();
    for is_rust in [false, true] {
        let config_path = std::env::temp_dir().join(format!(
            "sa010-bridge-{}-{is_rust}.toml",
            std::process::id()
        ));
        std::fs::write(&config_path, "[keyspace]\nwait-region-split=false\n").unwrap();
        let config_arg = format!("--config={}", config_path.display());
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
        assert_eq!(
            http(
                &s.url,
                "POST",
                "v2/tso/keyspace-groups",
                r#"{"keyspace-groups":[{"id":101,"user-kind":"enterprise"}]}"#
            )["status"],
            200
        );
        let store = astersql_pd_storage_kv::EtcdMetaStore::connect(
            &[s.metadata_url().into()],
            Duration::from_secs(2),
        )
        .await
        .unwrap();
        let p = ClusterPaths::new(id);
        let before = rows(&store, &p).await;
        let response = http(
            &s.url,
            "PATCH",
            "v2/tso/keyspace-groups/101/missing",
            r#"{"priority":-7}"#,
        );
        assert_eq!(before, rows(&store, &p).await);
        responses.push(response);
        let created = http(
            &s.url,
            "POST",
            "v2/keyspaces/id",
            r#"{"id":201,"name":"priority_bridge10","config":{"user_kind":"enterprise"}}"#,
        );
        assert_eq!(created["status"], 200, "{created}");
        assert_eq!(created["body"]["config"]["tso_keyspace_group_id"], "101");
        let mut keyspace =
            astersql_pd_proto::keyspacepb::keyspace_client::KeyspaceClient::connect(s.url.clone())
                .await
                .unwrap();
        let rpc = keyspace
            .load_keyspace_by_id(astersql_pd_proto::keyspacepb::LoadKeyspaceByIdRequest {
                header: Some(pdpb::RequestHeader {
                    cluster_id: id,
                    ..Default::default()
                }),
                keyspace: Some(
                    astersql_pd_proto::keyspacepb::load_keyspace_by_id_request::Keyspace::Id(201),
                ),
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
                .get("tso_keyspace_group_id")
                .map(String::as_str),
            Some("101")
        );
        let group = http(&s.url, "GET", "v2/tso/keyspace-groups/101", "");
        assert_eq!(group["body"]["user-kind"], "enterprise");
        assert_eq!(group["body"]["keyspaces"], json!([201]));
        group_results.push(group.clone());
        bridges.push(json!({"rust":is_rust,"http":created,"rpc":format!("{rpc:?}"),"group":group,"physical":rows(&store,&p).await}));
        std::fs::write(
            "/tmp/sa010-user-kind.json",
            serde_json::to_vec_pretty(&bridges).unwrap(),
        )
        .unwrap();
        s.stop().unwrap();
        std::fs::remove_file(config_path).unwrap();
    }
    std::fs::write(
        "/tmp/sa010-priority-error.json",
        serde_json::to_vec_pretty(&responses).unwrap(),
    )
    .unwrap();
    assert_eq!(responses[0], responses[1]);
    assert_eq!(group_results[0], group_results[1]);
}
