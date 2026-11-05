// Copyright 2026 AsterSQL.
use crate::harness::Server;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_011_behavior() {
    let rust = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut s = Server::start_rust(&rust).unwrap();
    s.ready().unwrap();
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "5",
            "-X",
            "POST",
            "-w",
            "\n%{http_code}",
            &format!("{}/pd/api/v2/tso/keyspace-groups/1/split", s.url),
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
use crate::accept_http_10_test::{http, rows};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_storage_domain::MetaStore;
use serde_json::json;
use std::{sync::Arc, time::Duration};
fn physical_signature(pairs: &[serde_json::Value]) -> serde_json::Value {
    use std::collections::{BTreeMap, BTreeSet};
    json!(pairs.iter().map(|pair| {
        let before=pair["before"].as_array().unwrap();
        let after=pair["after"].as_array().unwrap();
        let identity=|row:&serde_json::Value|row["key"].as_str().unwrap().rsplit('/').next().unwrap().to_owned();
        let previous:BTreeMap<_,_>=before.iter().map(|row|(identity(row),row)).collect();
        let mut changed=BTreeMap::new();
        for row in after {
            let id=identity(row);
            if let Some(old)=previous.get(&id) {
                assert_eq!(old["create_revision"],row["create_revision"]);
                if old["mod_revision"]!=row["mod_revision"] {
                    assert!(row["mod_revision"].as_i64().unwrap()>old["mod_revision"].as_i64().unwrap());
                    changed.insert(id,row["mod_revision"].as_i64().unwrap());
                }
            } else {
                assert_eq!(row["create_revision"],row["mod_revision"]);
                changed.insert(id,row["mod_revision"].as_i64().unwrap());
            }
        }
        let revisions:BTreeSet<_>=changed.values().copied().collect();
        let same_commit:Vec<_>=revisions.iter().map(|revision|changed.iter().filter(|(_,r)|*r==revision).map(|(id,_)|id.clone()).collect::<Vec<_>>()).collect();
        json!({"before":before.iter().map(|r|json!({"id":identity(r),"bytes":r["bytes"]})).collect::<Vec<_>>(),"after":after.iter().map(|r|json!({"id":identity(r),"bytes":r["bytes"]})).collect::<Vec<_>>(),"same_commit":same_commit})
    }).collect::<Vec<_>>())
}
fn routes() -> Vec<(&'static str, &'static str)> {
    include_str!("../../server/apiv2/handlers/tso_keyspace_group.go")
        .lines()
        .filter_map(|l| {
            let (m, r) = l.trim().strip_prefix("router.")?.split_once("(\"")?;
            let p = r.split('"').next()?;
            (p.ends_with("/split") || p.ends_with("/merge") || p.ends_with("/keyspaces"))
                .then_some((m, p))
        })
        .collect()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_011_dual_transitions_and_wal() {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut reports = vec![];
    for api in [false, true] {
        let mut compared = vec![];
        let mut physical_compared = vec![];
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
            let mut observations = vec![];
            for (method, suffix) in routes() {
                let path = format!("v2/tso/keyspace-groups{}", suffix.replace(":id", "1"));
                let response = http(&s.url, method, &path, "{}");
                assert_eq!(response["status"], 500);
                observations.push(json!({"stage":"unbootstrapped","method":method,"path":path,"response":response}));
            }

            let mut physical = vec![];
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
            // Concrete routes are derived from Go registrations, including slash redirects.
            let mut cases = vec![];
            for (m, suffix) in routes() {
                let path = suffix.replace(":id", "999");
                cases.push((m.to_owned(), path.clone(), "{}".to_owned()));
                cases.push((m.to_owned(), format!("{path}/"), "{}".to_owned()));
            }
            for operation in ["split", "merge", "keyspaces"] {
                let method = if operation == "keyspaces" {
                    "DELETE"
                } else {
                    "POST"
                };
                for (group, input) in [
                    ("bad", "{}"),
                    ("4097", "{}"),
                    ("1", "{"),
                    ("1", "[]"),
                    ("1", "null"),
                    (
                        "1",
                        r#"{"new-id":-1,"merge-list":["bad"],"keyspaces":["bad"]}"#,
                    ),
                    ("1", r#"{"new-id":null,"merge-list":null,"keyspaces":null}"#),
                    ("1", r#"{"new-id":3,"new-id":"bad","keyspaces":[101]}"#),
                    (
                        "1",
                        r#"{"keyspaces":"bad","merge-list":"bad","new-id":"bad"}"#,
                    ),
                    ("1", r#"{"keyspaces":[-1],"merge-list":[-1],"new-id":-1}"#),
                ] {
                    cases.push((method.into(), format!("/{group}/{operation}"), input.into()));
                }
            }
            cases.extend([
 ("POST","",r#"{"keyspace-groups":[{"id":10,"keyspaces":[101,102,103],"members":[{"address":"http://a","priority":0},{"address":"http://b","priority":0}]},{"id":12,"keyspaces":[104]}]}"#),
 ("PATCH","/10/http%3A%2F%2Fa",r#"{"priority":7,"priority":null}"#),
 ("PATCH","/10",r#"{"nodes":[null]}"#),
 ("PATCH","/10",r#"{"nodes":[42]}"#),
 ("PATCH","/10/http%3A%2F%2Fa",r#"{"priority":18446744073709551615}"#),
 ("POST","/10/split",r#"{"new-id":4097,"new-id":null,"keyspaces":[999]}"#),
 ("POST","/999/merge",r#"{"merge-list":[999],"merge-all-into-default":true,"merge-all-into-default":null}"#),
 ("POST","/10/split",r#"{"new-id":11}"#),
 ("POST","/10/split",r#"{"new-id":4097,"keyspaces":[101]}"#),
 ("POST","/10/split",r#"{"new-id":11,"start-keyspace-id":103,"end-keyspace-id":102}"#),
 ("POST","/10/split",r#"{"new-id":11,"keyspaces":[999]}"#),
 ("POST","/10/split",r#"{"new-id":11,"keyspaces":[101,101]}"#),
 ("GET","/10",""),("GET","/11",""),
 ("PATCH","/10",r#"{"nodes":[]}"#),
 ("POST","/10/split",r#"{"new-id":13,"keyspaces":[102]}"#),
 ("POST","/10/merge",r#"{"merge-list":[11]}"#),
 ("DELETE","/10/split",""),("DELETE","/11/split",""),("DELETE","/11/split",""),
 ("GET","/10",""),("GET","/11",""),
 ("POST","/10/split",r#"{"new-id":13,"start-keyspace-id":102,"end-keyspace-id":103}"#),
 ("GET","/13",""),("DELETE","/13/split",""),
 ("POST","/10/merge",r#"{"merge-list":[11,13]}"#),
 ("GET","/10",""),("GET","/11",""),("GET","/13",""),
 ("POST","/10/merge",r#"{"merge-list":[12]}"#),
 ("PATCH","/10",r#"{"nodes":[]}"#),
 ("DELETE","/10/merge",""),("DELETE","/10/merge",""),
 ("POST","/10/merge",r#"{"merge-list":[0]}"#),
 ("POST","/10/merge",r#"{"merge-list":[12],"merge-all-into-default":true}"#),
 ("POST","/10/merge",r#"{"merge-list":[4097]}"#),
 ("DELETE","/10/keyspaces",r#"{"keyspaces":[101,999]}"#),
 ("DELETE","/999/keyspaces",r#"{"keyspaces":[101]}"#),
 ("PATCH","/10/",r#"{"priority":1}"#),
 ("PATCH","/10/alloc/",r#"{"priority":1}"#),
 ("PATCH","/10/split/",r#"{"priority":1}"#),
 ("PATCH","/10/merge/",r#"{"priority":1}"#),
 ("PATCH","/10/keyspaces/",r#"{"priority":1}"#),
 ("PATCH","/10/split",r#"{"priority":1}"#),("PATCH","/10/merge",r#"{"priority":1}"#),("PATCH","/10/keyspaces",r#"{"priority":1}"#),
 ].into_iter().map(|(m,p,b)|(m.into(),p.into(),b.into())));
            for (method, path, input) in cases {
                let before = rows(store.as_ref(), &p).await;
                let response = http(
                    &s.url,
                    &method,
                    &format!("v2/tso/keyspace-groups{path}"),
                    &input,
                );
                let after = rows(store.as_ref(), &p).await;
                if response["status"].as_u64().unwrap() >= 400 {
                    assert_eq!(
                        before, after,
                        "failed transition mutated group {method} {path} {response}"
                    );
                }
                observations
                    .push(json!({"method":method,"path":path,"input":input,"response":response}));
                physical.push(json!({"before":before,"after":after}));
                std::fs::write(
                    format!("/tmp/sa011-progress-{api}-{is_rust}.json"),
                    serde_json::to_vec_pretty(&json!({"http":observations,"physical":physical}))
                        .unwrap(),
                )
                .unwrap();
            }
            // Archived metadata is encoded with the existing protocol codec and committed to the real shared store.
            if api {
                use astersql_pd_storage_domain::KeyspaceCodec;
                use prost::Message;
                for (key, state) in [(101, 2), (102, 0), (103, 3)] {
                    let meta = astersql_pd_proto::keyspacepb::KeyspaceMeta {
                        keyspace: Some(astersql_pd_proto::keyspacepb::keyspace_meta::Keyspace::Id(
                            key,
                        )),
                        name: format!("sa011_{key}"),
                        state,
                        config: std::collections::HashMap::from([(
                            "tso_keyspace_group_id".into(),
                            "10".into(),
                        )]),
                        ..Default::default()
                    };
                    let storage = astersql_pd_storage_domain::KeyspaceStorage::new(
                        store.clone(),
                        p.clone(),
                        Arc::new(astersql_pd_storage_endpoint::ProtobufKeyspaceCodec),
                    );
                    let mut txn = astersql_pd_storage_domain::Base::new(store.as_ref()).begin_txn();
                    storage
                        .save_keyspace_meta(
                            &mut txn,
                            &p,
                            &astersql_pd_storage_endpoint::ProtobufKeyspaceCodec
                                .decode(&meta.encode_to_vec())
                                .unwrap(),
                        )
                        .unwrap();
                    txn.commit::<std::convert::Infallible>().await.unwrap();
                }
                let input = r#"{"keyspaces":[101,102,103,999,101]}"#;
                let r = http(
                    &s.url,
                    "DELETE",
                    "v2/tso/keyspace-groups/10/keyspaces",
                    input,
                );
                assert_eq!(r["status"], 200, "{r}");
                assert_eq!(r["body"]["keyspaces"], json!([102]));
                observations.push(r);
            }
            let before = rows(store.as_ref(), &p).await;
            s.stop().unwrap();
            if is_rust {
                s.restart_rust(&rust).unwrap()
            } else {
                s.restart_with_args(&go, &[]).unwrap()
            };
            s.ready().unwrap();
            assert_eq!(before, rows(store.as_ref(), &p).await);
            // A disconnected partial body cannot publish any of the three mutation payloads.
            use std::io::Write;
            let before_cancel = rows(store.as_ref(), &p).await;
            for (method, operation) in [
                ("POST", "split"),
                ("POST", "merge"),
                ("DELETE", "keyspaces"),
            ] {
                let mut tcp =
                    std::net::TcpStream::connect(s.url.trim_start_matches("http://")).unwrap();
                write!(tcp,"{method} /pd/api/v2/tso/keyspace-groups/10/{operation} HTTP/1.1\r\nHost: localhost\r\nContent-Length: 999999\r\n\r\n{{\"keyspaces\":[101]").unwrap();
                drop(tcp);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
            assert_eq!(before_cancel, rows(store.as_ref(), &p).await);
            let args = if is_rust {
                vec![
                    format!("--etcd-endpoints={}", s.metadata_url()),
                    "--name=follower11".into(),
                ]
            } else {
                vec![
                    format!("--join={}", s.url),
                    "--initial-cluster=".into(),
                    "--name=follower11".into(),
                ]
            };
            let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
            let mut follower =
                Server::start_with_environment(if is_rust { &rust } else { &go }, &refs, &env)
                    .unwrap();
            follower.ready().unwrap();
            for (method, suffix) in routes() {
                let path = format!("v2/tso/keyspace-groups{}", suffix.replace(":id", "999"));
                let direct = http(&s.url, method, &path, "{}");
                let forward = http(&follower.url, method, &path, "{}");
                assert_eq!(direct, forward, "{method} {path}");
                observations.push(forward);
            }
            observations.push(http(&s.url, "GET", "v2/tso/keyspace-groups", ""));
            follower.stop().unwrap();
            s.stop().unwrap();
            reports.push(json!({"api":api,"rust":is_rust,"http":observations,"physical":physical,"restart":before}));
            compared.push(observations);
            physical_compared.push(physical_signature(&physical));
            std::fs::write(
                "/tmp/sa011-matrix.json",
                serde_json::to_vec_pretty(&reports).unwrap(),
            )
            .unwrap();
        }
        assert_eq!(compared[0], compared[1], "api={api}");
        assert_eq!(
            physical_compared[0], physical_compared[1],
            "persisted bytes and commit footprint api={api}"
        );
    }
    assert!(!routes().is_empty());
}
async fn service_timestamp(
    nodes: &[String],
    cluster: u64,
    key: u32,
    expected_group: u32,
) -> (u32, astersql_pd_tso::Timestamp) {
    use astersql_pd_proto::tsopb::{self, tso_client::TsoClient};
    for attempt in 0..1000 {
        for node in nodes {
            let Ok(mut client) = TsoClient::connect(node.clone()).await else {
                continue;
            };
            let req = tsopb::TsoRequest {
                header: Some(tsopb::RequestHeader {
                    cluster_id: cluster,
                    keyspace_group_id: expected_group,
                    keyspace: Some(tsopb::request_header::Keyspace::KeyspaceId(key)),
                    ..Default::default()
                }),
                count: 1,
                ..Default::default()
            };
            let (sender, receiver) = tokio::sync::mpsc::channel(1);
            sender.send(req).await.unwrap();
            let reply = match client
                .tso(tokio_stream::wrappers::ReceiverStream::new(receiver))
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    if attempt == 0 {
                        eprintln!("TSO node={node} key={key} group={expected_group} {e}");
                    }
                    continue;
                }
            };
            let reply = reply.into_inner().message().await;
            drop(sender);
            if let Err(ref e) = reply {
                use std::io::Write;
                writeln!(
                    std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open("/tmp/sa011-rpc-errors.jsonl")
                        .unwrap(),
                    "{}",
                    json!({"group":expected_group,"node":node,"key":key,"error":format!("{e:?}")})
                )
                .unwrap();
            }
            if let Ok(Some(r)) = reply {
                use std::io::Write;
                writeln!(std::fs::OpenOptions::new().create(true).append(true).open("/tmp/sa011-rpc.jsonl").unwrap(),"{}",json!({"node":node,"cluster":cluster,"keyspace":key,"requested_group":expected_group,"count":1,"response":format!("{r:?}")})).unwrap();
                let t = r.timestamp.unwrap();
                return (
                    r.header.unwrap().keyspace_group_id,
                    astersql_pd_tso::Timestamp {
                        physical: t.physical as u64,
                        logical: t.logical as u32,
                    },
                );
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("keyspace {key} could not issue on {nodes:?}")
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_011_mcs_144_behavior() {
    let rust = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    std::fs::write("/tmp/sa011-rpc.jsonl", "").unwrap();
    let mut semantics = vec![];
    for is_rust in [false, true] {
        let env = [("PD_SERVICE_MODE".into(), "api".into())];
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
        pd.bootstrap(pdpb::BootstrapRequest {
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
        .unwrap();
        let store: Arc<dyn MetaStore> = Arc::new(
            astersql_pd_storage_kv::EtcdMetaStore::connect(
                &[s.metadata_url().into()],
                Duration::from_secs(2),
            )
            .await
            .unwrap(),
        );
        let paths = ClusterPaths::new(id);
        let groups = Arc::new(astersql_pd_keyspace::KeyspaceGroupManager::new(
            store.clone(),
            paths.clone(),
        ));
        groups.bootstrap().await.unwrap();
        let mut hosts = vec![];
        let mut go_hosts = vec![];
        let mut nodes = vec![];
        for _ in 0..2 {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = format!("http://{}", listener.local_addr().unwrap());
            nodes.push(address.clone());
            if !is_rust {
                drop(listener);
                let log = std::fs::File::create(format!(
                    "/tmp/sa011-go-tso-{}-{}.log",
                    std::process::id(),
                    nodes.len()
                ))
                .unwrap();
                let child = std::process::Command::new(&go)
                    .args([
                        "services",
                        "tso",
                        "--backend-endpoints",
                        &s.url,
                        "--listen-addr",
                        &address,
                        "--name",
                        &address,
                    ])
                    .stdout(log.try_clone().unwrap())
                    .stderr(log)
                    .spawn()
                    .unwrap();
                go_hosts.push(GoTso(child));
                continue;
            }
            let config = tso_config(store.clone(), id, address);
            hosts.push(
                astersql_pd_mcs_tso_server::TsoServiceHost::start_with_pd_http(
                    config,
                    groups.clone(),
                    listener,
                    s.url.clone(),
                )
                .await
                .unwrap(),
            );
        }
        assert_eq!(
            http(
                &s.url,
                "POST",
                "v2/tso/keyspace-groups",
                r#"{"keyspace-groups":[{"id":10,"keyspaces":[101,102]}]}"#
            )["status"],
            200
        );
        let mut allocated = false;
        for _ in 0..80 {
            let r = http(
                &s.url,
                "PATCH",
                "v2/tso/keyspace-groups/10",
                &serde_json::to_string(&json!({"nodes":nodes})).unwrap(),
            );
            if r["status"] == 200 {
                allocated = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(allocated);
        let mut default_ready = false;
        for _ in 0..100 {
            let g = http(&s.url, "GET", "v2/tso/keyspace-groups/0", "");
            if g["body"]["members"]
                .as_array()
                .is_some_and(|m| m.len() == 2)
            {
                default_ready = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(
            default_ready,
            "canonical host must reconcile the default group's actual registered members"
        );

        let (original, before) = service_timestamp(&nodes, id, 101, 10).await;
        assert_eq!(original, 10);
        let split = http(
            &s.url,
            "POST",
            "v2/tso/keyspace-groups/10/split",
            r#"{"new-id":11,"keyspaces":[101]}"#,
        );
        assert_eq!(split["status"], 200, "{split}");
        let (target, after_split) = service_timestamp(&nodes, id, 101, 11).await;
        assert_eq!(target, 11);
        assert!(after_split > before);
        let (source, source_stamp) = service_timestamp(&nodes, id, 102, 10).await;
        assert_eq!(source, 10);
        assert!(source_stamp > before);
        let split_state = http(&s.url, "GET", "v2/tso/keyspace-groups/11", "");
        assert!(
            split_state["body"].get("split-state").is_none(),
            "{split_state}"
        );
        let merged = http(
            &s.url,
            "POST",
            "v2/tso/keyspace-groups/10/merge",
            r#"{"merge-list":[11]}"#,
        );
        assert_eq!(merged["status"], 200, "{merged}");
        let (target, after_merge) = service_timestamp(&nodes, id, 101, 10).await;
        assert_eq!(target, 10);
        assert!(after_merge > after_split);
        assert!(after_merge > source_stamp);
        let final_state = http(&s.url, "GET", "v2/tso/keyspace-groups/10", "");
        assert!(final_state["body"].get("merge-state").is_none());
        // Real client discovery follows the same watch assignment as stream allocation.
        let mut found = false;
        for node in &nodes {
            use astersql_pd_proto::tsopb;
            let mut client = tsopb::tso_client::TsoClient::connect(node.clone())
                .await
                .unwrap();
            let r = client
                .find_group_by_keyspace_id(tsopb::FindGroupByKeyspaceIdRequest {
                    header: Some(tsopb::RequestHeader {
                        cluster_id: id,
                        ..Default::default()
                    }),
                    keyspace: Some(
                        tsopb::find_group_by_keyspace_id_request::Keyspace::KeyspaceId(101),
                    ),
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner();
            let group = r.keyspace_group.unwrap();
            assert_eq!(group.id, 10);
            assert_eq!(group.members.iter().filter(|m| m.is_primary).count(), 1);

            found = true;
        }
        assert!(found);
        let all = http(
            &s.url,
            "POST",
            "v2/tso/keyspace-groups/999/merge",
            r#"{"merge-all-into-default":true}"#,
        );
        assert_eq!(all["status"], 200, "{all}");
        let (default, after_all) = service_timestamp(&nodes, id, 101, 0).await;
        assert_eq!(default, 0);
        assert!(after_all > after_merge);
        let mut semantic=serde_json::to_string(&json!({"split":split,"split_state":split_state,"merged":merged,"final_state":final_state,"merge_all":all,"default":http(&s.url,"GET","v2/tso/keyspace-groups/0","")})).unwrap();
        for (index, node) in nodes.iter().enumerate() {
            semantic = semantic.replace(node, &format!("<tso:{index}>"));
        }
        let mut semantic = serde_json::from_str::<serde_json::Value>(&semantic).unwrap();
        // The default group's automatic allocation follows Go map/discovery order.
        // Explicit caller-provided member order on groups 10/11 remains compared verbatim.
        semantic["default"]["body"]["members"]
            .as_array_mut()
            .unwrap()
            .sort_by_key(|m| m["address"].as_str().unwrap().to_owned());
        semantics.push(semantic);
        let mut evidence = json!({"nodes":nodes,"split":split,"split_state":split_state,"merged":merged,"final_state":final_state,"before":format!("{before:?}"),"after_split":format!("{after_split:?}"),"source":format!("{source_stamp:?}"),"after_merge":format!("{after_merge:?}"),"merge_all":all,"after_all":format!("{after_all:?}"),"physical":rows(store.as_ref(),&paths).await});
        std::fs::write(
            format!("/tmp/sa011-tso-chain-{is_rust}.json"),
            serde_json::to_vec_pretty(&evidence).unwrap(),
        )
        .unwrap();
        for host in hosts {
            tokio::time::timeout(Duration::from_secs(5), host.shutdown())
                .await
                .unwrap()
                .unwrap();
        }
        for host in go_hosts {
            host.stop();
        }
        let durable = rows(store.as_ref(), &paths).await;
        s.stop().unwrap();
        if is_rust {
            s.restart_rust(&rust).unwrap()
        } else {
            s.restart_with_args(&go, &[]).unwrap()
        };
        s.ready().unwrap();
        let store: Arc<dyn MetaStore> = Arc::new(
            astersql_pd_storage_kv::EtcdMetaStore::connect(
                &[s.metadata_url().into()],
                Duration::from_secs(2),
            )
            .await
            .unwrap(),
        );
        assert_eq!(durable, rows(store.as_ref(), &paths).await);
        let groups = Arc::new(astersql_pd_keyspace::KeyspaceGroupManager::new(
            store.clone(),
            paths.clone(),
        ));
        groups.bootstrap().await.unwrap();
        let mut restarted = vec![];
        let mut restarted_go = vec![];
        for (index, address) in nodes.iter().enumerate() {
            if is_rust {
                let listener = tokio::net::TcpListener::bind(address.trim_start_matches("http://"))
                    .await
                    .unwrap();
                restarted.push(
                    astersql_pd_mcs_tso_server::TsoServiceHost::start_with_pd_http(
                        tso_config(store.clone(), id, address.clone()),
                        groups.clone(),
                        listener,
                        s.url.clone(),
                    )
                    .await
                    .unwrap(),
                );
            } else {
                let log = std::fs::File::create(format!(
                    "/tmp/sa011-go-tso-{}-restart-{index}.log",
                    std::process::id()
                ))
                .unwrap();
                restarted_go.push(GoTso(
                    std::process::Command::new(&go)
                        .args([
                            "services",
                            "tso",
                            "--backend-endpoints",
                            &s.url,
                            "--listen-addr",
                            address,
                            "--name",
                            address,
                        ])
                        .stdout(log.try_clone().unwrap())
                        .stderr(log)
                        .spawn()
                        .unwrap(),
                ));
            }
        }
        let (group, recovered) = service_timestamp(&nodes, id, 101, 0).await;
        assert_eq!(group, 0);
        assert!(recovered > after_all);
        evidence["after_wal"] = json!(format!("{recovered:?}"));
        evidence["wal_directory"] = durable;
        std::fs::write(
            format!("/tmp/sa011-tso-chain-{is_rust}.json"),
            serde_json::to_vec_pretty(&evidence).unwrap(),
        )
        .unwrap();
        for host in restarted {
            tokio::time::timeout(Duration::from_secs(5), host.shutdown())
                .await
                .unwrap()
                .unwrap();
        }
        for host in restarted_go {
            host.stop();
        }
        s.stop().unwrap();
    }
    assert_eq!(semantics[0], semantics[1]);
}
fn tso_config(
    store: Arc<dyn MetaStore>,
    id: u64,
    address: String,
) -> astersql_pd_tso::GroupWatchConfig {
    use prost::Message;
    let name = address.clone();
    let config = astersql_pd_tso::GroupWatchConfig {
        store: store.clone(),
        runtime: tokio::runtime::Handle::current(),
        cancel: tokio_util::sync::CancellationToken::new(),
        cluster_id: id,
        address,
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
    config
}
struct GoTso(std::process::Child);
impl GoTso {
    fn stop(mut self) {
        assert!(
            std::process::Command::new("kill")
                .args(["-TERM", &self.0.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        while std::time::Instant::now() < deadline {
            if let Some(status) = self.0.try_wait().unwrap() {
                assert!(status.success());
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("Go TSO graceful shutdown timed out");
    }
}
impl Drop for GoTso {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_011_legacy_assignment_before_split() {
    use astersql_pd_storage_domain::{Consistency, Mutation, Read};
    use prost::Message;
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut compared = vec![];
    let mut reports = vec![];
    for is_rust in [false, true] {
        let env = [("PD_SERVICE_MODE".into(), "api".into())];
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
        pd.bootstrap(pdpb::BootstrapRequest {
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
        .unwrap();
        let store = astersql_pd_storage_kv::EtcdMetaStore::connect(
            &[s.metadata_url().into()],
            Duration::from_secs(2),
        )
        .await
        .unwrap();
        let p = ClusterPaths::new(id);
        let before = http(&s.url, "GET", "v2/tso/keyspace-groups/0", "");
        let mut g = before["body"].clone();
        g["members"] =
            json!([{"address":"http://a","priority":0},{"address":"http://b","priority":0}]);
        let meta = astersql_pd_proto::keyspacepb::KeyspaceMeta {
            keyspace: Some(astersql_pd_proto::keyspacepb::keyspace_meta::Keyspace::Id(
                101,
            )),
            name: "legacy101".into(),
            ..Default::default()
        };
        store
            .transact(
                vec![],
                vec![
                    Mutation::put(p.keyspace_meta_path(101).as_bytes(), &meta.encode_to_vec()),
                    Mutation::put(
                        p.keyspace_group_id_path(0).as_bytes(),
                        &serde_json::to_vec(&g).unwrap(),
                    ),
                ],
            )
            .await
            .unwrap();
        let split = http(
            &s.url,
            "POST",
            "v2/tso/keyspace-groups/0/split",
            r#"{"new-id":1,"keyspaces":[101]}"#,
        );
        assert_eq!(split["status"], 200, "{split}");
        let source = http(&s.url, "GET", "v2/tso/keyspace-groups/0", "");
        let target = http(&s.url, "GET", "v2/tso/keyspace-groups/1", "");
        assert_eq!(source["body"]["keyspaces"], json!([0]));
        assert_eq!(target["body"]["keyspaces"], json!([101]));
        let row = store
            .read(Read::prefix(
                p.keyspace_meta_path(101).as_bytes(),
                Consistency::Linearizable,
            ))
            .await
            .unwrap()
            .entries
            .remove(0);
        let loaded =
            astersql_pd_proto::keyspacepb::KeyspaceMeta::decode(row.value.as_slice()).unwrap();
        assert_eq!(loaded.config["tso_keyspace_group_id"], "0");
        let finish = http(&s.url, "DELETE", "v2/tso/keyspace-groups/1/split", "");
        assert_eq!(finish["status"], 200);
        compared.push(json!({"source":source,"target":target,"split":split,"finish":finish,"keyspace":format!("{loaded:?}")}));
        reports.push(json!({"rust":is_rust,"physical":rows(&store,&p).await,"keyspace_bytes":row.value,"keyspace_revision":row.mod_revision}));
        s.stop().unwrap();
    }
    assert_eq!(compared[0], compared[1]);
    std::fs::write(
        "/tmp/sa011-legacy.json",
        serde_json::to_vec_pretty(&json!({"http":compared,"physical":reports})).unwrap(),
    )
    .unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_011_patch_trailing_behavior() {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut observations = vec![];
    for is_rust in [false, true] {
        let mut s = if is_rust {
            Server::start_rust(&rust)
        } else {
            Server::start(&go)
        }
        .unwrap();
        s.ready().unwrap();
        let observed = ["1/", "1/alloc/", "1/split/", "1/merge/", "1/keyspaces/"]
            .iter()
            .map(|suffix| {
                http(
                    &s.url,
                    "PATCH",
                    &format!("v2/tso/keyspace-groups/{suffix}"),
                    r#"{"priority":1}"#,
                )
            })
            .collect::<Vec<_>>();
        s.stop().unwrap();
        observations.push(observed);
    }
    std::fs::write(
        "/tmp/sa011-patch-trailing.json",
        serde_json::to_vec_pretty(&observations).unwrap(),
    )
    .unwrap();
    assert_eq!(observations[0], observations[1]);
    assert!(observations[0].iter().all(|r| r["status"] == 500));
}
