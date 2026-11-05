// Copyright 2026 AsterSQL.
use crate::{
    harness::Server,
    txn_safe_point_oracle_test::{bytes, records},
};
use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_storage_domain::{Base, MetaStore, StorageEndpoint};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
type Client = PdClient<tonic::transport::Channel>;
fn header(id: u64) -> Option<pdpb::RequestHeader> {
    Some(pdpb::RequestHeader {
        cluster_id: id,
        ..Default::default()
    })
}
fn check_header(h: Option<pdpb::ResponseHeader>, id: u64) -> Value {
    let mut h = h.unwrap();
    assert_eq!(h.cluster_id, id);
    h.cluster_id = 0;
    json!(format!("{h:?}"))
}
fn status(s: tonic::Status) -> Value {
    json!({"code":format!("{:?}",s.code()),"message":s.message()})
}
async fn get(c: &mut Client, id: u64) -> Result<pdpb::GetGcSafePointResponse, tonic::Status> {
    c.get_gc_safe_point(pdpb::GetGcSafePointRequest { header: header(id) })
        .await
        .map(|r| r.into_inner())
}
async fn update(
    c: &mut Client,
    id: u64,
    point: u64,
) -> Result<pdpb::UpdateGcSafePointResponse, tonic::Status> {
    c.update_gc_safe_point(pdpb::UpdateGcSafePointRequest {
        header: header(id),
        safe_point: point,
    })
    .await
    .map(|r| r.into_inner())
}
async fn service(
    c: &mut Client,
    id: u64,
    name: &[u8],
    point: u64,
    ttl: i64,
) -> Result<pdpb::UpdateServiceGcSafePointResponse, tonic::Status> {
    c.update_service_gc_safe_point(pdpb::UpdateServiceGcSafePointRequest {
        header: header(id),
        service_id: name.to_vec(),
        safe_point: point,
        ttl,
    })
    .await
    .map(|r| r.into_inner())
}
async fn tso(c: &mut Client, id: u64) -> i64 {
    let mut stream = c
        .tso(tokio_stream::iter([pdpb::TsoRequest {
            header: header(id),
            count: 1,
            ..Default::default()
        }]))
        .await
        .unwrap()
        .into_inner();
    stream
        .message()
        .await
        .unwrap()
        .unwrap()
        .timestamp
        .unwrap()
        .physical
        / 1000
}
async fn bootstrap(c: &mut Client, id: u64) {
    let r = c
        .bootstrap(pdpb::BootstrapRequest {
            header: header(id),
            store: Some(metapb::Store {
                id: 1,
                version: "8.5.0".into(),
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
}
fn exact(state: &Value, index: usize) -> &Value {
    state[index]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["suffix"].as_array().unwrap().is_empty())
        .unwrap()
}
fn revision_counter(state: &Value) -> u64 {
    let bytes: Vec<u8> = exact(state, 0)["bytes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b.as_u64().unwrap() as u8)
        .collect();
    std::str::from_utf8(&bytes).unwrap().parse().unwrap()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_003_behavior() {
    assert_eq!(std::env::var("PD_COMPAT_MODE").as_deref(), Ok("dual"));
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    assert_ne!(go.canonicalize().unwrap(), rust.canonicalize().unwrap());
    let config = std::env::temp_dir().join(format!("pd-sa003-{}.toml", std::process::id()));
    std::fs::write(&config, "max-request-bytes = 1572864\n").unwrap();
    struct Remove(std::path::PathBuf);
    impl Drop for Remove {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _remove = Remove(config.clone());
    let config_arg = format!("--config={}", config.display());
    let mut samples = vec![];
    let mut reports = vec![];
    for is_rust in [false, true] {
        let mut server = if is_rust {
            Server::start_rust(&rust).unwrap()
        } else {
            Server::start_with_args(&go, &[&config_arg]).unwrap()
        };
        server.ready().unwrap();
        let mut c = PdClient::connect(server.url.clone()).await.unwrap();
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
                &[server.metadata_url().to_string()],
                Duration::from_secs(2),
            )
            .await
            .unwrap(),
        );
        let p = ClusterPaths::new(id);
        let mut sample = vec![];
        let mut detail = vec![];
        let a = get(&mut c, id)
            .await
            .expect("legacy GC RPC must reach the shared manager");
        let b = update(&mut c, id, 10).await.unwrap();
        let d = service(&mut c, id, b"backup", 10, 60).await.unwrap();
        for h in [a.header, b.header, d.header] {
            let h = h.unwrap();
            assert_eq!(h.cluster_id, id);
            assert_eq!(
                h.error.as_ref().unwrap().r#type,
                pdpb::ErrorType::NotBootstrapped as i32
            );
            sample.push(json!({"not_bootstrapped":check_header(Some(h),id)}));
        }
        bootstrap(&mut c, id).await;
        let ep = StorageEndpoint::new(store.clone());
        let mut tx = ep.begin_gc_state_transaction(&p).await.unwrap();
        tx.batch.set_txn_safe_point(NULL_KEYSPACE_ID, 50);
        tx.batch.set_gc_safe_point(NULL_KEYSPACE_ID, 0);
        tx.commit().await.unwrap();
        for target in [20, 20, 10, 51, 50] {
            let before = records(store.as_ref(), &p, NULL_KEYSPACE_ID).await;
            let response = update(&mut c, id, target).await;
            let value = match response {
                Ok(r) => json!({"header":check_header(r.header,id),"point":r.new_safe_point}),
                Err(s) => status(s),
            };
            let after = records(store.as_ref(), &p, NULL_KEYSPACE_ID).await;
            if target == 51 || target == 10 {
                for i in 0..6 {
                    assert_eq!(before[i]["entries"], after[i]["entries"]);
                }
            }
            if target != 51 && target != 10 {
                assert_eq!(revision_counter(&after), revision_counter(&before) + 1);
                assert_eq!(
                    exact(&after, 0)["mod_revision"],
                    exact(&after, 2)["mod_revision"]
                );
                assert!(
                    exact(&after, 2)["mod_revision"].as_i64().unwrap()
                        > exact(&before, 2)["mod_revision"].as_i64().unwrap()
                );
            }
            sample.push(json!({"update":target,"response":value,"state":bytes(&after)}));
            detail.push(json!({"update":target,"before":before,"after":after}));
            let r = get(&mut c, id).await.unwrap();
            assert_eq!(r.safe_point, if target == 50 { 50 } else { 20 });
            sample.push(json!({"get":r.safe_point,"header":check_header(r.header,id)}));
        }
        // Empty-but-present revision causes an actual etcd CAS failure, not a synthetic error.
        let revision = Base::new(store.as_ref())
            .load(p.gc_state_revision_path().as_bytes())
            .await
            .unwrap();
        Base::new(store.as_ref())
            .save(p.gc_state_revision_path().as_bytes(), b"")
            .await
            .unwrap();
        let before = records(store.as_ref(), &p, NULL_KEYSPACE_ID).await;
        let error = update(&mut c, id, 50).await.unwrap_err();
        assert_eq!(error.code(), tonic::Code::Unknown);
        assert_eq!(
            error.message(),
            "[PD:etcd:ErrEtcdTxnConflict]etcd transaction failed, conflicted and rolled back"
        );
        sample.push(json!({"conflict":status(error)}));
        let after = records(store.as_ref(), &p, NULL_KEYSPACE_ID).await;
        for i in 0..6 {
            assert_eq!(before[i]["entries"], after[i]["entries"]);
        }
        detail.push(json!({"conflict_before":before,"conflict_after":after}));
        Base::new(store.as_ref())
            .save(p.gc_state_revision_path().as_bytes(), &revision)
            .await
            .unwrap();
        Base::new(store.as_ref())
            .save(p.gc_safe_point_path(NULL_KEYSPACE_ID).as_bytes(), b"!")
            .await
            .unwrap();
        let error = get(&mut c, id).await.unwrap_err();
        sample.push(json!({"corrupt_get":status(error)}));
        Base::new(store.as_ref())
            .save(p.gc_safe_point_path(NULL_KEYSPACE_ID).as_bytes(), b"32")
            .await
            .unwrap();
        // Re-seed lower legacy values through the real storage fixture, then drive the public service API.
        let mut tx = ep.begin_gc_state_transaction(&p).await.unwrap();
        tx.batch.set_txn_safe_point(NULL_KEYSPACE_ID, 10);
        tx.batch.set_gc_safe_point(NULL_KEYSPACE_ID, 5);
        tx.commit().await.unwrap();
        for (name, point, ttl) in [
            (b"backup".as_slice(), 40, i64::MAX),
            (b"backup", 30, i64::MAX),
            (b"backup", 9, i64::MAX),
            (b"gc_worker", 20, i64::MAX),
            (b"gc_worker", 100, i64::MAX),
            (b"gc_worker", 100, 60),
            (b"native_br", 50, i64::MAX),
            (b"bad\xff", 50, i64::MAX),
            (b"backup", 0, 0),
            (b"native_br", 0, -1),
            (b"", 50, i64::MAX),
        ] {
            let before = records(store.as_ref(), &p, NULL_KEYSPACE_ID).await;
            let t0 = tso(&mut c, id).await;
            let response = service(&mut c, id, name, point, ttl).await;
            let t1 = tso(&mut c, id).await;
            let raw = format!("{response:?}");
            let value = match response {
                Ok(r) => {
                    let second = i64::MAX - r.ttl;
                    assert!(
                        (t0..=t1).contains(&second),
                        "real TSO TTL: {second}/{t0}/{t1}"
                    );
                    json!({"header":check_header(r.header,id),"service":r.service_id,"point":r.min_safe_point,"absolute_expiration":r.ttl+second})
                }
                Err(s) => status(s),
            };
            let after = records(store.as_ref(), &p, NULL_KEYSPACE_ID).await;
            let wrote =
                !name.is_empty() && !(name == b"gc_worker" && ttl != i64::MAX) && point != 9;
            assert_eq!(
                revision_counter(&after),
                revision_counter(&before) + u64::from(wrote)
            );
            if !wrote {
                for i in 0..6 {
                    assert_eq!(before[i]["entries"], after[i]["entries"]);
                }
            }
            if name == b"gc_worker" && wrote {
                assert_eq!(
                    exact(&after, 0)["mod_revision"],
                    exact(&after, 1)["mod_revision"]
                );
            }
            sample.push(json!({"service":name,"point":point,"ttl":ttl,"response":value,"state":bytes(&after)}));
            detail.push(json!({"service":name,"request_point":point,"request_ttl":ttl,"response":raw,"tso_before":t0,"tso_after":t1,"before":before,"after":after}));
        }
        // Finite TTL uses each implementation's actual TSO, with the full stored expiration retained.
        for ttl in [60, 1] {
            let name = if ttl == 60 {
                b"finite".as_slice()
            } else {
                b"expires".as_slice()
            };
            let r = service(&mut c, id, name, 30, ttl).await.unwrap();
            let now = i64::MAX - r.ttl;
            let barrier = ep
                .load_gc_barrier_with_service_id(&p, NULL_KEYSPACE_ID, name)
                .await
                .unwrap()
                .unwrap();
            let expiration = barrier.expiration_time.unwrap().unix_seconds();
            assert!((now + ttl..=now + ttl + 1).contains(&expiration));
            detail.push(json!({"finite_ttl":ttl,"response":format!("{r:?}"),"tso_seconds":now,"absolute_expiration":expiration,"stored":records(store.as_ref(),&p,NULL_KEYSPACE_ID).await}));
            if ttl == 60 {
                service(&mut c, id, name, 0, 0).await.unwrap();
            } else {
                tokio::time::timeout(Duration::from_secs(5), async {
                    while tso(&mut c, id).await <= expiration {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                })
                .await
                .unwrap();
                let r = service(&mut c, id, b"gc_worker", 100, i64::MAX)
                    .await
                    .unwrap();
                assert_eq!(r.min_safe_point, 50);
                assert!(
                    ep.load_gc_barrier_with_service_id(&p, NULL_KEYSPACE_ID, name)
                        .await
                        .unwrap()
                        .is_none()
                );
            }
            let state = records(store.as_ref(), &p, NULL_KEYSPACE_ID).await;
            sample.push(json!({"finite_ttl":ttl,"expiration_verified_against_real_tso":true,"after_delete_or_expiry":bytes(&state)}));
            detail.push(json!({"finite_ttl":ttl,"after_delete_or_expiry":state}));
        }
        // The request fits gRPC, but its real etcd transaction exceeds max-request-bytes.
        let oversized = vec![b'X'; 900_000];
        let before = records(store.as_ref(), &p, NULL_KEYSPACE_ID).await;
        let error = service(&mut c, id, &oversized, 100, i64::MAX)
            .await
            .unwrap_err();
        assert_eq!(error.code(), tonic::Code::Unknown);
        sample.push(json!({"oversized_transaction":status(error)}));
        let after = records(store.as_ref(), &p, NULL_KEYSPACE_ID).await;
        for i in 0..6 {
            assert_eq!(before[i]["entries"], after[i]["entries"]);
        }
        detail
            .push(json!({"oversized_service_bytes":oversized.len(),"before":before,"after":after}));
        // Every method validates role before the exact cluster identity error.
        for error in [
            get(&mut c, 0).await.unwrap_err(),
            update(&mut c, 0, 100).await.unwrap_err(),
            service(&mut c, 0, b"backup", 100, i64::MAX)
                .await
                .unwrap_err(),
        ] {
            assert_eq!(error.code(), tonic::Code::FailedPrecondition);
            assert_eq!(
                error.message(),
                format!("mismatch cluster id, need {id} but got 0")
            );
            sample.push(json!({"wrong_cluster":{"code":format!("{:?}",error.code()),"message":"mismatch cluster id, need <own-id> but got 0"}}));
        }
        let durable = records(store.as_ref(), &p, NULL_KEYSPACE_ID).await;
        drop(c);
        drop(ep);
        drop(store);
        if is_rust {
            server.restart_rust(&rust).unwrap();
        } else {
            server.restart_with_args(&go, &[&config_arg]).unwrap();
        }
        server.ready().unwrap();
        c = PdClient::connect(server.url.clone()).await.unwrap();
        let recovered: Arc<dyn MetaStore> = Arc::new(
            astersql_pd_storage_kv::EtcdMetaStore::connect(
                &[server.metadata_url().to_string()],
                Duration::from_secs(2),
            )
            .await
            .unwrap(),
        );
        let state = records(recovered.as_ref(), &p, NULL_KEYSPACE_ID).await;
        assert_eq!(bytes(&durable), bytes(&state));
        for i in 0..6 {
            assert_eq!(durable[i]["entries"], state[i]["entries"]);
        }

        sample
            .push(json!({"recovered":bytes(&state),"gc":get(&mut c,id).await.unwrap().safe_point}));
        detail.push(json!({"before_restart":durable,"same_wal_recovered":state}));
        // Real transport deadlines with valid read-only/no-op compatibility requests.
        // Cancellation never promises rollback of a mutation already dispatched; 44/47 cover uncertain committed outcomes.
        // The owned process is suspended before admission and resumed by RAII.
        let before = records(recovered.as_ref(), &p, NULL_KEYSPACE_ID).await;
        {
            let _pause = server.suspend();
            let mut a = tonic::Request::new(pdpb::GetGcSafePointRequest { header: header(id) });
            a.set_timeout(Duration::from_millis(30));
            let mut b = tonic::Request::new(pdpb::UpdateGcSafePointRequest {
                header: header(id),
                safe_point: 0,
            });
            b.set_timeout(Duration::from_millis(30));
            let mut d = tonic::Request::new(pdpb::UpdateServiceGcSafePointRequest {
                header: header(id),
                service_id: b"cancelled".to_vec(),
                safe_point: 0,
                ttl: i64::MAX,
            });
            d.set_timeout(Duration::from_millis(30));
            for e in [
                c.get_gc_safe_point(a).await.unwrap_err(),
                c.update_gc_safe_point(b).await.unwrap_err(),
                c.update_service_gc_safe_point(d).await.unwrap_err(),
            ] {
                sample.push(json!({"deadline":status(e)}));
            }
        }
        get(&mut c, id).await.unwrap();
        let after = records(recovered.as_ref(), &p, NULL_KEYSPACE_ID).await;
        for i in 0..6 {
            assert_eq!(before[i]["entries"], after[i]["entries"]);
        }
        detail.push(json!({"cancel_before":before,"cancel_after":after}));
        // A second real PD process shares the backend and must reject leader-only RPCs.
        let mut follower = if is_rust {
            Server::start_with_args(
                &rust,
                &[
                    &format!("--etcd-endpoints={}", server.metadata_url()),
                    "--name=follower",
                ],
            )
            .unwrap()
        } else {
            Server::start_with_args(
                &go,
                &[
                    &format!("--join={}", server.url),
                    "--initial-cluster=",
                    "--name=follower",
                ],
            )
            .unwrap()
        };
        follower.ready().unwrap();
        let mut fc = PdClient::connect(follower.url.clone()).await.unwrap();
        for e in [
            get(&mut fc, id).await.unwrap_err(),
            update(&mut fc, id, 100).await.unwrap_err(),
            service(&mut fc, id, b"follower", 100, i64::MAX)
                .await
                .unwrap_err(),
        ] {
            assert_eq!(e.code(), tonic::Code::Unavailable);
            assert_eq!(e.message(), "not leader");
            sample.push(json!({"follower":status(e)}));
        }
        let after = records(recovered.as_ref(), &p, NULL_KEYSPACE_ID).await;
        for i in 0..6 {
            assert_eq!(before[i]["entries"], after[i]["entries"]);
        }
        follower.stop().unwrap();
        server.stop().unwrap();
        reports.push(json!({"implementation":if is_rust{"rust"}else{"go"},"cluster":id,"sample":sample,"physical_evidence":detail}));
        samples.push(sample);
    }
    assert_eq!(samples[0], samples[1]);
    if let Some(path) = std::env::var_os("SA_003_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
    }
}
