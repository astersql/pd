// Copyright 2026 AsterSQL.
use crate::harness::Server;
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_005_behavior() {
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
    for txn in [true, false] {
        let header = Some(pdpb::RequestHeader {
            cluster_id: id,
            ..Default::default()
        });
        let h = if txn {
            c.advance_txn_safe_point(pdpb::AdvanceTxnSafePointRequest {
                header,
                ..Default::default()
            })
            .await
            .expect("txn advancement must reach bootstrap validation")
            .into_inner()
            .header
        } else {
            c.advance_gc_safe_point(pdpb::AdvanceGcSafePointRequest {
                header,
                ..Default::default()
            })
            .await
            .expect("GC advancement must reach bootstrap validation")
            .into_inner()
            .header
        };
        assert_eq!(
            h.unwrap().error.unwrap().r#type,
            pdpb::ErrorType::NotBootstrapped as i32
        );
    }
    s.stop().unwrap();
}

use crate::txn_safe_point_oracle_test::{bytes, records};
use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
use astersql_pd_storage_domain::{
    Base, GCBarrier, GlobalGCBarrier, Keyspace, KeyspaceIdentity, MetaStore, StorageEndpoint,
};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
#[path = "../../pkg/storage/kv/response_gate_test.rs"]
mod response_gate;
type Client = PdClient<tonic::transport::Channel>;
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
async fn advance(
    c: &mut Client,
    id: u64,
    ks: u32,
    target: u64,
    txn: bool,
) -> Result<Value, tonic::Status> {
    let (mut h, old, new, blocker) = if txn {
        let r = c
            .advance_txn_safe_point(pdpb::AdvanceTxnSafePointRequest {
                header: header(id),
                keyspace_scope: scope(ks),
                target,
            })
            .await?
            .into_inner();
        (
            r.header.unwrap(),
            r.old_txn_safe_point,
            r.new_txn_safe_point,
            r.blocker_description,
        )
    } else {
        let r = c
            .advance_gc_safe_point(pdpb::AdvanceGcSafePointRequest {
                header: header(id),
                keyspace_scope: scope(ks),
                target,
            })
            .await?
            .into_inner();
        (
            r.header.unwrap(),
            r.old_gc_safe_point,
            r.new_gc_safe_point,
            String::new(),
        )
    };
    assert_eq!(h.cluster_id, id);
    h.cluster_id = 0;
    Ok(
        json!({"header":format!("{h:?}"),"old":old,"new":new,"blocker":blocker,"error":h.error.is_some()}),
    )
}
fn counter(state: &Value) -> u64 {
    let rows = state[0]["entries"].as_array().unwrap();
    if rows.is_empty() {
        0
    } else {
        let b: Vec<u8> = serde_json::from_value(rows[0]["bytes"].clone()).unwrap();
        String::from_utf8(b).unwrap().parse().unwrap()
    }
}
#[track_caller]
fn unchanged(a: &Value, b: &Value) {
    for n in 0..6 {
        assert_eq!(a[n]["entries"], b[n]["entries"]);
    }
}
fn status(e: tonic::Status) -> Value {
    json!({"code":format!("{:?}",e.code()),"message":e.message()})
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_005_dual_matrix() {
    assert_eq!(std::env::var("PD_COMPAT_MODE").as_deref(), Ok("dual"));
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut reports = vec![];
    let mut compared = vec![];
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
        let ep = StorageEndpoint::new(store.clone());
        let base = Base::new(store.as_ref());
        let mut values = vec![];
        let mut raw = vec![];
        for txn in [true, false] {
            let r = advance(&mut c, id, NULL_KEYSPACE_ID, 10, txn)
                .await
                .unwrap();
            assert!(r["error"].as_bool().unwrap());
            values.push(r);
        }
        // Seed actual metadata before first access; the term-owned manager remains the only writer of points.
        let keys = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
        let mut tx = base.begin_txn();
        for (ks, independent) in [(2, true), (3, true), (4, false)] {
            keys.save_keyspace_meta(
                &mut tx,
                &p,
                &Keyspace {
                    identity: Some(KeyspaceIdentity::Id(ks)),
                    name: format!("sa005_{ks}"),
                    config: if independent {
                        std::collections::HashMap::from([(
                            "gc_management_type".into(),
                            "keyspace_level".into(),
                        )])
                    } else {
                        Default::default()
                    },
                    ..Default::default()
                },
            )
            .unwrap();
        }
        tx.commit::<String>().await.unwrap();
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
            })
            .await
            .unwrap()
            .into_inner();
        assert!(boot.header.unwrap().error.is_none());
        let mut tx = ep.begin_gc_state_transaction(&p).await.unwrap();
        tx.batch
            .set_gc_barrier(2, &GCBarrier::new("local", 40, None))
            .unwrap();
        tx.batch
            .set_global_gc_barrier(&GlobalGCBarrier::new("global", 60, None))
            .unwrap();
        let mut expired = astersql_pd_storage_domain::BarrierTime::from_unix(1, 0);
        tx.batch
            .set_gc_barrier(2, &GCBarrier::new("expired", 1, Some(&mut expired)))
            .unwrap();
        tx.commit().await.unwrap();
        for (txn, ks, target, expected_error, expected_new) in [
            (true, NULL_KEYSPACE_ID, 100, false, 60),
            (true, NULL_KEYSPACE_ID, 60, false, 60),
            (true, NULL_KEYSPACE_ID, 59, true, 0),
            (false, NULL_KEYSPACE_ID, 50, false, 50),
            (false, NULL_KEYSPACE_ID, 50, false, 50),
            (false, NULL_KEYSPACE_ID, 49, true, 0),
            (false, NULL_KEYSPACE_ID, 61, true, 0),
            (true, 2, 100, false, 40),
            (false, 2, 40, false, 40),
            (false, 2, 41, true, 0),
            (true, 3, 55, false, 55),
            (false, 3, 30, false, 30),
            (true, 4, 100, true, 0),
            (false, 4, 10, true, 0),
            (true, 999, 100, true, 0),
            (false, 999, 10, true, 0),
            (true, 0x1000000, 60, false, 60),
            (false, 0x1000000, 50, false, 50),
            (true, NULL_KEYSPACE_ID, u64::MAX, false, 60),
        ] {
            let physical = if ks > 0xffffff { NULL_KEYSPACE_ID } else { ks };
            let before = records(store.as_ref(), &p, physical).await;
            let r = advance(&mut c, id, ks, target, txn).await.unwrap();
            assert_eq!(r["error"], expected_error, "{txn}/{ks}/{target}: {r}");
            assert_eq!(r["new"], expected_new);
            let after = records(store.as_ref(), &p, physical).await;
            if expected_error {
                unchanged(&before, &after);
            } else {
                assert_eq!(counter(&after), counter(&before) + 1);
                let row = if txn { 1 } else { 2 };
                assert_eq!(
                    after[0]["entries"][0]["mod_revision"],
                    after[row]["entries"][0]["mod_revision"]
                );
            }
            values.push(
                json!({"txn":txn,"ks":ks,"target":target,"response":r,"bytes":bytes(&after)}),
            );
            raw.push(json!({"before":before,"after":after}));
        }
        assert!(
            ep.load_gc_barrier_with_service_id(&p, 2, b"expired")
                .await
                .unwrap()
                .is_none()
        );
        // A legacy min-start older than the current point cannot force regression.
        let min = format!("{}server", p.compatible_tidb_min_start_ts_prefix(2));
        base.save(min.as_bytes(), b"30").await.unwrap();
        let r = advance(&mut c, id, 2, 100, true).await.unwrap();
        assert_eq!(r["new"], 40);
        values.push(json!({"old_min_start":r,"state":bytes(&records(store.as_ref(),&p,2).await)}));
        base.remove(min.as_bytes()).await.unwrap();
        // Supply identical metadata as in sa004; automatic default-keyspace bootstrap
        // belongs to the keyspace host acceptance, not this GC fixture.
        let mut tx = base.begin_txn();
        keys.save_keyspace_meta(
            &mut tx,
            &p,
            &Keyspace {
                identity: Some(KeyspaceIdentity::Id(0)),
                name: "DEFAULT".into(),
                ..Default::default()
            },
        )
        .unwrap();
        tx.commit::<String>().await.unwrap();
        // Empty and V3 scopes use the Go getter's zero ID, while absent scope means Null.
        for keyspace in [
            None,
            Some(pdpb::keyspace_scope::Keyspace::KeyspaceIdentity(
                Default::default(),
            )),
        ] {
            for txn in [true, false] {
                let scope = Some(pdpb::KeyspaceScope { keyspace });
                let before = records(store.as_ref(), &p, 0).await;
                let (r, error) = if txn {
                    let mut r = c
                        .advance_txn_safe_point(pdpb::AdvanceTxnSafePointRequest {
                            header: header(id),
                            keyspace_scope: scope,
                            target: 10,
                        })
                        .await
                        .unwrap()
                        .into_inner();
                    assert_eq!(r.header.as_ref().unwrap().cluster_id, id);
                    r.header.as_mut().unwrap().cluster_id = 0;
                    (format!("{r:?}"), r.header.unwrap().error.is_some())
                } else {
                    let mut r = c
                        .advance_gc_safe_point(pdpb::AdvanceGcSafePointRequest {
                            header: header(id),
                            keyspace_scope: scope,
                            target: 10,
                        })
                        .await
                        .unwrap()
                        .into_inner();
                    assert_eq!(r.header.as_ref().unwrap().cluster_id, id);
                    r.header.as_mut().unwrap().cluster_id = 0;
                    (format!("{r:?}"), r.header.unwrap().error.is_some())
                };
                let after = records(store.as_ref(), &p, 0).await;
                if error {
                    unchanged(&before, &after);
                } else {
                    assert_eq!(counter(&after), counter(&before) + 1);
                }
                values
                    .push(json!({"special_scope":format!("{keyspace:?}"),"txn":txn,"response":r,"bytes":bytes(&after)}));
                raw.push(json!({"special_scope_before":before,"after":after}));
            }
        }
        // Remove real barriers, advance past them, and exercise the full unsigned boundary.
        base.remove(p.global_gc_barrier_path("global").as_bytes())
            .await
            .unwrap();
        base.remove(p.gc_barrier_path(2, "local").as_bytes())
            .await
            .unwrap();
        for (txn, ks, target) in [
            (true, 2, 100),
            (false, 2, 100),
            (true, NULL_KEYSPACE_ID, 100),
            (true, 3, u64::MAX),
            (false, 3, u64::MAX),
            (false, 3, u64::MAX - 1),
        ] {
            let r = advance(&mut c, id, ks, target, txn).await.unwrap();
            values.push(json!({"txn":txn,"ks":ks,"target":target,"response":r,"bytes":bytes(&records(store.as_ref(),&p,ks).await)}));
        }
        // Empty-but-present revision exercises actual etcd CAS, after both scopes are warm.
        let revision = base
            .load(p.gc_state_revision_path().as_bytes())
            .await
            .unwrap();
        base.save(p.gc_state_revision_path().as_bytes(), b"")
            .await
            .unwrap();
        for txn in [true, false] {
            let before = records(store.as_ref(), &p, 2).await;
            let r = advance(&mut c, id, 2, 100, txn).await.unwrap();
            assert_eq!(r["error"], true);
            let after = records(store.as_ref(), &p, 2).await;
            unchanged(&before, &after);
            values.push(json!({"conflict":txn,"response":r}));
            raw.push(json!({"before":before,"after":after}));
        }
        base.save(p.gc_state_revision_path().as_bytes(), &revision)
            .await
            .unwrap();
        for txn in [true, false] {
            let e = advance(&mut c, 0, 2, 100, txn).await.unwrap_err();
            assert_eq!(e.code(), tonic::Code::FailedPrecondition);
            assert_eq!(
                e.message(),
                format!("mismatch cluster id, need {id} but got 0")
            );
            values.push(json!({"wrong_cluster":txn,"verified":true}));
        }
        let durable = [
            records(store.as_ref(), &p, NULL_KEYSPACE_ID).await,
            records(store.as_ref(), &p, 2).await,
            records(store.as_ref(), &p, 3).await,
        ];
        drop(c);
        drop(base);
        drop(keys);
        drop(ep);
        drop(store);
        if is_rust {
            s.restart_rust(&rust).unwrap()
        } else {
            s.restart_with_args(&go, &[]).unwrap()
        };
        s.ready().unwrap();
        c = PdClient::connect(s.url.clone()).await.unwrap();
        let store: Arc<dyn MetaStore> = Arc::new(
            astersql_pd_storage_kv::EtcdMetaStore::connect(
                &[s.metadata_url().to_string()],
                Duration::from_secs(2),
            )
            .await
            .unwrap(),
        );
        for (n, ks) in [NULL_KEYSPACE_ID, 2, 3].into_iter().enumerate() {
            let after = records(store.as_ref(), &p, ks).await;
            unchanged(&durable[n], &after);
            raw.push(json!({"wal_before":durable[n],"wal_after":after}));
        }
        // A lost RPC response is not a transaction rollback: independently read the committed row.
        for txn in [true, false] {
            use prost::Message;
            let gate = response_gate::ResponseGate::start(&s.url).await;
            let mut lost = PdClient::connect(gate.endpoint.clone()).await.unwrap();
            lost.get_members(pdpb::GetMembersRequest::default())
                .await
                .unwrap();
            let request = if txn {
                let r = pdpb::AdvanceTxnSafePointRequest {
                    header: header(id),
                    keyspace_scope: None,
                    target: 110,
                };
                gate.arm_on_key(&r.encode_to_vec());
                let mut r = tonic::Request::new(r);
                r.set_timeout(Duration::from_secs(2));
                tokio::spawn(async move { lost.advance_txn_safe_point(r).await.map(|_| ()) })
            } else {
                let r = pdpb::AdvanceGcSafePointRequest {
                    header: header(id),
                    keyspace_scope: None,
                    target: 60,
                };
                gate.arm_on_key(&r.encode_to_vec());
                let mut r = tonic::Request::new(r);
                r.set_timeout(Duration::from_secs(2));
                tokio::spawn(async move { lost.advance_gc_safe_point(r).await.map(|_| ()) })
            };
            tokio::time::timeout(Duration::from_secs(3), gate.held.notified())
                .await
                .unwrap();
            let ep = StorageEndpoint::new(store.clone());
            let actual = tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    let actual = if txn {
                        ep.load_txn_safe_point(&p, NULL_KEYSPACE_ID).await.unwrap()
                    } else {
                        ep.load_gc_safe_point(&p, NULL_KEYSPACE_ID).await.unwrap()
                    };
                    if actual == if txn { 110 } else { 60 } {
                        break actual;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            let e = request.await.unwrap().unwrap_err();
            values.push(json!({"lost_reply":txn,"status":status(e),"actual":actual}));
            raw.push(
                json!({"lost_reply":txn,"state":records(store.as_ref(),&p,NULL_KEYSPACE_ID).await}),
            );
            gate.release();
            gate.close().await;
        }
        let before = records(store.as_ref(), &p, 2).await;
        {
            let _pause = s.suspend();
            let mut a = tonic::Request::new(pdpb::AdvanceTxnSafePointRequest {
                header: header(id),
                keyspace_scope: scope(2),
                target: 0,
            });
            a.set_timeout(Duration::from_millis(30));
            let mut b = tonic::Request::new(pdpb::AdvanceGcSafePointRequest {
                header: header(id),
                keyspace_scope: scope(2),
                target: 0,
            });
            b.set_timeout(Duration::from_millis(30));
            for e in [
                c.advance_txn_safe_point(a).await.unwrap_err(),
                c.advance_gc_safe_point(b).await.unwrap_err(),
            ] {
                values.push(json!({"deadline":status(e)}));
            }
        }
        c.get_members(pdpb::GetMembersRequest::default())
            .await
            .unwrap();
        unchanged(&before, &records(store.as_ref(), &p, 2).await);
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
        for txn in [true, false] {
            let e = advance(&mut fc, id, 2, 100, txn).await.unwrap_err();
            assert_eq!(e.code(), tonic::Code::Unavailable);
            assert_eq!(e.message(), "not leader");
            values.push(json!({"follower":txn,"status":status(e)}));
        }
        unchanged(&before, &records(store.as_ref(), &p, 2).await);
        drop(fc);
        follower.stop().unwrap();
        drop(c);
        s.stop().unwrap();
        reports.push(json!({"rust":is_rust,"cluster_id":id,"raw":raw,"observations":values}));
        compared.push(values);
    }
    if let Some(path) = std::env::var_os("PD_SA005_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
    }
    assert_eq!(compared[0], compared[1]);
}
