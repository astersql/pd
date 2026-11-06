// Copyright 2026 AsterSQL.
use crate::harness::Server;
use astersql_pd_proto::{meta_storagepb as p, pdpb};
use astersql_pd_storage_domain::{Consistency, MetaStore, Mutation, Read};
use astersql_pd_storage_kv::EtcdMetaStore;
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};

fn save(records: &[Value]) {
    let path =
        PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").expect("explicit evidence output"));
    let first = records.first().unwrap();
    let kind = if first.get("method").is_some() {
        "write"
    } else if first["scenario"] == "watch-open" {
        "watch"
    } else {
        "get"
    };
    let path = path.with_file_name(format!(
        "{}-{kind}.json",
        path.file_name().unwrap().to_string_lossy()
    ));
    std::fs::write(path, serde_json::to_vec(records).unwrap()).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_metastorage_get() {
    assert_eq!(std::env::var("PD_COMPAT_MODE").unwrap(), "dual");
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut records = Vec::new();
    let mut expected = None;
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        let mut server = if is_rust {
            Server::start_rust(binary)
        } else {
            Server::start(binary)
        }
        .unwrap();
        server.ready().unwrap();
        let store = EtcdMetaStore::connect(&[server.metadata_url().into()], Duration::from_secs(3))
            .await
            .unwrap();
        let commit = store
            .transact(
                vec![],
                vec![
                    Mutation::put(b"sa16/a", b"alpha"),
                    Mutation::put(b"sa16/b", b"beta"),
                ],
            )
            .await
            .unwrap();
        let mut client = p::meta_storage_client::MetaStorageClient::connect(server.url.clone())
            .await
            .unwrap();
        let mut pd = pdpb::pd_client::PdClient::connect(server.url.clone())
            .await
            .unwrap();
        let cluster = pd
            .get_members(pdpb::GetMembersRequest::default())
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .cluster_id;
        let updated = store
            .transact(
                vec![],
                vec![
                    Mutation::put(b"sa16/a", b"changed"),
                    Mutation::put(b"sa16/c", b"gamma"),
                    Mutation::put(b"sa16outside", b"outside"),
                ],
            )
            .await
            .unwrap();
        let mut responses = Vec::new();
        let point = p::GetRequest {
            key: b"sa16/a".to_vec(),
            ..Default::default()
        };
        let range = p::GetRequest {
            key: b"sa16/".to_vec(),
            range_end: b"sa160".to_vec(),
            ..Default::default()
        };
        let mut cases = vec![
            ("point", point.clone()),
            (
                "missing",
                p::GetRequest {
                    key: b"sa16/missing".to_vec(),
                    ..Default::default()
                },
            ),
            ("range", range.clone()),
            (
                "limit",
                p::GetRequest {
                    limit: 1,
                    ..range.clone()
                },
            ),
            (
                "negative-limit",
                p::GetRequest {
                    limit: -1,
                    ..range.clone()
                },
            ),
            (
                "historical",
                p::GetRequest {
                    revision: commit.revision,
                    ..range.clone()
                },
            ),
            (
                "negative-revision",
                p::GetRequest {
                    revision: -1,
                    ..range.clone()
                },
            ),
            (
                "from-key",
                p::GetRequest {
                    key: b"sa16/b".to_vec(),
                    range_end: vec![0],
                    ..Default::default()
                },
            ),
            (
                "reversed-range",
                p::GetRequest {
                    key: b"sa16/z".to_vec(),
                    range_end: b"sa16/a".to_vec(),
                    ..Default::default()
                },
            ),
            ("empty-key", p::GetRequest::default()),
            (
                "future-revision",
                p::GetRequest {
                    revision: i64::MAX,
                    ..point.clone()
                },
            ),
            (
                "ignored-cluster-header",
                p::GetRequest {
                    header: Some(p::RequestHeader {
                        cluster_id: 1,
                        source: "unexpected".into(),
                    }),
                    ..point.clone()
                },
            ),
        ];
        for (label, request) in cases.drain(..) {
            let before = store
                .read(Read::prefix(b"", Consistency::Linearizable))
                .await
                .unwrap()
                .revision;
            let result = client.get(request.clone()).await;
            records.push(json!({"rust":is_rust,"scenario":label,"request":format!("{request:?}"),"response":format!("{result:?}")}));
            save(&records);
            let response = result
                .expect("registered MetaStorage.Get must read canonical state")
                .into_inner();
            let after = store
                .read(Read::prefix(b"", Consistency::Linearizable))
                .await
                .unwrap()
                .revision;
            let h = response.header.as_ref().unwrap();
            assert_eq!(h.cluster_id, cluster);
            if h.error.is_none() {
                assert!(
                    (before..=after).contains(&h.revision),
                    "{label}: {response:?}"
                );
            }
            let body = json!({"count":response.count,"more":response.more,"error":h.error.as_ref().map(|e|json!({"type":e.r#type,"message":e.message})),"kvs":response.kvs.iter().map(|v|json!({"key":v.key,"value":v.value,"create_revision":v.create_revision,"mod_revision":v.mod_revision,"version":v.version,"lease":v.lease})).collect::<Vec<_>>()});
            if label == "limit" {
                assert_eq!(response.count, 3);
                assert!(response.more);
                assert_eq!(response.kvs.len(), 1);
            }
            if label == "point" {
                assert_eq!(response.kvs[0].value, b"changed");
                assert_eq!(response.count, 1);
            }
            if label == "historical" {
                assert_eq!(response.kvs.len(), 2);
                assert_eq!(response.kvs[0].value, b"alpha");
            }
            if label == "from-key" {
                assert_eq!(
                    response
                        .kvs
                        .iter()
                        .map(|e| e.key.clone())
                        .collect::<Vec<_>>(),
                    vec![
                        b"sa16/b".to_vec(),
                        b"sa16/c".to_vec(),
                        b"sa16outside".to_vec()
                    ]
                );
            }
            responses.push(body);
        }
        store.compact(updated.revision).await.unwrap();
        let response = client
            .get(p::GetRequest {
                revision: commit.revision,
                ..point
            })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(
            response
                .header
                .as_ref()
                .unwrap()
                .error
                .as_ref()
                .unwrap()
                .message,
            "etcdserver: mvcc: required revision has been compacted"
        );
        records.push(
            json!({"rust":is_rust,"scenario":"compacted","response":format!("{response:?}")}),
        );
        save(&records);
        responses.push(json!({"compacted":response.header.unwrap().error.unwrap().message}));
        let extra = if is_rust {
            vec![
                format!("--etcd-endpoints={}", server.metadata_url()),
                "--name=follower16".into(),
            ]
        } else {
            vec![
                format!("--join={}", server.url),
                "--initial-cluster=".into(),
                "--name=follower16".into(),
            ]
        };
        let args: Vec<&str> = extra.iter().map(String::as_str).collect();
        let mut follower = Server::start_with_args(binary, &args).unwrap();
        follower.ready().unwrap();
        let members = pdpb::pd_client::PdClient::connect(follower.url.clone())
            .await
            .unwrap()
            .get_members(pdpb::GetMembersRequest::default())
            .await
            .unwrap()
            .into_inner();
        assert_ne!(members.leader.as_ref().unwrap().name, "follower16");
        let mut follower_client =
            p::meta_storage_client::MetaStorageClient::connect(follower.url.clone())
                .await
                .unwrap();
        let denied = follower_client
            .get(p::GetRequest {
                key: b"sa16/a".to_vec(),
                ..Default::default()
            })
            .await
            .unwrap_err();
        assert_eq!(denied.code(), tonic::Code::Unavailable);
        assert_eq!(denied.message(), "not leader");
        records.push(json!({"rust":is_rust,"scenario":"follower-get","members":format!("{members:?}"),"response":format!("{denied:?}")}));
        save(&records);
        responses.push(
            json!({"follower_code":format!("{:?}",denied.code()),"message":denied.message()}),
        );
        follower.stop_with_timeout(Duration::from_secs(15)).unwrap();
        if is_rust {
            assert_eq!(expected.as_ref().unwrap(), &responses);
        } else {
            expected = Some(responses);
        }
        server.stop().unwrap();
    }
}
async fn write_cases(server: &mut Server, is_rust: bool, records: &mut Vec<Value>) -> Vec<Value> {
    let store = EtcdMetaStore::connect(&[server.metadata_url().into()], Duration::from_secs(3))
        .await
        .unwrap();
    let mut client = p::meta_storage_client::MetaStorageClient::connect(server.url.clone())
        .await
        .unwrap();
    let mut responses = Vec::new();
    let key = b"sa16/write".to_vec();
    let lease = store.grant(30).await.unwrap();
    let puts = vec![
        (
            "create",
            p::PutRequest {
                key: key.clone(),
                value: b"first".to_vec(),
                prev_kv: true,
                ..Default::default()
            },
        ),
        (
            "replace-prev",
            p::PutRequest {
                key: key.clone(),
                value: b"second".to_vec(),
                prev_kv: true,
                ..Default::default()
            },
        ),
        (
            "replace-no-prev",
            p::PutRequest {
                key: key.clone(),
                value: b"third".to_vec(),
                ..Default::default()
            },
        ),
        (
            "attach-lease",
            p::PutRequest {
                key: key.clone(),
                value: b"leased".to_vec(),
                lease: lease.id,
                prev_kv: true,
                ..Default::default()
            },
        ),
        (
            "missing-lease",
            p::PutRequest {
                key: b"sa16/absent".to_vec(),
                value: b"bad".to_vec(),
                lease: i64::MAX,
                ..Default::default()
            },
        ),
        (
            "negative-lease",
            p::PutRequest {
                key: b"sa16/absent".to_vec(),
                value: b"bad".to_vec(),
                lease: -1,
                ..Default::default()
            },
        ),
        ("empty-key", p::PutRequest::default()),
    ];
    for (label, request) in puts {
        let result = client.put(request.clone()).await;
        records.push(json!({"rust":is_rust,"method":"Put","scenario":label,"request":format!("{request:?}"),"response":format!("{result:?}")}));
        save(records);
        let response = result
            .expect("registered MetaStorage.Put must mutate canonical state")
            .into_inner();
        let h = response.header.unwrap();
        let actual = store
            .read(Read::range(&request.key, b"", Consistency::Linearizable))
            .await;
        records.last_mut().unwrap()["persisted_state"] = json!(format!("{actual:?}"));
        save(records);
        if h.error.is_none() {
            let state = actual.unwrap();
            let entry = &state.entries[0];
            assert_eq!(entry.value, request.value);
            assert_eq!(entry.mod_revision, h.revision);
            assert_eq!(
                entry.lease,
                if request.lease == 0 {
                    None
                } else {
                    Some(request.lease)
                }
            );
        } else if !request.key.is_empty() {
            assert!(actual.unwrap().entries.is_empty());
        }
        if label == "replace-prev" {
            assert_eq!(response.prev_kv.as_ref().unwrap().value, b"first");
        }
        if label == "replace-no-prev" {
            assert!(response.prev_kv.is_none());
        }
        responses.push(json!({"scenario":label,"error":h.error.map(|e|json!({"type":e.r#type,"message":e.message})),"prev":format!("{:?}",response.prev_kv)}));
    }
    store.revoke(lease.id).await.unwrap();
    assert!(
        store
            .read(Read::range(&key, b"", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    store
        .transact(
            vec![],
            vec![
                Mutation::put(b"sa16/d/a", b"a"),
                Mutation::put(b"sa16/d/b", b"b"),
            ],
        )
        .await
        .unwrap();
    for (label, request) in [
        (
            "range-is-ignored",
            p::DeleteRequest {
                key: b"sa16/d/a".to_vec(),
                range_end: vec![0],
                prev_kv: true,
                ..Default::default()
            },
        ),
        (
            "missing",
            p::DeleteRequest {
                key: b"sa16/d/a".to_vec(),
                prev_kv: true,
                ..Default::default()
            },
        ),
        (
            "no-prev",
            p::DeleteRequest {
                key: b"sa16/d/b".to_vec(),
                ..Default::default()
            },
        ),
        ("empty", p::DeleteRequest::default()),
    ] {
        let result = client.delete(request.clone()).await;
        records.push(json!({"rust":is_rust,"method":"Delete","scenario":label,"request":format!("{request:?}"),"response":format!("{result:?}")}));
        save(records);
        let response = result.unwrap().into_inner();
        let h = response.header.unwrap();
        assert_eq!(response.deleted, 0, "Go intentionally leaves deleted unset");
        if label == "range-is-ignored" {
            assert_eq!(response.prev_kvs.len(), 1);
            assert_eq!(response.prev_kvs[0].value, b"a");
            assert_eq!(
                store
                    .read(Read::range(b"sa16/d/b", b"", Consistency::Linearizable))
                    .await
                    .unwrap()
                    .entries
                    .len(),
                1
            );
        }
        if !request.key.is_empty() {
            assert!(
                store
                    .read(Read::range(&request.key, b"", Consistency::Linearizable))
                    .await
                    .unwrap()
                    .entries
                    .is_empty()
            );
        }
        responses.push(json!({"scenario":label,"error":h.error.map(|e|json!({"type":e.r#type,"message":e.message})),"prev":format!("{:?}",response.prev_kvs),"deleted":response.deleted}));
    }
    responses
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_metastorage_write() {
    assert_eq!(std::env::var("PD_COMPAT_MODE").unwrap(), "dual");
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut expected = None;
    let mut records = Vec::new();
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        let mut server = if is_rust {
            Server::start_rust(binary)
        } else {
            Server::start(binary)
        }
        .unwrap();
        server.ready().unwrap();
        let response = write_cases(&mut server, is_rust, &mut records).await;
        if is_rust {
            assert_eq!(expected.as_ref().unwrap(), &response);
        } else {
            expected = Some(response);
        }
        server.stop().unwrap();
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_metastorage_watch() {
    assert_eq!(std::env::var("PD_COMPAT_MODE").unwrap(), "dual");
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut records = Vec::new();
    let mut expected = None;
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        let mut server = if is_rust {
            Server::start_rust(binary)
        } else {
            Server::start(binary)
        }
        .unwrap();
        server.ready().unwrap();
        let store = EtcdMetaStore::connect(&[server.metadata_url().into()], Duration::from_secs(3))
            .await
            .unwrap();
        let seed = store
            .transact(
                vec![],
                vec![
                    Mutation::put(b"sa16/watch/a", b"first"),
                    Mutation::put(b"sa16/watch/b", b"neighbor"),
                ],
            )
            .await
            .unwrap();
        let mut client = p::meta_storage_client::MetaStorageClient::connect(server.url.clone())
            .await
            .unwrap();
        let request = p::WatchRequest {
            key: b"sa16/watch/a".to_vec(),
            start_revision: seed.revision,
            prev_kv: true,
            ..Default::default()
        };
        let result = tokio::time::timeout(Duration::from_secs(10), client.watch(request.clone()))
            .await
            .unwrap();
        records.push(json!({"rust":is_rust,"scenario":"watch-open","request":format!("{request:?}"),"response":format!("{result:?}")}));
        save(&records);
        let mut stream = result
            .expect("registered MetaStorage.Watch must replay the requested revision")
            .into_inner();
        let mut observed = Vec::new();
        for stage in 0..3 {
            if stage == 1 {
                store
                    .transact(vec![], vec![Mutation::put(b"sa16/watch/a", b"second")])
                    .await
                    .unwrap();
            }
            if stage == 2 {
                store
                    .transact(
                        vec![],
                        vec![astersql_pd_storage_domain::Mutation::Delete {
                            key: b"sa16/watch/a".to_vec(),
                        }],
                    )
                    .await
                    .unwrap();
            }
            let response = tokio::time::timeout(Duration::from_secs(10), stream.message())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            records.push(json!({"rust":is_rust,"scenario":"watch-event","stage":stage,"response":format!("{response:?}")}));
            save(&records);
            assert_eq!(
                response.events.len(),
                1,
                "point watch must exclude the neighboring key"
            );
            let event = &response.events[0];
            let kv = event.kv.as_ref().unwrap();
            assert_eq!(kv.key, b"sa16/watch/a");
            assert_eq!(kv.mod_revision, response.header.as_ref().unwrap().revision);
            let version = [1, 2, 0][stage];
            assert_eq!(kv.version, version);
            if stage == 1 {
                assert_eq!(event.prev_kv.as_ref().unwrap().value, b"first");
                assert_eq!(event.prev_kv.as_ref().unwrap().mod_revision, 0);
            }
            if stage == 2 {
                assert_eq!(event.prev_kv.as_ref().unwrap().value, b"second");
                assert_eq!(event.r#type, 1);
            }
            observed.push(json!({"stage":stage,"value":kv.value,"version":kv.version,"type":event.r#type,"prev":format!("{:?}",event.prev_kv),"lease":kv.lease}));
        }
        drop(stream);
        let after = client
            .get(p::GetRequest {
                key: b"sa16/watch/a".to_vec(),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        assert!(after.kvs.is_empty());
        let outside = store
            .transact(
                vec![],
                vec![Mutation::put(b"sa16/outside", b"after-last-matching-event")],
            )
            .await
            .unwrap();
        let mut replay = client
            .watch(p::WatchRequest {
                key: b"sa16/watch/a".to_vec(),
                start_revision: seed.revision,
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        let response = tokio::time::timeout(Duration::from_secs(10), replay.message())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        records.push(json!({"rust":is_rust,"scenario":"historical-header","outside_revision":outside.revision,"response":format!("{response:?}")}));
        save(&records);
        observed.push(json!({"historical_header_delta_from_independent_commit":response.header.as_ref().unwrap().revision-outside.revision,"events":response.events.iter().map(|e|json!({"type":e.r#type,"value":e.kv.as_ref().unwrap().value,"version":e.kv.as_ref().unwrap().version})).collect::<Vec<_>>()}));
        drop(replay);
        for (label, end) in [
            ("bounded-range", b"sa16/watch/c".to_vec()),
            ("from-key", vec![0]),
        ] {
            let mut stream = client
                .watch(p::WatchRequest {
                    key: b"sa16/watch/a".to_vec(),
                    range_end: end,
                    start_revision: seed.revision,
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner();
            let response = tokio::time::timeout(Duration::from_secs(10), stream.message())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(response.events.len(), 4);
            assert!(response.events.iter().all(|e| e.prev_kv.is_none()));
            assert_eq!(response.header.as_ref().unwrap().revision, outside.revision);
            records
                .push(json!({"rust":is_rust,"scenario":label,"response":format!("{response:?}")}));
            save(&records);
            observed.push(json!({"scenario":label,"events":response.events.iter().map(|e|json!({"key":e.kv.as_ref().unwrap().key,"value":e.kv.as_ref().unwrap().value,"version":e.kv.as_ref().unwrap().version,"type":e.r#type})).collect::<Vec<_>>()}));
        }
        store.compact(outside.revision).await.unwrap();
        for (label, request) in [(
            "compacted",
            p::WatchRequest {
                key: b"sa16/watch/a".to_vec(),
                start_revision: seed.revision,
                ..Default::default()
            },
        )] {
            let result =
                tokio::time::timeout(Duration::from_secs(10), client.watch(request.clone()))
                    .await
                    .unwrap();
            records.push(json!({"rust":is_rust,"scenario":label,"request":format!("{request:?}"),"open":format!("{result:?}")}));
            save(&records);
            let mut stream = result.unwrap().into_inner();
            let response = tokio::time::timeout(Duration::from_secs(10), stream.message())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let h = response.header.as_ref().unwrap();
            let error = h.error.as_ref().unwrap();
            if label == "compacted" {
                assert_eq!(response.compact_revision, outside.revision);
                assert_eq!(error.r#type, p::ErrorType::DataCompacted as i32);
                assert_eq!(
                    error.message,
                    format!(
                        "required watch revision: {} is smaller than current compact/min revision {}.",
                        seed.revision, outside.revision
                    )
                );
            }
            let terminal = tokio::time::timeout(Duration::from_secs(10), stream.message())
                .await
                .unwrap()
                .unwrap_err();
            assert_eq!(terminal.code(), tonic::Code::Unknown);
            records.push(json!({"rust":is_rust,"scenario":format!("{label}-end"),"response":format!("{response:?}"),"terminal":format!("{terminal:?}")}));
            save(&records);
            observed.push(json!({"scenario":label,"header_revision":h.revision,"type":error.r#type,"error_message":if label=="compacted"{None}else{Some(&error.message)},"terminal_code":format!("{:?}",terminal.code()),"terminal_message":terminal.message()}));
        }
        let snapshot = client
            .get(p::GetRequest {
                key: b"sa16/watch/a".to_vec(),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        let start = snapshot.header.unwrap().revision + 1;
        let (mut reconnected, committed) = {
            let open = client.watch(p::WatchRequest {
                key: b"sa16/watch/a".to_vec(),
                start_revision: start,
                ..Default::default()
            });
            tokio::pin!(open);
            assert!(
                tokio::time::timeout(Duration::from_millis(30), &mut open)
                    .await
                    .is_err(),
                "idle watch must wait for the first message before returning headers"
            );
            let committed = store
                .transact(
                    vec![],
                    vec![Mutation::put(b"sa16/watch/a", b"after-reconnect")],
                )
                .await
                .unwrap();
            (open.await.unwrap().into_inner(), committed)
        };
        let response = tokio::time::timeout(Duration::from_secs(10), reconnected.message())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let kv = response.events[0].kv.as_ref().unwrap();
        assert_eq!(kv.version, 1);
        assert_eq!(kv.value, b"after-reconnect");
        assert_eq!(kv.create_revision, committed.revision);
        assert_eq!(kv.mod_revision, committed.revision);
        records.push(json!({"rust":is_rust,"scenario":"reconnect-after-compaction","request_start":start,"commit":committed.revision,"response":format!("{response:?}")}));
        save(&records);
        observed.push(json!({"reconnect_value":kv.value,"version":kv.version}));
        drop(reconnected);
        let mut empty = tonic::Request::new(p::WatchRequest::default());
        empty.set_timeout(Duration::from_millis(300));
        let result = tokio::time::timeout(Duration::from_secs(3), client.watch(empty))
            .await
            .unwrap();
        records.push(
            json!({"rust":is_rust,"scenario":"empty-watch-deadline","open":format!("{result:?}")}),
        );
        save(&records);
        let empty_result = match result {
            Err(error) => {
                json!({"open_error_code":format!("{:?}",error.code()),"message":error.message()})
            }
            Ok(response) => {
                let first =
                    tokio::time::timeout(Duration::from_secs(3), response.into_inner().message())
                        .await;
                records.push(json!({"rust":is_rust,"scenario":"empty-watch-first","response":format!("{first:?}")}));
                save(&records);
                json!({"open_succeeded":true,"first":format!("{first:?}")})
            }
        };
        observed.push(empty_result);
        drop(client);
        if is_rust {
            server.restart_rust(binary).unwrap();
        } else {
            server.restart_with_args(binary, &[]).unwrap();
        }
        server.ready().unwrap();
        let mut restored = p::meta_storage_client::MetaStorageClient::connect(server.url.clone())
            .await
            .unwrap();
        let persisted = restored
            .get(p::GetRequest {
                key: b"sa16/watch/a".to_vec(),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(persisted.kvs[0].value, b"after-reconnect");
        let mut resumed = restored
            .watch(p::WatchRequest {
                key: b"sa16/watch/a".to_vec(),
                start_revision: committed.revision,
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        let replayed = tokio::time::timeout(Duration::from_secs(10), resumed.message())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let kv = replayed.events[0].kv.as_ref().unwrap();
        assert_eq!(kv.value, b"after-reconnect");
        assert_eq!(kv.create_revision, committed.revision);
        assert_eq!(kv.mod_revision, committed.revision);
        assert_eq!(kv.version, 1);
        records.push(json!({"rust":is_rust,"scenario":"wal-restart","state":format!("{persisted:?}"),"replayed":format!("{replayed:?}")}));
        save(&records);
        observed.push(json!({"wal_value":kv.value,"version":kv.version}));
        drop(resumed);
        if is_rust {
            assert_eq!(expected.as_ref().unwrap(), &observed);
        } else {
            expected = Some(observed);
        }
        server.stop().unwrap();
    }
}
