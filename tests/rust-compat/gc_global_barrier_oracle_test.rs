// Copyright 2026 AsterSQL.
use crate::txn_safe_point_oracle_test::{bytes, records};
use astersql_pd_gc::GCStateManager;
use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
use astersql_pd_keyspace::KeyspaceManager;
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_storage_domain::{
    BarrierTime, Base, Keyspace, KeyspaceIdentity, MetaStore, StorageEndpoint,
};
use astersql_pd_typeutil::Duration as GoDuration;
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{collections::HashMap, sync::Arc, time::Duration};
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires admitted read-only Go binary and official etcd"]
async fn mcs_046_global_barrier_go_differential() {
    let (mut go, cluster) = crate::group_merge_oracle_test::start_bootstrapped_go(None).await;
    let gs: Arc<dyn MetaStore> = Arc::new(
        astersql_pd_storage_kv::EtcdMetaStore::connect(&[go.url.clone()], Duration::from_secs(2))
            .await
            .unwrap(),
    );
    let mut fixture = crate::harness::fixture::Fixture::start();
    let rs: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let gp = ClusterPaths::new(cluster);
    let rp = ClusterPaths::new(42);
    let mut dataset = vec![
        (1, "first".into(), 0, 20),
        (2, "unified".into(), 0, 30),
        (3, "level".into(), 0, 25),
        (4, "archived".into(), 2, 9999),
        (5, "disabled".into(), 1, 99999),
        (6, "tombstone".into(), 3, 999999),
        (7, "unknown".into(), 99, u64::MAX),
    ];
    for id in 100..=204 {
        dataset.push((
            id,
            if id == 204 {
                "终点\"\0".into()
            } else {
                format!("p{id}")
            },
            0,
            if id == 204 { 40 } else { 0 },
        ));
    }
    for (store, p) in [(&gs, &gp), (&rs, &rp)] {
        let wire = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
        for (id, name, state, value) in &dataset {
            let mut tx = Base::new(store.as_ref()).begin_txn();
            wire.save_keyspace_meta(
                &mut tx,
                p,
                &Keyspace {
                    identity: Some(KeyspaceIdentity::Id(*id)),
                    name: name.clone(),
                    state: *state,
                    config: if *id == 3 {
                        HashMap::from([("gc_management_type".into(), "keyspace_level".into())])
                    } else {
                        HashMap::new()
                    },
                    ..Default::default()
                },
            )
            .unwrap();
            tx.commit::<String>().await.unwrap();
            let ep = StorageEndpoint::new(store.clone());
            let mut tx = ep.begin_gc_state_transaction(p).await.unwrap();
            tx.batch.set_txn_safe_point(*id, *value);
            tx.commit().await.unwrap();
        }
        let ep = StorageEndpoint::new(store.clone());
        let mut tx = ep.begin_gc_state_transaction(p).await.unwrap();
        tx.batch.set_txn_safe_point(NULL_KEYSPACE_ID, 10);
        tx.batch.set_txn_safe_point(5000, u64::MAX);
        tx.commit().await.unwrap();
    }
    let keys = Arc::new(KeyspaceManager::new(
        astersql_pd_storage_endpoint::StorageEndpoint::new(rs.clone()).keyspace_storage(rp.clone()),
    ));
    let m = GCStateManager::new(
        StorageEndpoint::new(rs.clone()),
        rp.clone(),
        keys,
        tokio::runtime::Handle::current(),
    );
    let role = m.on_node_becomes_leader().await;
    let mut client = PdClient::connect(go.url.clone()).await.unwrap();
    let mut report = vec![];
    let cases = [
        (
            "behind-unified-page",
            "global",
            39,
            i64::MAX,
            10,
            20,
            30,
            false,
            false,
        ),
        (
            "at-maximum",
            "global",
            40,
            i64::MAX,
            10,
            20,
            30,
            false,
            false,
        ),
        ("raise", "global", 60, i64::MAX, 10, 20, 30, false, false),
        ("shrink", "global", 40, i64::MAX, 10, 20, 30, false, false),
        (
            "null-maximum",
            "global",
            49,
            i64::MAX,
            50,
            20,
            30,
            false,
            false,
        ),
        (
            "metadata-tie",
            "global",
            49,
            i64::MAX,
            50,
            50,
            30,
            false,
            false,
        ),
        (
            "unified-stale",
            "global",
            99,
            i64::MAX,
            10,
            20,
            100,
            false,
            false,
        ),
        ("finite", "finite", 40, 60, 10, 20, 30, false, false),
        (
            "global-reserved-allowed",
            "gc_worker",
            40,
            i64::MAX,
            10,
            20,
            30,
            false,
            false,
        ),
        ("ttl-first", "", 40, 0, 10, 20, 30, false, false),
        ("empty-id", "", 40, i64::MAX, 10, 20, 30, false, false),
        (
            "corrupt-metadata",
            "global",
            100,
            i64::MAX,
            10,
            20,
            30,
            true,
            false,
        ),
        (
            "maximum-metadata-id",
            "global",
            100,
            i64::MAX,
            10,
            20,
            30,
            false,
            true,
        ),
    ];
    for (label, name, ts, ttl, null, first, unified, corrupt, maximum) in cases {
        for (store, p) in [(&gs, &gp), (&rs, &rp)] {
            let ep = StorageEndpoint::new(store.clone());
            let mut tx = ep.begin_gc_state_transaction(p).await.unwrap();
            for (id, value) in [(NULL_KEYSPACE_ID, null), (1, first), (2, unified)] {
                tx.batch.set_txn_safe_point(id, value);
            }
            tx.commit().await.unwrap();
            if corrupt {
                Base::new(store.as_ref())
                    .save(p.keyspace_meta_path(300).as_bytes(), b"\xff")
                    .await
                    .unwrap();
            }
            if maximum {
                let mut tx = Base::new(store.as_ref()).begin_txn();
                astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone())
                    .save_keyspace_meta(
                        &mut tx,
                        p,
                        &Keyspace {
                            identity: Some(KeyspaceIdentity::Id(0xffffff)),
                            name: "maximum".into(),
                            ..Default::default()
                        },
                    )
                    .unwrap();
                tx.commit::<String>().await.unwrap();
            }
        }
        let before_go = records(gs.as_ref(), &gp, NULL_KEYSPACE_ID).await;
        let before_rust = records(rs.as_ref(), &rp, NULL_KEYSPACE_ID).await;
        let request = pdpb::SetGlobalGcBarrierRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: cluster,
                ..Default::default()
            }),
            barrier_id: name.into(),
            barrier_ts: ts,
            ttl_seconds: ttl,
        };
        let response = client
            .set_global_gc_barrier(request.clone())
            .await
            .unwrap()
            .into_inner();
        let duration = astersql_pd_typeutil::saturating_std_duration_from_seconds(ttl);
        let stored = if response.header.as_ref().unwrap().error.is_none() {
            StorageEndpoint::new(gs.clone())
                .load_global_gc_barrier(&gp, name)
                .await
                .unwrap()
        } else {
            None
        };
        let now = stored.as_ref().and_then(|b| b.expiration_time).map_or(
            BarrierTime::from_unix(1_800_000_000, 0),
            |time| {
                BarrierTime::from_unix(
                    time.unix_seconds() - duration.as_secs() as i64 - 1,
                    500_000_000,
                )
            },
        );
        let rust = m
            .set_global_gc_barrier(
                name,
                ts,
                GoDuration::from_nanos(duration.as_nanos() as i64),
                now,
            )
            .await;
        match (&response.header.as_ref().unwrap().error, &rust) {
            (None, Ok(value)) => {
                assert_eq!(stored.as_ref(), Some(value));
                let info = response.new_barrier_info.as_ref().unwrap();
                assert_eq!(
                    (&info.barrier_id, info.barrier_ts),
                    (&value.barrier_id, value.barrier_ts)
                );
                assert_eq!(
                    info.ttl_seconds,
                    if value.expiration_time.is_none() {
                        i64::MAX
                    } else {
                        ttl
                    }
                );
            }
            (Some(error), Err(rust)) => {
                if corrupt {
                    assert_eq!(error.message, "unexpected EOF", "{response:?}");
                    assert!(matches!(rust, astersql_pd_gc::GCReadError::Keyspace(_)));
                } else {
                    assert_eq!(error.message, rust.to_string(), "{label}");
                }
            }
            _ => panic!("{label}: {response:?} / {rust:?}"),
        }
        let after_go = records(gs.as_ref(), &gp, NULL_KEYSPACE_ID).await;
        let after_rust = records(rs.as_ref(), &rp, NULL_KEYSPACE_ID).await;
        assert_eq!(bytes(&after_go), bytes(&after_rust));
        if rust.is_err() {
            for i in 0..6 {
                assert_eq!(before_go[i]["entries"], after_go[i]["entries"]);
                assert_eq!(before_rust[i]["entries"], after_rust[i]["entries"]);
            }
        }
        report.push(json!({"case":label,"request":format!("{request:?}"),"go":format!("{response:?}"),"rust":format!("{rust:?}"),"rust_clock":format!("{now:?}"),"before_go":before_go,"before_rust":before_rust,"after_go":after_go,"after_rust":after_rust,"dataset":format!("{dataset:?}"),"null":null,"first":first,"unified":unified,"corrupt_metadata":corrupt,"maximum_metadata":maximum}));
        for (store, p) in [(&gs, &gp), (&rs, &rp)] {
            if corrupt {
                Base::new(store.as_ref())
                    .remove(p.keyspace_meta_path(300).as_bytes())
                    .await
                    .unwrap();
            }
            if maximum {
                Base::new(store.as_ref())
                    .remove(p.keyspace_meta_path(0xffffff).as_bytes())
                    .await
                    .unwrap();
            }
        }
    }
    for name in ["global", "finite", "gc_worker", "absent", ""] {
        let request = pdpb::DeleteGlobalGcBarrierRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: cluster,
                ..Default::default()
            }),
            barrier_id: name.into(),
        };
        let wall_before = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as i128;
        let response = client
            .delete_global_gc_barrier(request)
            .await
            .unwrap()
            .into_inner();
        let wall_after = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as i128;
        let rust = m.delete_global_gc_barrier(name).await;
        match (&response.header.as_ref().unwrap().error, &rust) {
            (None, Ok(value)) => {
                assert_eq!(response.deleted_barrier_info.is_some(), value.is_some());
                if let (Some(go), Some(rust)) = (&response.deleted_barrier_info, value) {
                    assert_eq!(
                        (&go.barrier_id, go.barrier_ts),
                        (&rust.barrier_id, rust.barrier_ts)
                    );
                    if let Some(expiration) = rust.expiration_time {
                        let expire = expiration.unix_seconds() as i128 * 1_000_000_000;
                        let low = ((expire - wall_after).div_euclid(1_000_000_000)).max(0);
                        let high = ((expire - wall_before).div_euclid(1_000_000_000)).max(0);
                        assert!((low..=high).contains(&(go.ttl_seconds as i128)));
                    } else {
                        assert_eq!(go.ttl_seconds, i64::MAX);
                    }
                }
            }
            (Some(go), Err(rust)) => assert_eq!(go.message, rust.to_string()),
            _ => panic!("{response:?}/{rust:?}"),
        }
        assert_eq!(
            bytes(&records(gs.as_ref(), &gp, NULL_KEYSPACE_ID).await),
            bytes(&records(rs.as_ref(), &rp, NULL_KEYSPACE_ID).await)
        );
        report.push(json!({"delete":name,"go":format!("{response:?}"),"rust":format!("{rust:?}")}));
    }
    role.shutdown().await;
    go.stop().unwrap();
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    go.restart_with_args(&crate::readonly_oracle::pinned_go_binary().unwrap(), &[])
        .unwrap();
    go.ready().unwrap();
    let recovered_go =
        astersql_pd_storage_kv::EtcdMetaStore::connect(&[go.url.clone()], Duration::from_secs(2))
            .await
            .unwrap();
    let recovered_rust = fixture.store().await;
    let go_state = records(&recovered_go, &gp, NULL_KEYSPACE_ID).await;
    let rust_state = records(&recovered_rust, &rp, NULL_KEYSPACE_ID).await;
    assert_eq!(bytes(&go_state), bytes(&rust_state));
    report.push(json!({"recovered_go":go_state,"recovered_rust":rust_state}));
    if let Some(path) = std::env::var_os("MCS_046_ORACLE_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
}
