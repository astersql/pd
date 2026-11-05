// Copyright 2026 AsterSQL.
use crate::harness::Server;
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_006_behavior() {
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
        .set_gc_barrier(pdpb::SetGcBarrierRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: id,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .expect("barrier must reach bootstrap validation")
        .into_inner()
        .header;
    assert_eq!(
        h.unwrap().error.unwrap().r#type,
        pdpb::ErrorType::NotBootstrapped as i32
    );
    s.stop().unwrap();
}

use crate::txn_safe_point_oracle_test::{bytes, records};
use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
use astersql_pd_storage_domain::{Base, Keyspace, KeyspaceIdentity, MetaStore};
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
fn req(id: u64, ks: u32, name: &str, ts: u64, ttl: i64) -> pdpb::SetGcBarrierRequest {
    pdpb::SetGcBarrierRequest {
        header: header(id),
        keyspace_scope: scope(ks),
        barrier_id: name.into(),
        barrier_ts: ts,
        ttl_seconds: ttl,
    }
}
fn del(id: u64, ks: u32, name: &str) -> pdpb::DeleteGcBarrierRequest {
    pdpb::DeleteGcBarrierRequest {
        header: header(id),
        keyspace_scope: scope(ks),
        barrier_id: name.into(),
    }
}
async fn call(
    c: &mut Client,
    id: u64,
    ks: u32,
    name: &str,
    ts: u64,
    ttl: i64,
    set: bool,
) -> Result<(String, bool, Option<pdpb::GcBarrierInfo>), tonic::Status> {
    let (mut h, b) = if set {
        let r = c
            .set_gc_barrier(req(id, ks, name, ts, ttl))
            .await?
            .into_inner();
        (r.header.unwrap(), r.new_barrier_info)
    } else {
        let r = c.delete_gc_barrier(del(id, ks, name)).await?.into_inner();
        (r.header.unwrap(), r.deleted_barrier_info)
    };
    assert_eq!(h.cluster_id, id);
    h.cluster_id = 0;
    let error = h.error.is_some();
    Ok((format!("{h:?}"), error, b))
}
fn unchanged(a: &Value, b: &Value) {
    for n in 0..6 {
        assert_eq!(a[n]["entries"], b[n]["entries"]);
    }
}
fn counter(a: &Value) -> u64 {
    let a = a[0]["entries"].as_array().unwrap();
    if a.is_empty() {
        0
    } else {
        String::from_utf8(serde_json::from_value(a[0]["bytes"].clone()).unwrap())
            .unwrap()
            .parse()
            .unwrap()
    }
}
fn seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_006_dual_matrix() {
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
        for set in [true, false] {
            let (h, error, b) = call(&mut c, id, 2, "a", 30, i64::MAX, set).await.unwrap();
            assert!(error);
            assert!(h.contains("NotBootstrapped"));
            raw.push(json!({"unbootstrapped":set,"header":h,"barrier":format!("{b:?}")}));
        }
        let keys = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
        let mut tx = base.begin_txn();
        for (ks, independent) in [(0, false), (2, true), (3, true), (4, false)] {
            keys.save_keyspace_meta(
                &mut tx,
                &p,
                &Keyspace {
                    identity: Some(KeyspaceIdentity::Id(ks)),
                    name: if ks == 0 {
                        "DEFAULT".into()
                    } else {
                        format!("sa006_{ks}")
                    },
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
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        assert!(boot.header.unwrap().error.is_none());
        for ks in [NULL_KEYSPACE_ID, 2, 3] {
            let r = c
                .advance_txn_safe_point(pdpb::AdvanceTxnSafePointRequest {
                    header: header(id),
                    keyspace_scope: scope(ks),
                    target: 20,
                })
                .await
                .unwrap()
                .into_inner();
            assert!(r.header.unwrap().error.is_none());
        }
        for (set, ks, name, ts, ttl, error) in [
            (true, 2, "a", 30, i64::MAX, false),
            (true, 3, "a", 80, i64::MAX, false),
            (true, 2, "a", 40, i64::MAX, false),
            (true, 2, "a", 25, i64::MAX, false),
            (true, 2, "a", 20, i64::MAX, false),
            (true, 2, "a", 19, i64::MAX, true),
            (true, 2, "a", 30, 0, true),
            (true, 2, "a", 30, -1, true),
            (true, 999, "gc_worker", 30, 0, true),
            (true, 2, "", 30, 60, true),
            (true, 2, "gc_worker", 30, 60, true),
            (true, 999, "gc_worker", 30, 60, true),
            (true, 4, "unified", 60, i64::MAX, false),
            (true, NULL_KEYSPACE_ID, "global-scope", 40, i64::MAX, false),
            (true, 0x1000000, "alias", 50, i64::MAX, false),
            (true, 2, "服务/a", u64::MAX, i64::MAX, false),
            (false, 2, "gc_worker", 0, 0, true),
            (false, 2, "", 0, 0, true),
            (false, 999, "a", 0, 0, true),
            (false, 2, "missing", 0, 0, false),
            (false, 2, "a", 0, 0, false),
            (false, 2, "a", 0, 0, false),
            (false, 4, "unified", 0, 0, false),
            (false, 0x1000000, "alias", 0, 0, false),
        ] {
            let physical = if ks > 0xffffff || ks == 4 {
                NULL_KEYSPACE_ID
            } else {
                ks
            };
            let before = records(store.as_ref(), &p, physical).await;
            let other = records(store.as_ref(), &p, 3).await;
            let (h, e, b) = call(&mut c, id, ks, name, ts, ttl, set).await.unwrap();
            assert_eq!(e, error, "{set}/{ks}/{name}/{ts}/{ttl}: {h}");
            let after = records(store.as_ref(), &p, physical).await;
            if error {
                unchanged(&before, &after)
            } else {
                assert_eq!(counter(&after), counter(&before) + 1);
                if set {
                    let b = b.as_ref().unwrap();
                    assert_eq!(b.barrier_id, name);
                    assert_eq!(b.barrier_ts, ts);
                    assert_eq!(b.ttl_seconds, i64::MAX);
                    let key = p.gc_barrier_path(physical, name);
                    let row = after[3]["entries"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|e| {
                            let k: Vec<u8> = serde_json::from_value(e["key"].clone()).unwrap();
                            k == key.as_bytes()
                        })
                        .unwrap();
                    assert_eq!(row["mod_revision"], after[0]["entries"][0]["mod_revision"]);
                }
            }
            if ks != 3 {
                assert_eq!(
                    other[3]["entries"],
                    records(store.as_ref(), &p, 3).await[3]["entries"]
                );
            }
            values.push(json!({"set":set,"ks":ks,"name":name,"ts":ts,"ttl":ttl,"header":h,"barrier":format!("{b:?}"),"bytes":bytes(&after)}));
            raw.push(json!({"before":before,"after":after}));
        }
        // Empty/V3 scopes use the existing Go getter's ID zero, including unified redirection.
        for keyspace in [
            None,
            Some(pdpb::keyspace_scope::Keyspace::KeyspaceIdentity(
                Default::default(),
            )),
        ] {
            let mut request = req(id, 0, "special", 31, i64::MAX);
            request.keyspace_scope = Some(pdpb::KeyspaceScope { keyspace });
            let mut r = c
                .set_gc_barrier(request.clone())
                .await
                .unwrap()
                .into_inner();
            assert!(r.header.as_ref().unwrap().error.is_none());
            r.header.as_mut().unwrap().cluster_id = 0;
            values.push(json!({"scope_set":format!("{r:?}"),"bytes":bytes(&records(store.as_ref(),&p,NULL_KEYSPACE_ID).await)}));
            let mut r = c
                .delete_gc_barrier(pdpb::DeleteGcBarrierRequest {
                    header: header(id),
                    keyspace_scope: request.keyspace_scope,
                    barrier_id: "special".into(),
                })
                .await
                .unwrap()
                .into_inner();
            assert!(r.header.as_ref().unwrap().error.is_none());
            r.header.as_mut().unwrap().cluster_id = 0;
            values.push(json!({"scope_delete":format!("{r:?}"),"bytes":bytes(&records(store.as_ref(),&p,NULL_KEYSPACE_ID).await)}));
        }
        // Finite TTL is wall-clock dependent: preserve raw expiry and validate its exact rounding interval.
        for ttl in [1, 60, 9_223_372_036, 9_223_372_037, i64::MAX] {
            let start = seconds();
            let (_, e, b) = call(&mut c, id, 2, "finite", 30, ttl, true).await.unwrap();
            let end = seconds();
            assert!(!e);
            let b = b.unwrap();
            let saturated = ttl >= 9_223_372_037;
            assert_eq!(b.ttl_seconds, if saturated { i64::MAX } else { ttl });
            let ep = astersql_pd_storage_domain::StorageEndpoint::new(store.clone());
            let persisted = ep
                .load_gc_barrier_with_service_id(&p, 2, b"finite")
                .await
                .unwrap()
                .unwrap();
            if saturated {
                assert!(persisted.expiration_time.is_none())
            } else {
                let expiry = persisted.expiration_time.unwrap().unix_seconds();
                assert!(expiry >= start + ttl && expiry <= end + ttl + 1);
            }
            let raw_before = records(store.as_ref(), &p, 2).await;
            let start_ns = std::time::SystemTime::now();
            let (_, e, b) = call(&mut c, id, 2, "finite", 0, 0, false).await.unwrap();
            let end_ns = std::time::SystemTime::now();
            assert!(!e);
            let b = b.unwrap();
            if saturated {
                assert_eq!(b.ttl_seconds, i64::MAX)
            } else {
                let expiry = persisted.expiration_time.unwrap().unix_seconds() as f64;
                let at = |t: std::time::SystemTime| {
                    t.duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs_f64()
                };
                assert!(
                    b.ttl_seconds >= (expiry - at(end_ns)).floor().max(0.) as i64
                        && b.ttl_seconds <= (expiry - at(start_ns)).floor().max(0.) as i64
                );
            }
            raw.push(json!({"finite_ttl":ttl,"start":start,"end":end,"set":raw_before,"deleted_response":format!("{b:?}"),"after":records(store.as_ref(),&p,2).await}));
        }
        // A real Set-created expiring barrier blocks advancement, then is atomically removed on expiry.
        call(&mut c, id, 2, "expiring", 20, 1, true).await.unwrap();
        let r = c
            .advance_txn_safe_point(pdpb::AdvanceTxnSafePointRequest {
                header: header(id),
                keyspace_scope: scope(2),
                target: 21,
            })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(r.new_txn_safe_point, 20);
        tokio::time::sleep(Duration::from_secs(3)).await;
        let r = c
            .advance_txn_safe_point(pdpb::AdvanceTxnSafePointRequest {
                header: header(id),
                keyspace_scope: scope(2),
                target: 21,
            })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(r.new_txn_safe_point, 21);
        assert!(
            astersql_pd_storage_domain::StorageEndpoint::new(store.clone())
                .load_gc_barrier_with_service_id(&p, 2, b"expiring")
                .await
                .unwrap()
                .is_none()
        );
        raw.push(json!({"expired":records(store.as_ref(),&p,2).await}));
        // Force genuine etcd revision CAS conflicts for both methods; no state mutation is allowed.
        let rev = base
            .load(p.gc_state_revision_path().as_bytes())
            .await
            .unwrap();
        base.save(p.gc_state_revision_path().as_bytes(), b"")
            .await
            .unwrap();
        for set in [true, false] {
            let before = records(store.as_ref(), &p, 2).await;
            let (h, e, _) = call(&mut c, id, 2, "fault", 30, i64::MAX, set)
                .await
                .unwrap();
            assert!(e);
            let after = records(store.as_ref(), &p, 2).await;
            unchanged(&before, &after);
            values.push(json!({"conflict":set,"header":h}));
            raw.push(json!({"before":before,"after":after}));
        }
        base.save(p.gc_state_revision_path().as_bytes(), &rev)
            .await
            .unwrap();
        for set in [true, false] {
            let e = call(&mut c, 0, 2, "a", 30, i64::MAX, set)
                .await
                .unwrap_err();
            raw.push(
                json!({"wrong_cluster":set,"code":format!("{:?}",e.code()),"message":e.message()}),
            );
            assert_eq!(e.code(), tonic::Code::FailedPrecondition);
            assert_eq!(
                e.message(),
                format!("mismatch cluster id, need {id} but got 0")
            );
        }
        // Renewal updates the same physical record and persists its new finite expiry.
        for (ts, ttl) in [(30, 30), (40, 3600), (25, 7200)] {
            let start = seconds();
            let (_, error, b) = call(&mut c, id, 2, "renew", ts, ttl, true).await.unwrap();
            let end = seconds();
            assert!(!error);
            assert_eq!(b.as_ref().unwrap().ttl_seconds, ttl);
            let state = records(store.as_ref(), &p, 2).await;
            let persisted = astersql_pd_storage_domain::StorageEndpoint::new(store.clone())
                .load_gc_barrier_with_service_id(&p, 2, b"renew")
                .await
                .unwrap()
                .unwrap();
            assert_eq!(persisted.barrier_ts, ts);
            let expiry = persisted.expiration_time.unwrap().unix_seconds();
            assert!(expiry >= start + ttl && expiry <= end + ttl + 1);
            raw.push(json!({"renew_ts":ts,"ttl":ttl,"response":format!("{b:?}"),"start":start,"end":end,"state":state}));
        }
        // Real WAL restart retains keys and physical create/mod revisions.
        let durable = [
            records(store.as_ref(), &p, NULL_KEYSPACE_ID).await,
            records(store.as_ref(), &p, 2).await,
            records(store.as_ref(), &p, 3).await,
        ];
        drop(c);
        drop(base);
        drop(keys);
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
        // Lost response is not rollback: observe Set and Delete commits independently through etcd.
        for set in [true, false] {
            use prost::Message;
            let gate = response_gate::ResponseGate::start(&s.url).await;
            let mut lost = PdClient::connect(gate.endpoint.clone()).await.unwrap();
            lost.get_members(pdpb::GetMembersRequest::default())
                .await
                .unwrap();
            let request = if set {
                let r = req(id, 2, "lost", 45, i64::MAX);
                gate.arm_on_key(&r.encode_to_vec());
                let mut r = tonic::Request::new(r);
                r.set_timeout(Duration::from_secs(2));
                tokio::spawn(async move { lost.set_gc_barrier(r).await.map(|_| ()) })
            } else {
                let r = del(id, 2, "lost");
                gate.arm_on_key(&r.encode_to_vec());
                let mut r = tonic::Request::new(r);
                r.set_timeout(Duration::from_secs(2));
                tokio::spawn(async move { lost.delete_gc_barrier(r).await.map(|_| ()) })
            };
            tokio::time::timeout(Duration::from_secs(3), gate.held.notified())
                .await
                .unwrap();
            tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    let b = astersql_pd_storage_domain::StorageEndpoint::new(store.clone())
                        .load_gc_barrier_with_service_id(&p, 2, b"lost")
                        .await
                        .unwrap();
                    if b.is_some() == set {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            let e = request.await.unwrap().unwrap_err();
            raw.push(json!({"lost_response":set,"code":format!("{:?}",e.code()),"message":e.message(),"state":records(store.as_ref(),&p,2).await}));
            gate.release();
            gate.close().await;
        }
        let before = records(store.as_ref(), &p, 2).await;
        {
            let _pause = s.suspend();
            let mut a = tonic::Request::new(req(id, 2, "cancel", 0, 0));
            a.set_timeout(Duration::from_millis(30));
            let mut b = tonic::Request::new(del(id, 2, "gc_worker"));
            b.set_timeout(Duration::from_millis(30));
            for e in [
                c.set_gc_barrier(a).await.unwrap_err(),
                c.delete_gc_barrier(b).await.unwrap_err(),
            ] {
                assert_eq!(e.code(), tonic::Code::Cancelled);
                assert_eq!(e.message(), "Timeout expired");
                raw.push(json!({"cancel":format!("{e:?}")}));
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
        for set in [true, false] {
            let e = call(&mut fc, id, 2, "a", 30, i64::MAX, set)
                .await
                .unwrap_err();
            assert_eq!(e.code(), tonic::Code::Unavailable);
            assert_eq!(e.message(), "not leader");
            raw.push(json!({"follower":set,"code":format!("{:?}",e.code()),"message":e.message()}));
        }
        unchanged(&before, &records(store.as_ref(), &p, 2).await);
        drop(fc);
        follower.stop().unwrap();
        drop(c);
        s.stop().unwrap();
        reports.push(json!({"rust":is_rust,"cluster_id":id,"raw":raw,"observations":values}));
        compared.push(values);
    }
    if let Some(path) = std::env::var_os("PD_SA006_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
    }
    assert_eq!(compared[0], compared[1]);
}
