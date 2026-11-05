// Copyright 2026 AsterSQL.
use crate::harness::Server;
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_008_behavior() {
    let rust = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut s = Server::start_rust(&rust).unwrap();
    s.ready().unwrap();
    let mut c = PdClient::connect(s.url.clone()).await.unwrap();
    let id = c
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let h = c
        .get_gc_state(pdpb::GetGcStateRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: id,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .expect("state query must reach bootstrap validation")
        .into_inner()
        .header;
    assert_eq!(
        h.unwrap().error.unwrap().r#type,
        pdpb::ErrorType::NotBootstrapped as i32
    );
    s.stop().unwrap();
}

use crate::txn_safe_point_oracle_test::records;
use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
use astersql_pd_storage_domain::{
    BarrierTime, Base, GCBarrier, GlobalGCBarrier, Keyspace, KeyspaceIdentity, MetaStore,
    StorageEndpoint,
};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
#[path = "../../pkg/storage/kv/response_gate_test.rs"]
mod response_gate;
fn header(id: u64) -> Option<pdpb::RequestHeader> {
    Some(pdpb::RequestHeader {
        cluster_id: id,
        ..Default::default()
    })
}
fn scope(id: u32) -> Option<pdpb::KeyspaceScope> {
    if id == NULL_KEYSPACE_ID {
        None
    } else {
        Some(pdpb::KeyspaceScope {
            keyspace: Some(pdpb::keyspace_scope::Keyspace::KeyspaceId(id)),
        })
    }
}
fn req(id: u64, ks: u32, exclude: bool, globals: bool) -> pdpb::GetGcStateRequest {
    pdpb::GetGcStateRequest {
        header: header(id),
        keyspace_scope: scope(ks),
        exclude_gc_barriers: exclude,
        include_global_gc_barriers: globals,
    }
}
fn http(url: &str, path: &str) -> (u16, Value) {
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "5",
            "-w",
            "\n%{http_code}",
            &format!("{url}/pd/api/v2/gc/safepoint/{path}"),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    let (b, s) = text.rsplit_once('\n').unwrap();
    (s.parse().unwrap(), serde_json::from_str(b).unwrap())
}
fn legacy_http(url: &str, method: &str, path: &str) -> (u16, Value) {
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "5",
            "-X",
            method,
            "-w",
            "\n%{http_code}",
            &format!("{url}/pd/api/v1/gc/safepoint{path}"),
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    let (b, s) = text.rsplit_once('\n').unwrap();
    (
        s.parse().unwrap(),
        serde_json::from_str(b).unwrap_or_else(|_| json!(b)),
    )
}
fn normalize_header(h: &mut Option<pdpb::ResponseHeader>, id: u64) {
    assert_eq!(h.as_ref().unwrap().cluster_id, id);
    h.as_mut().unwrap().cluster_id = 0;
}
fn unchanged(a: &Value, b: &Value) {
    for n in 0..6 {
        assert_eq!(a[n]["entries"], b[n]["entries"]);
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_008_dual_matrix() {
    assert_eq!(std::env::var("PD_COMPAT_MODE").as_deref(), Ok("dual"));
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut compared = vec![];
    let mut reports = vec![];
    for is_rust in [false, true] {
        let mut s = if is_rust {
            Server::start_rust(&rust)
        } else {
            Server::start(&go)
        }
        .unwrap();
        s.ready().unwrap();
        let mut c = PdClient::connect(s.url.clone()).await.unwrap();
        let id = c
            .get_members(pdpb::GetMembersRequest::default())
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .cluster_id;
        let store: Arc<dyn MetaStore> = Arc::new(
            astersql_pd_storage_kv::EtcdMetaStore::connect(
                &[s.metadata_url().to_string()],
                Duration::from_secs(2),
            )
            .await
            .unwrap(),
        );
        let p = ClusterPaths::new(id);
        let base = Base::new(store.as_ref());
        let mut values = vec![];
        let mut raw = vec![];
        let r = c
            .get_gc_state(req(id, 2, false, true))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(
            r.header.unwrap().error.unwrap().r#type,
            pdpb::ErrorType::NotBootstrapped as i32
        );
        let r = c
            .get_all_keyspaces_gc_states(pdpb::GetAllKeyspacesGcStatesRequest {
                header: header(id),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(
            r.header.unwrap().error.unwrap().r#type,
            pdpb::ErrorType::NotBootstrapped as i32
        );
        let h = http(&s.url, "garbage");
        assert_eq!(h.0, 500);
        values.push(json!({"unbootstrapped_http":h}));
        let legacy_unboot = legacy_http(&s.url, "GET", "");
        assert_eq!(legacy_unboot.0, 200);
        values.push(json!({"legacy_unbootstrapped":legacy_unboot}));
        let keys = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
        let mut tx = base.begin_txn();
        for ks in 0..105 {
            let meta = Keyspace {
                identity: Some(KeyspaceIdentity::Id(ks)),
                name: if ks == 0 {
                    "DEFAULT".into()
                } else {
                    format!("sa008_{ks}")
                },
                state: if ks == 3 { 2 } else { 0 },
                config: if ks != 0 && ks != 4 {
                    std::collections::HashMap::from([(
                        "gc_management_type".into(),
                        "keyspace_level".into(),
                    )])
                } else {
                    Default::default()
                },
                ..Default::default()
            };
            keys.save_keyspace_meta(&mut tx, &p, &meta).unwrap();
        }
        tx.commit::<String>().await.unwrap();
        let ep = StorageEndpoint::new(store.clone());
        let mut tx = ep.begin_gc_state_transaction(&p).await.unwrap();
        for ks in [NULL_KEYSPACE_ID, 2, 3, 4] {
            tx.batch.set_txn_safe_point(ks, 100);
            tx.batch.set_gc_safe_point(ks, 20);
            for (name, expiration) in [
                ("forever", None),
                ("expired", Some(BarrierTime::from_unix(1, 0))),
                ("gc_worker", None),
            ] {
                tx.batch
                    .set_gc_barrier(
                        ks,
                        &GCBarrier {
                            barrier_id: name.into(),
                            barrier_ts: 110,
                            expiration_time: expiration,
                        },
                    )
                    .unwrap();
            }
        }
        tx.batch
            .set_global_gc_barrier(&GlobalGCBarrier::new("global", 200, None))
            .unwrap();
        tx.commit().await.unwrap();
        let boot = c
            .bootstrap(pdpb::BootstrapRequest {
                header: header(id),
                store: Some(astersql_pd_proto::metapb::Store {
                    id: 1,
                    version: "8.5.0".into(),
                    ..Default::default()
                }),
                region: Some(astersql_pd_proto::metapb::Region {
                    id: 2,
                    peers: vec![astersql_pd_proto::metapb::Peer {
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
        values.push(json!({"seed_physical":crate::txn_safe_point_oracle_test::bytes(&records(store.as_ref(),&p,2).await)}));
        let list = legacy_http(&s.url, "GET", "");
        assert_eq!(list.0, 200, "{list:?}");
        assert_eq!(list.1["gc_safe_point"], 20);
        assert_eq!(list.1["min_service_gc_safe_point"], 100);
        values.push(json!({"legacy_list":list}));
        for ks in [NULL_KEYSPACE_ID, 0, 2, 3, 4, 999, 0x1000000, u32::MAX] {
            for exclude in [false, true] {
                for globals in [false, true] {
                    let before = records(store.as_ref(), &p, 2).await;
                    let mut r = c
                        .get_gc_state(req(id, ks, exclude, globals))
                        .await
                        .unwrap()
                        .into_inner();
                    normalize_header(&mut r.header, id);
                    assert_eq!(
                        r.header.as_ref().unwrap().error.is_some(),
                        ks == 999,
                        "{r:?}"
                    );
                    if let Some(ref state) = r.gc_state {
                        assert_eq!(
                            state.gc_safe_point,
                            if ks == 2
                                || ks == 3
                                || ks == NULL_KEYSPACE_ID
                                || ks == 0
                                || ks == 4
                                || ks > 0xffffff
                            {
                                20
                            } else {
                                0
                            }
                        );
                        if exclude {
                            assert!(state.gc_barriers.is_empty());
                        } else {
                            assert_eq!(state.gc_barriers.len(), 2);
                            assert!(
                                state
                                    .gc_barriers
                                    .iter()
                                    .all(|b| b.barrier_id != "gc_worker")
                            );
                        }
                    }
                    unchanged(&before, &records(store.as_ref(), &p, 2).await);
                    values.push(json!({"single":[ks,exclude as u32,globals as u32],"response":format!("{r:?}")}));
                }
            }
        }
        for path in [
            "2",
            "3",
            "4",
            "0",
            "4294967295",
            "16777216",
            "999",
            "abc",
            "-1",
            "4294967296",
            "+2",
            "02",
        ] {
            let h = http(&s.url, path);
            assert_eq!(
                h.0,
                if ["abc", "-1", "4294967296", "+2"].contains(&path) {
                    400
                } else if path == "999" {
                    500
                } else {
                    200
                },
                "{path}: {h:?}"
            );
            if h.0 == 200 {
                assert_eq!(h.1["safe_point"], 20);
            }
            values.push(json!({"http":path,"response":h}));
        }
        // Force delete delegates to the existing legacy domain operation, including reserved and missing names.
        for name in ["forever", "gc_worker", "missing"] {
            let before = records(store.as_ref(), &p, NULL_KEYSPACE_ID).await;
            let r = legacy_http(&s.url, "DELETE", &format!("/{name}"));
            assert_eq!(r, (200, json!("Delete service GC safepoint successfully.")));
            let after = records(store.as_ref(), &p, NULL_KEYSPACE_ID).await;
            assert!(
                ep.load_gc_barrier(&p, NULL_KEYSPACE_ID, name)
                    .await
                    .unwrap()
                    .is_none()
            );
            values.push(json!({"legacy_delete":name,"response":r,"bytes":crate::txn_safe_point_oracle_test::bytes(&after)}));
            raw.push(json!({"legacy_delete":name,"before":before,"after":after}));
        }
        let list = legacy_http(&s.url, "GET", "");
        assert_eq!(list.0, 200);
        values.push(json!({"legacy_after_delete":list}));
        for exclude in [false, true] {
            for exclude_global in [false, true] {
                let before = records(store.as_ref(), &p, 2).await;
                let mut r = c
                    .get_all_keyspaces_gc_states(pdpb::GetAllKeyspacesGcStatesRequest {
                        header: header(id),
                        exclude_gc_barriers: exclude,
                        exclude_global_gc_barriers: exclude_global,
                    })
                    .await
                    .unwrap()
                    .into_inner();
                normalize_header(&mut r.header, id);
                assert!(r.header.as_ref().unwrap().error.is_none(), "{r:?}");
                r.gc_states
                    .sort_by_key(|s| format!("{:?}", s.keyspace_scope));
                assert_eq!(r.gc_states.len(), 105);
                assert_eq!(r.global_gc_barriers.len(), usize::from(!exclude_global));
                unchanged(&before, &records(store.as_ref(), &p, 2).await);
                values.push(json!({"all":[exclude,exclude_global],"response":format!("{r:?}")}));
            }
        }
        for all in [false, true] {
            let e = if all {
                c.get_all_keyspaces_gc_states(pdpb::GetAllKeyspacesGcStatesRequest {
                    header: header(id + 1),
                    ..Default::default()
                })
                .await
                .unwrap_err()
            } else {
                c.get_gc_state(req(id + 1, 2, false, true))
                    .await
                    .unwrap_err()
            };
            assert_eq!(e.code(), tonic::Code::FailedPrecondition);
            raw.push(json!({"wrong_cluster":all,"error":format!("{e:?}")}));
        }
        // Read TTLs are remaining lifetimes: retain finite values and bound them by the wall clock.
        for ttl in [1, 60, 3600] {
            let r = c
                .set_gc_barrier(pdpb::SetGcBarrierRequest {
                    header: header(id),
                    keyspace_scope: scope(2),
                    barrier_id: "finite".into(),
                    barrier_ts: 120,
                    ttl_seconds: ttl,
                })
                .await
                .unwrap()
                .into_inner();
            assert!(r.header.unwrap().error.is_none());
            let single = c
                .get_gc_state(req(id, 2, false, true))
                .await
                .unwrap()
                .into_inner();
            let b = single
                .gc_state
                .unwrap()
                .gc_barriers
                .into_iter()
                .find(|b| b.barrier_id == "finite")
                .unwrap();
            assert!(b.ttl_seconds >= ttl - 2 && b.ttl_seconds <= ttl);
            let r = c
                .get_all_keyspaces_gc_states(pdpb::GetAllKeyspacesGcStatesRequest {
                    header: header(id),
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner();
            let b = r
                .gc_states
                .into_iter()
                .flat_map(|s| s.gc_barriers)
                .find(|b| b.barrier_id == "finite")
                .unwrap();
            assert!(b.ttl_seconds >= ttl - 2 && b.ttl_seconds <= ttl);
            raw.push(json!({"finite_ttl":ttl,"barrier":format!("{b:?}"),"state":records(store.as_ref(),&p,2).await}));
        }
        // Real revision CAS conflict: read-with-barriers must validate its transaction even though it has no writes.
        base.save(p.gc_state_revision_path().as_bytes(), b"")
            .await
            .unwrap();
        for all in [false, true] {
            let mut h = if all {
                c.get_all_keyspaces_gc_states(pdpb::GetAllKeyspacesGcStatesRequest {
                    header: header(id),
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner()
                .header
            } else {
                c.get_gc_state(req(id, 2, false, true))
                    .await
                    .unwrap()
                    .into_inner()
                    .header
            };
            assert!(h.as_ref().unwrap().error.is_some(), "{h:?}");
            normalize_header(&mut h, id);
            values.push(json!({"cas":all,"header":format!("{h:?}")}));
            raw.push(json!({"cas":all,"header":format!("{h:?}"),"state":records(store.as_ref(),&p,2).await}));
        }
        for (method, path) in [("GET", ""), ("DELETE", "/missing")] {
            let before = records(store.as_ref(), &p, NULL_KEYSPACE_ID).await;
            let r = legacy_http(&s.url, method, path);
            assert_eq!(r.0, 500);
            values.push(json!({"legacy_transaction_error":method,"response":r}));
            unchanged(
                &before,
                &records(store.as_ref(), &p, NULL_KEYSPACE_ID).await,
            );
        }
        base.save(p.gc_state_revision_path().as_bytes(), b"999")
            .await
            .unwrap();
        let before = records(store.as_ref(), &p, 2).await;
        // Corrupt uncached disabled scope, then repair; excludes use caches only where contract allows.
        base.save(p.txn_safe_point_path(3).as_bytes(), b"broken")
            .await
            .unwrap();
        let mut e = c
            .get_gc_state(req(id, 3, false, false))
            .await
            .unwrap()
            .into_inner();
        assert!(e.header.as_ref().unwrap().error.is_some());
        normalize_header(&mut e.header, id);
        values.push(json!({"corrupt_state":format!("{e:?}")}));
        base.save(p.txn_safe_point_path(3).as_bytes(), b"100")
            .await
            .unwrap();
        let repaired = c
            .get_gc_state(req(id, 3, false, false))
            .await
            .unwrap()
            .into_inner();
        assert!(repaired.header.as_ref().unwrap().error.is_none());
        raw.push(json!({"repaired":format!("{repaired:?}")}));
        let durable = records(store.as_ref(), &p, 2).await;
        let legacy_durable = records(store.as_ref(), &p, NULL_KEYSPACE_ID).await;
        drop(c);
        s.stop().unwrap();
        if is_rust {
            s.restart_rust(&rust).unwrap();
        } else {
            s.restart_with_args(&go, &[]).unwrap();
        }
        s.ready().unwrap();
        c = PdClient::connect(s.url.clone()).await.unwrap();
        let recovered = c
            .get_gc_state(req(id, 3, false, false))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(repaired.gc_state, recovered.gc_state);
        unchanged(&durable, &records(store.as_ref(), &p, 2).await);
        raw.push(json!({"wal_before":durable,"wal_after":records(store.as_ref(),&p,2).await,"recovered":format!("{recovered:?}")}));
        unchanged(
            &legacy_durable,
            &records(store.as_ref(), &p, NULL_KEYSPACE_ID).await,
        );
        raw.push(json!({"legacy_wal_before":legacy_durable,"legacy_wal_after":records(store.as_ref(),&p,NULL_KEYSPACE_ID).await}));
        // A lost read response has no storage side effect.
        for all in [false, true] {
            use prost::Message;
            let gate = response_gate::ResponseGate::start(&s.url).await;
            let mut lost = PdClient::connect(gate.endpoint.clone()).await.unwrap();
            lost.get_members(pdpb::GetMembersRequest::default())
                .await
                .unwrap();
            let task = if all {
                let r = pdpb::GetAllKeyspacesGcStatesRequest {
                    header: header(id),
                    ..Default::default()
                };
                gate.arm_on_key(&r.encode_to_vec());
                let mut r = tonic::Request::new(r);
                r.set_timeout(Duration::from_secs(1));
                tokio::spawn(async move { lost.get_all_keyspaces_gc_states(r).await.map(|_| ()) })
            } else {
                let r = req(id, 2, false, true);
                gate.arm_on_key(&r.encode_to_vec());
                let mut r = tonic::Request::new(r);
                r.set_timeout(Duration::from_secs(1));
                tokio::spawn(async move { lost.get_gc_state(r).await.map(|_| ()) })
            };
            tokio::time::timeout(Duration::from_secs(3), gate.held.notified())
                .await
                .unwrap();
            let e = task.await.unwrap().unwrap_err();
            raw.push(json!({"lost_response":all,"error":format!("{e:?}")}));
            gate.release();
            gate.close().await;
        }
        {
            let _pause = s.suspend();
            let mut a = tonic::Request::new(req(id, 2, false, true));
            a.set_timeout(Duration::from_millis(30));
            let mut b = tonic::Request::new(pdpb::GetAllKeyspacesGcStatesRequest {
                header: header(id),
                ..Default::default()
            });
            b.set_timeout(Duration::from_millis(30));
            for e in [
                c.get_gc_state(a).await.unwrap_err(),
                c.get_all_keyspaces_gc_states(b).await.unwrap_err(),
            ] {
                assert_eq!(e.code(), tonic::Code::Cancelled);
                assert_eq!(e.message(), "Timeout expired");
                raw.push(json!({"cancel":format!("{e:?}")}));
            }
        }
        let mut follower = if is_rust {
            Server::start_with_args(
                &rust,
                &[
                    &format!("--etcd-endpoints={}", s.metadata_url()),
                    "--name=follower",
                ],
            )
        } else {
            Server::start_with_args(
                &go,
                &[
                    &format!("--join={}", s.url),
                    "--initial-cluster=",
                    "--name=follower",
                ],
            )
        }
        .unwrap();
        follower.ready().unwrap();
        let mut fc = PdClient::connect(follower.url.clone()).await.unwrap();
        for e in [
            fc.get_gc_state(req(id, 2, false, true)).await.unwrap_err(),
            fc.get_all_keyspaces_gc_states(pdpb::GetAllKeyspacesGcStatesRequest {
                header: header(id),
                ..Default::default()
            })
            .await
            .unwrap_err(),
        ] {
            assert_eq!(e.code(), tonic::Code::Unavailable);
            assert_eq!(e.message(), "not leader");
            raw.push(json!({"follower":format!("{e:?}")}));
        }
        drop(fc);
        unchanged(&before, &records(store.as_ref(), &p, 2).await);
        raw.push(json!({"physical":records(store.as_ref(),&p,2).await}));
        follower.stop().unwrap();
        drop(c);
        s.stop().unwrap();
        compared.push(values.clone());
        reports.push(json!({"rust":is_rust,"cluster_id":id,"observations":values,"raw":raw}));
    }
    if let Some(path) = std::env::var_os("PD_SA008_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
    }
    assert_eq!(compared[0], compared[1]);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_008_legacy_behavior() {
    let rust = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut s = Server::start_rust(&rust).unwrap();
    s.ready().unwrap();
    let mut c = PdClient::connect(s.url.clone()).await.unwrap();
    let id = c
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let b = c
        .bootstrap(pdpb::BootstrapRequest {
            header: header(id),
            store: Some(astersql_pd_proto::metapb::Store {
                id: 1,
                version: "8.5.0".into(),
                ..Default::default()
            }),
            region: Some(astersql_pd_proto::metapb::Region {
                id: 2,
                peers: vec![astersql_pd_proto::metapb::Peer {
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
    assert!(b.header.unwrap().error.is_none());
    assert_eq!(legacy_http(&s.url, "GET", "").0, 200);
    assert_eq!(legacy_http(&s.url, "DELETE", "/missing").0, 200);
    s.stop().unwrap();
}
