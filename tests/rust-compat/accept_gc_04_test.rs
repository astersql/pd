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
#[path = "../../pkg/storage/kv/response_gate_test.rs"]
mod response_gate;
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
fn revision_counter(state: &Value) -> u64 {
    let e = &state[0]["entries"];
    if e.as_array().unwrap().is_empty() {
        0
    } else {
        let b: Vec<u8> = serde_json::from_value(e[0]["bytes"].clone()).unwrap();
        String::from_utf8(b).unwrap().parse().unwrap()
    }
}
fn unchanged(before: &Value, after: &Value) {
    for n in 0..6 {
        assert_eq!(before[n]["entries"], after[n]["entries"]);
    }
}
fn status(s: tonic::Status) -> Value {
    json!({"code":format!("{:?}",s.code()),"message":s.message()})
}
async fn get(
    c: &mut Client,
    id: u64,
    ks: u32,
) -> Result<pdpb::GetGcSafePointV2Response, tonic::Status> {
    c.get_gc_safe_point_v2(pdpb::GetGcSafePointV2Request {
        header: header(id),
        keyspace: Some(pdpb::get_gc_safe_point_v2_request::Keyspace::KeyspaceId(ks)),
    })
    .await
    .map(|r| r.into_inner())
}
async fn update(
    c: &mut Client,
    id: u64,
    ks: u32,
    point: u64,
) -> Result<pdpb::UpdateGcSafePointV2Response, tonic::Status> {
    c.update_gc_safe_point_v2(pdpb::UpdateGcSafePointV2Request {
        header: header(id),
        keyspace: Some(pdpb::update_gc_safe_point_v2_request::Keyspace::KeyspaceId(
            ks,
        )),
        safe_point: point,
    })
    .await
    .map(|r| r.into_inner())
}
async fn service(
    c: &mut Client,
    id: u64,
    name: &[u8],
    ks: u32,
    point: u64,
    ttl: i64,
) -> Result<pdpb::UpdateServiceSafePointV2Response, tonic::Status> {
    c.update_service_safe_point_v2(pdpb::UpdateServiceSafePointV2Request {
        header: header(id),
        service_id: name.to_vec(),
        keyspace: Some(pdpb::update_service_safe_point_v2_request::Keyspace::KeyspaceId(ks)),
        safe_point: point,
        ttl,
    })
    .await
    .map(|r| r.into_inner())
}
async fn all(c: &mut Client, id: u64) -> Result<pdpb::GetAllGcSafePointV2Response, tonic::Status> {
    c.get_all_gc_safe_point_v2(pdpb::GetAllGcSafePointV2Request { header: header(id) })
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_004_behavior() {
    assert_eq!(std::env::var("PD_COMPAT_MODE").as_deref(), Ok("dual"));
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let config = std::env::temp_dir().join(format!("pd-sa004-{}.toml", std::process::id()));
    std::fs::write(&config, "max-request-bytes = 1572864\n").unwrap();
    struct Remove(std::path::PathBuf);
    impl Drop for Remove {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _remove = Remove(config.clone());
    let config_arg = format!("--config={}", config.display());
    let mut reports = vec![];
    let mut samples = vec![];
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
        let ep = StorageEndpoint::new(store.clone());
        let mut values = vec![];
        let mut detail = vec![];
        for h in [
            all(&mut c, id)
                .await
                .expect("GCv2 enumeration must reach its manager")
                .header,
            get(&mut c, id, 2)
                .await
                .expect("GCv2 read must reach its manager")
                .header,
            update(&mut c, id, 2, 10).await.unwrap().header,
            service(&mut c, id, b"backup", 2, 10, 60)
                .await
                .unwrap()
                .header,
        ] {
            assert_eq!(
                h.as_ref().unwrap().error.as_ref().unwrap().r#type,
                pdpb::ErrorType::NotBootstrapped as i32
            );
            values.push(check_header(h, id));
        }
        bootstrap(&mut c, id).await;
        let keys = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone())
            .keyspace_storage(p.clone());
        for (ks, level, state) in [
            (0, false, 0),
            (2, true, 0),
            (3, true, 0),
            (4, false, 0),
            (5, true, 1),
            (6, true, 2),
        ] {
            let mut tx = Base::new(store.as_ref()).begin_txn();
            keys.save_keyspace_meta(
                &mut tx,
                &p,
                &astersql_pd_storage_domain::Keyspace {
                    identity: Some(astersql_pd_storage_domain::KeyspaceIdentity::Id(ks)),
                    name: format!("space{ks}"),
                    state,
                    config: if level {
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
            tx.commit::<String>().await.unwrap();
        }
        // Deprecated oneof getters use numeric zero when absent or when a V3 identity is supplied.
        for identity in [false, true] {
            let get_key = identity.then_some(
                pdpb::get_gc_safe_point_v2_request::Keyspace::KeyspaceIdentity(Default::default()),
            );
            let update_key = identity.then_some(
                pdpb::update_gc_safe_point_v2_request::Keyspace::KeyspaceIdentity(
                    Default::default(),
                ),
            );
            let service_key = identity.then_some(
                pdpb::update_service_safe_point_v2_request::Keyspace::KeyspaceIdentity(
                    Default::default(),
                ),
            );
            let a = c
                .get_gc_safe_point_v2(pdpb::GetGcSafePointV2Request {
                    header: header(id),
                    keyspace: get_key,
                })
                .await
                .unwrap_err();
            let b = c
                .update_gc_safe_point_v2(pdpb::UpdateGcSafePointV2Request {
                    header: header(id),
                    keyspace: update_key,
                    safe_point: 1,
                })
                .await
                .unwrap_err();
            let d = c
                .update_service_safe_point_v2(pdpb::UpdateServiceSafePointV2Request {
                    header: header(id),
                    keyspace: service_key,
                    service_id: b"x".to_vec(),
                    safe_point: 0,
                    ttl: 0,
                })
                .await
                .unwrap()
                .into_inner();
            values.push(json!({"v3_or_absent_service":identity,"header":check_header(d.header,id),"service":d.service_id,"point":d.min_safe_point}));
            for error in [a, b] {
                assert_eq!(error.code(), tonic::Code::Unknown);
                values.push(json!({"v3_or_absent":identity,"error":status(error)}));
            }
        }
        let mut tx = ep.begin_gc_state_transaction(&p).await.unwrap();
        for ks in [2, 3, NULL_KEYSPACE_ID] {
            tx.batch.set_txn_safe_point(ks, 50);
            tx.batch.set_gc_safe_point(ks, 0);
        }
        tx.commit().await.unwrap();
        for (ks, target) in [
            (2, 20),
            (3, 30),
            (2, 20),
            (2, 10),
            (2, 51),
            (4, 20),
            (99, 20),
            (0xfffffffe, 15),
        ] {
            let before = records(
                store.as_ref(),
                &p,
                if ks > 0xffffff { NULL_KEYSPACE_ID } else { ks },
            )
            .await;
            let result = match update(&mut c, id, ks, target).await {
                Ok(r) => json!({"header":check_header(r.header,id),"new":r.new_safe_point}),
                Err(e) => status(e),
            };
            let after = records(
                store.as_ref(),
                &p,
                if ks > 0xffffff { NULL_KEYSPACE_ID } else { ks },
            )
            .await;
            if [10, 51].contains(&target) || [4, 99].contains(&ks) {
                assert_eq!(
                    before
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| &v["entries"])
                        .collect::<Vec<_>>(),
                    after
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| &v["entries"])
                        .collect::<Vec<_>>()
                );
            }
            let wrote = ![10, 51].contains(&target) && ![4, 99].contains(&ks);
            assert_eq!(
                revision_counter(&after),
                revision_counter(&before) + u64::from(wrote)
            );
            if wrote {
                assert_eq!(
                    after[0]["entries"][0]["mod_revision"],
                    after[2]["entries"][0]["mod_revision"]
                );
            }
            values.push(json!({"ks":ks,"target":target,"response":result,"state":bytes(&after)}));
            detail.push(json!({"before":before,"after":after}));
        }
        for ks in [2, 3, 4, 5, 6, 99, 0xfffffffe] {
            let result = match get(&mut c, id, ks).await {
                Ok(r) => json!({"header":check_header(r.header,id),"point":r.safe_point}),
                Err(e) => status(e),
            };
            values.push(result);
        }
        assert_eq!(get(&mut c, id, 2).await.unwrap().safe_point, 20);
        assert_eq!(get(&mut c, id, 3).await.unwrap().safe_point, 30);
        for (ks, name, point, ttl) in [
            (2, b"backup".as_slice(), 55, i64::MAX),
            (3, b"backup".as_slice(), 65, i64::MAX),
            (2, b"backup".as_slice(), 49, 60),
            (2, "服务".as_bytes(), 55, i64::MAX),
            (2, "服务".as_bytes(), 0, 0),
            (2, b"gc_worker".as_slice(), 25, 60),
            (4, b"backup".as_slice(), 25, 60),
            (99, b"backup".as_slice(), 25, 60),
            (2, b"backup".as_slice(), 0, 0),
            (2, b"native_br".as_slice(), 55, i64::MAX),
            (2, b"native_br".as_slice(), 0, 0),
            (0xfffffffe, b"native_br".as_slice(), 55, i64::MAX),
            (0xfffffffe, b"native_br".as_slice(), 0, 0),
            (2, b"".as_slice(), 55, i64::MAX),
        ] {
            let before = records(
                store.as_ref(),
                &p,
                if ks > 0xffffff { NULL_KEYSPACE_ID } else { ks },
            )
            .await;
            let t0 = tso(&mut c, id).await;
            let result = match service(&mut c, id, name, ks, point, ttl).await {
                Ok(r) => {
                    let t1 = tso(&mut c, id).await;
                    assert!((i64::MAX - t1..=i64::MAX - t0).contains(&r.ttl));
                    detail.push(json!({"raw_service_response":format!("{r:?}"),"tso_before":t0,"tso_after":t1}));
                    json!({"header":check_header(r.header,id),"service":r.service_id,"point":r.min_safe_point})
                }
                Err(e) => status(e),
            };
            let after = records(
                store.as_ref(),
                &p,
                if ks > 0xffffff { NULL_KEYSPACE_ID } else { ks },
            )
            .await;
            let wrote =
                result.get("header").is_some() && !name.is_empty() && !(point < 50 && ttl > 0);
            assert_eq!(
                revision_counter(&after),
                revision_counter(&before) + u64::from(wrote)
            );
            if !wrote {
                unchanged(&before, &after);
            }
            values.push(json!({"ks":ks,"name":name,"response":result,"state":bytes(&after)}));
            detail.push(json!({"before":before,"after":after}));
        }
        // An actual CAS conflict leaves all persisted rows unchanged.
        let base = Base::new(store.as_ref());
        let revision = base
            .load(p.gc_state_revision_path().as_bytes())
            .await
            .unwrap();
        base.save(p.gc_state_revision_path().as_bytes(), b"")
            .await
            .unwrap();
        let before = records(store.as_ref(), &p, 2).await;
        let error = update(&mut c, id, 2, 20).await.unwrap_err();
        assert_eq!(error.code(), tonic::Code::Unknown);
        values.push(json!({"conflict":status(error)}));
        let after = records(store.as_ref(), &p, 2).await;
        unchanged(&before, &after);
        detail.push(json!({"conflict_before":before,"conflict_after":after}));
        let before_null = records(store.as_ref(), &p, NULL_KEYSPACE_ID).await;
        let error = update(&mut c, id, NULL_KEYSPACE_ID, 20).await.unwrap_err();
        values.push(json!({"conflict_null":status(error)}));
        let after_null = records(store.as_ref(), &p, NULL_KEYSPACE_ID).await;
        unchanged(&before_null, &after_null);
        detail.push(json!({"conflict_null_before":before_null,"conflict_null_after":after_null}));
        base.save(p.gc_state_revision_path().as_bytes(), &revision)
            .await
            .unwrap();
        // The legacy hex read error is preserved byte-for-byte.
        let saved = base
            .load(p.gc_safe_point_path(NULL_KEYSPACE_ID).as_bytes())
            .await
            .unwrap();
        base.save(p.gc_safe_point_path(NULL_KEYSPACE_ID).as_bytes(), b"!")
            .await
            .unwrap();
        values.push(
            json!({"corrupt_get":status(get(&mut c,id,NULL_KEYSPACE_ID).await.unwrap_err())}),
        );
        base.save(p.gc_safe_point_path(NULL_KEYSPACE_ID).as_bytes(), &saved)
            .await
            .unwrap();
        let response = all(&mut c, id).await.unwrap();
        assert_eq!(response.revision, 0);
        assert!(response.header.as_ref().unwrap().error.is_none());
        let mut points = response.gc_safe_points;
        points.sort_by_key(|p| format!("{:?}", p.keyspace));
        assert!(points.iter().all(|p| !matches!(
            p.keyspace,
            Some(pdpb::gc_safe_point_v2::Keyspace::KeyspaceId(
                NULL_KEYSPACE_ID
            ))
        )));
        values.push(json!({"list":format!("{points:?}"),"header":check_header(response.header,id),"revision":response.revision}));
        assert!(points.iter().any(|p| p.keyspace
            == Some(pdpb::gc_safe_point_v2::Keyspace::KeyspaceId(4))
            && p.gc_safe_point == 0));
        assert!(points.iter().all(|p| !matches!(
            p.keyspace,
            Some(pdpb::gc_safe_point_v2::Keyspace::KeyspaceId(5 | 6))
        )));
        // A newly discovered scope with a corrupt txn point yields UNKNOWN in a response header.
        let mut tx = Base::new(store.as_ref()).begin_txn();
        keys.save_keyspace_meta(
            &mut tx,
            &p,
            &astersql_pd_storage_domain::Keyspace {
                identity: Some(astersql_pd_storage_domain::KeyspaceIdentity::Id(7)),
                name: "fault7".into(),
                config: std::collections::HashMap::from([(
                    "gc_management_type".into(),
                    "keyspace_level".into(),
                )]),
                ..Default::default()
            },
        )
        .unwrap();
        tx.save(p.txn_safe_point_path(7).as_bytes(), b"bad")
            .unwrap();
        tx.commit::<String>().await.unwrap();
        let before = records(store.as_ref(), &p, 7).await;
        let response = all(&mut c, id).await.unwrap();
        assert!(response.gc_safe_points.is_empty());
        assert_eq!(response.revision, 0);
        assert_eq!(
            response
                .header
                .as_ref()
                .unwrap()
                .error
                .as_ref()
                .unwrap()
                .r#type,
            pdpb::ErrorType::Unknown as i32
        );
        values.push(json!({"list_fault":check_header(response.header,id)}));
        let after = records(store.as_ref(), &p, 7).await;
        unchanged(&before, &after);
        detail.push(json!({"list_fault_before":before,"list_fault_after":after}));
        base.save(p.txn_safe_point_path(7).as_bytes(), b"0")
            .await
            .unwrap();
        // Finite service leases are verified using each implementation's actual TSO.
        for ttl in [60, 1] {
            let name = if ttl == 60 {
                b"finite".as_slice()
            } else {
                b"expires".as_slice()
            };
            let t0 = tso(&mut c, id).await;
            let r = service(&mut c, id, name, 2, 55, ttl).await.unwrap();
            let t1 = tso(&mut c, id).await;
            let expiration = ep
                .load_gc_barrier_with_service_id(&p, 2, name)
                .await
                .unwrap()
                .unwrap()
                .expiration_time
                .unwrap()
                .unix_seconds();
            assert!((t0 + ttl..=t1 + ttl + 1).contains(&expiration));
            detail.push(json!({"finite_ttl":ttl,"response":format!("{r:?}"),"tso_before":t0,"tso_after":t1,"expiration":expiration,"rows":records(store.as_ref(),&p,2).await}));
            if ttl == 1 {
                tokio::time::timeout(Duration::from_secs(5), async {
                    while tso(&mut c, id).await <= expiration {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                })
                .await
                .unwrap();
                service(&mut c, id, b"gc_worker", 2, 50, i64::MAX)
                    .await
                    .unwrap();
                assert!(
                    ep.load_gc_barrier_with_service_id(&p, 2, name)
                        .await
                        .unwrap()
                        .is_none()
                );
            } else {
                service(&mut c, id, name, 2, 0, 0).await.unwrap();
            }
            values.push(json!({"finite_ttl":ttl,"validated":true,"after_cleanup":bytes(&records(store.as_ref(),&p,2).await)}));
        }
        for error in [
            get(&mut c, 0, 2).await.unwrap_err(),
            update(&mut c, 0, 2, 20).await.unwrap_err(),
            service(&mut c, 0, b"x", 2, 55, 60).await.unwrap_err(),
            all(&mut c, 0).await.unwrap_err(),
        ] {
            assert_eq!(error.code(), tonic::Code::FailedPrecondition);
            assert_eq!(
                error.message(),
                format!("mismatch cluster id, need {id} but got 0")
            );
            values.push(json!({"wrong_cluster":"FailedPrecondition: mismatch cluster id, need <own-id> but got 0"}));
        }
        let before = records(store.as_ref(), &p, 2).await;
        let oversized = vec![b'X'; 900_000];
        let error = service(&mut c, id, &oversized, 2, 100, i64::MAX)
            .await
            .unwrap_err();
        assert_eq!(error.code(), tonic::Code::Unknown);
        values.push(json!({"oversized_transaction":status(error)}));
        let after = records(store.as_ref(), &p, 2).await;
        unchanged(&before, &after);
        detail
            .push(json!({"oversized_service_bytes":oversized.len(),"before":before,"after":after}));
        let durable = [
            records(store.as_ref(), &p, 2).await,
            records(store.as_ref(), &p, 3).await,
        ];
        drop(c);
        drop(ep);
        drop(keys);
        drop(store);
        if is_rust {
            server.restart_rust(&rust).unwrap()
        } else {
            server.restart_with_args(&go, &[&config_arg]).unwrap()
        };
        server.ready().unwrap();
        c = PdClient::connect(server.url.clone()).await.unwrap();
        assert_eq!(get(&mut c, id, 2).await.unwrap().safe_point, 20);
        assert_eq!(get(&mut c, id, 3).await.unwrap().safe_point, 30);
        let recovered: Arc<dyn MetaStore> = Arc::new(
            astersql_pd_storage_kv::EtcdMetaStore::connect(
                &[server.metadata_url().to_string()],
                Duration::from_secs(2),
            )
            .await
            .unwrap(),
        );
        for (index, ks) in [2, 3].into_iter().enumerate() {
            let after = records(recovered.as_ref(), &p, ks).await;
            for n in 0..6 {
                assert_eq!(durable[index][n]["entries"], after[n]["entries"]);
            }
            detail.push(json!({"wal_recovered":ks,"before":durable[index],"after":after}));
        }
        let recovered_list = all(&mut c, id).await.unwrap();
        assert!(recovered_list.header.as_ref().unwrap().error.is_none());
        assert_eq!(recovered_list.revision, 0);
        let mut recovered_points = recovered_list.gc_safe_points;
        recovered_points.sort_by_key(|p| format!("{:?}", p.keyspace));
        values.push(json!({"recovered_list":format!("{recovered_points:?}"),"header":check_header(recovered_list.header,id),"revision":0}));
        // Hold actual TCP replies after dispatch, independently confirm persisted mutation, then observe deadline.
        for service_write in [false, true] {
            use prost::Message;
            let gate = response_gate::ResponseGate::start(&server.url).await;
            let mut lost = PdClient::connect(gate.endpoint.clone()).await.unwrap();
            lost.get_members(pdpb::GetMembersRequest::default())
                .await
                .unwrap();
            let before = records(recovered.as_ref(), &p, 2).await;
            let request = if service_write {
                let r = pdpb::UpdateServiceSafePointV2Request {
                    header: header(id),
                    keyspace: Some(
                        pdpb::update_service_safe_point_v2_request::Keyspace::KeyspaceId(2),
                    ),
                    service_id: b"lost_reply".to_vec(),
                    safe_point: 55,
                    ttl: i64::MAX,
                };
                gate.arm_on_key(&r.encode_to_vec());
                let mut r = tonic::Request::new(r);
                r.set_timeout(Duration::from_secs(2));
                tokio::spawn(async move { lost.update_service_safe_point_v2(r).await.map(|_| ()) })
            } else {
                let r = pdpb::UpdateGcSafePointV2Request {
                    header: header(id),
                    keyspace: Some(pdpb::update_gc_safe_point_v2_request::Keyspace::KeyspaceId(
                        2,
                    )),
                    safe_point: 21,
                };
                gate.arm_on_key(&r.encode_to_vec());
                let mut r = tonic::Request::new(r);
                r.set_timeout(Duration::from_secs(2));
                tokio::spawn(async move { lost.update_gc_safe_point_v2(r).await.map(|_| ()) })
            };
            tokio::time::timeout(Duration::from_secs(3), gate.held.notified())
                .await
                .unwrap();
            let endpoint = StorageEndpoint::new(recovered.clone());
            tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    let committed = if service_write {
                        endpoint
                            .load_gc_barrier_with_service_id(&p, 2, b"lost_reply")
                            .await
                            .unwrap()
                            .is_some()
                    } else {
                        endpoint.load_gc_safe_point(&p, 2).await.unwrap() == 21
                    };
                    if committed {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            let error = request.await.unwrap().unwrap_err();
            values.push(json!({"committed_but_response_cancelled":if service_write{"service"}else{"gc"},"error":status(error),"state":bytes(&records(recovered.as_ref(),&p,2).await)}));
            detail.push(json!({"committed_but_response_cancelled":service_write,"before":before,"after":records(recovered.as_ref(),&p,2).await}));
            gate.release();
            gate.close().await;
        }
        let before = records(recovered.as_ref(), &p, 2).await;
        {
            let _pause = server.suspend();
            let mut a = tonic::Request::new(pdpb::GetGcSafePointV2Request {
                header: header(id),
                keyspace: Some(pdpb::get_gc_safe_point_v2_request::Keyspace::KeyspaceId(2)),
            });
            a.set_timeout(Duration::from_millis(30));
            let mut b = tonic::Request::new(pdpb::UpdateGcSafePointV2Request {
                header: header(id),
                keyspace: Some(pdpb::update_gc_safe_point_v2_request::Keyspace::KeyspaceId(
                    2,
                )),
                safe_point: 0,
            });
            b.set_timeout(Duration::from_millis(30));
            let mut d = tonic::Request::new(pdpb::UpdateServiceSafePointV2Request {
                header: header(id),
                keyspace: Some(pdpb::update_service_safe_point_v2_request::Keyspace::KeyspaceId(2)),
                service_id: b"cancelled".to_vec(),
                safe_point: 0,
                ttl: i64::MAX,
            });
            d.set_timeout(Duration::from_millis(30));
            let mut e =
                tonic::Request::new(pdpb::GetAllGcSafePointV2Request { header: header(id) });
            e.set_timeout(Duration::from_millis(30));
            for error in [
                c.get_gc_safe_point_v2(a).await.unwrap_err(),
                c.update_gc_safe_point_v2(b).await.unwrap_err(),
                c.update_service_safe_point_v2(d).await.unwrap_err(),
                c.get_all_gc_safe_point_v2(e).await.unwrap_err(),
            ] {
                values.push(json!({"deadline":status(error)}));
            }
        }
        get(&mut c, id, 2).await.unwrap();
        let after = records(recovered.as_ref(), &p, 2).await;
        unchanged(&before, &after);
        detail.push(json!({"deadline_before":before,"deadline_after":after}));
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
        for error in [
            get(&mut fc, id, 2).await.unwrap_err(),
            update(&mut fc, id, 2, 20).await.unwrap_err(),
            service(&mut fc, id, b"follower", 2, 55, i64::MAX)
                .await
                .unwrap_err(),
            all(&mut fc, id).await.unwrap_err(),
        ] {
            assert_eq!(error.code(), tonic::Code::Unavailable);
            assert_eq!(error.message(), "not leader");
            values.push(json!({"follower":status(error)}));
        }
        unchanged(&after, &records(recovered.as_ref(), &p, 2).await);
        drop(fc);
        follower.stop().unwrap();
        drop(c);
        reports.push(json!({"rust":is_rust,"raw":detail,"observations":values}));
        samples.push(values);
    }
    if let Some(path) = std::env::var_os("PD_SA004_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
    }
    assert_eq!(samples[0], samples[1]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_004_list_admission() {
    let rust = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut server = Server::start_rust(&rust).unwrap();
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
    let response = all(&mut c, id)
        .await
        .expect("GCv2 list must reach bootstrap validation instead of a stub");
    assert_eq!(
        response.header.unwrap().error.unwrap().r#type,
        pdpb::ErrorType::NotBootstrapped as i32
    );
    drop(c);
    server.stop().unwrap();
}
