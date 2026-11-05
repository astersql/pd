// Copyright 2026 AsterSQL.
use crate::txn_safe_point_oracle_test::{bytes, records};
use astersql_pd_gc::GCStateManager;
use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
use astersql_pd_keyspace::KeyspaceManager;
use astersql_pd_proto::pdpb::{self, keyspace_scope, pd_client::PdClient};
use astersql_pd_storage_domain::{
    BarrierTime, Base, Keyspace, KeyspaceIdentity, MetaStore, StorageEndpoint,
};
use astersql_pd_typeutil::Duration as GoDuration;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
fn scope(id: u32) -> Option<pdpb::KeyspaceScope> {
    if id == NULL_KEYSPACE_ID {
        None
    } else {
        Some(pdpb::KeyspaceScope {
            keyspace: Some(keyspace_scope::Keyspace::KeyspaceId(id)),
        })
    }
}
fn wall() -> i128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as i128
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires admitted read-only Go binary and official etcd"]
async fn mcs_045_barrier_go_differential() {
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
    for (store, p) in [(&gs, &gp), (&rs, &rp)] {
        for (id, level) in [(1001, true), (1002, false)] {
            let mut tx = Base::new(store.as_ref()).begin_txn();
            astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone())
                .save_keyspace_meta(
                    &mut tx,
                    p,
                    &Keyspace {
                        identity: Some(KeyspaceIdentity::Id(id)),
                        name: format!("barrier45_{id}"),
                        config: if level {
                            HashMap::from([("gc_management_type".into(), "keyspace_level".into())])
                        } else {
                            HashMap::new()
                        },
                        ..Default::default()
                    },
                )
                .unwrap();
            tx.commit::<String>().await.unwrap();
        }
        let ep = StorageEndpoint::new(store.clone());
        let mut tx = ep.begin_gc_state_transaction(p).await.unwrap();
        for id in [NULL_KEYSPACE_ID, 1001, 1002] {
            tx.batch.set_txn_safe_point(id, 10);
            tx.batch.set_gc_safe_point(id, 5);
        }
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
        (NULL_KEYSPACE_ID, "tiny", 10, 1, true),
        (NULL_KEYSPACE_ID, "finite", 20, 60, true),
        (NULL_KEYSPACE_ID, "finite-big", 20, 9_223_372_036, true),
        (NULL_KEYSPACE_ID, "infinite", 20, 9_223_372_037, true),
        (NULL_KEYSPACE_ID, "max", 20, i64::MAX, true),
        (NULL_KEYSPACE_ID, "gc_worker", 20, i64::MAX, true),
        (NULL_KEYSPACE_ID, "", 20, i64::MAX, true),
        (NULL_KEYSPACE_ID, "bad-ttl", 20, 0, true),
        (NULL_KEYSPACE_ID, "bad-negative", 20, -1, true),
        (NULL_KEYSPACE_ID, "behind", 9, i64::MAX, true),
        (1001, "scoped", 20, i64::MAX, true),
        (1002, "unified", 20, i64::MAX, true),
        (1009, "missing", 20, 0, true),
        (1009, "gc_worker", 20, i64::MAX, true),
        (0x0100_0000, "alias", 20, i64::MAX, true),
        (NULL_KEYSPACE_ID, "服务", 20, i64::MAX, true),
        (NULL_KEYSPACE_ID, "native_br", 20, i64::MAX, true),
        (NULL_KEYSPACE_ID, "path/服务", 20, i64::MAX, true),
        (NULL_KEYSPACE_ID, "rewrite", 30, i64::MAX, false),
        (NULL_KEYSPACE_ID, "rewrite", 25, i64::MAX, true),
    ];
    for (id, name, ts, ttl, delete) in cases {
        let actual = if id == 1002 || id > 0xffffff {
            NULL_KEYSPACE_ID
        } else {
            id
        };
        let before_go = records(gs.as_ref(), &gp, actual).await;
        let before_rust = records(rs.as_ref(), &rp, actual).await;
        let request = pdpb::SetGcBarrierRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: cluster,
                ..Default::default()
            }),
            keyspace_scope: scope(id),
            barrier_id: name.into(),
            barrier_ts: ts,
            ttl_seconds: ttl,
        };
        let response = client
            .set_gc_barrier(request.clone())
            .await
            .unwrap()
            .into_inner();
        let duration = astersql_pd_typeutil::saturating_std_duration_from_seconds(ttl);
        let nanos = duration.as_nanos() as i64;
        let stored = if response.header.as_ref().unwrap().error.is_none() {
            StorageEndpoint::new(gs.clone())
                .load_gc_barrier(&gp, actual, name)
                .await
                .unwrap()
        } else {
            None
        };
        // The Go API does not expose its now. Choose an explicit clock from the same observable rounding interval.
        let now = stored.as_ref().and_then(|b| b.expiration_time).map_or(
            BarrierTime::from_unix(1_800_000_000, 0),
            |expiration| {
                BarrierTime::from_unix(
                    expiration.unix_seconds() - duration.as_secs() as i64 - 1,
                    500_000_000,
                )
            },
        );
        let rust = m
            .set_gc_barrier(id, name, ts, GoDuration::from_nanos(nanos), now)
            .await;
        match (&response.header.as_ref().unwrap().error, &rust) {
            (None, Ok(barrier)) => {
                assert_eq!(Some(barrier), stored.as_ref());
                let info = response.new_barrier_info.as_ref().unwrap();
                assert_eq!(
                    (&info.barrier_id, info.barrier_ts),
                    (&barrier.barrier_id, barrier.barrier_ts)
                );
                assert_eq!(
                    info.ttl_seconds,
                    if barrier.expiration_time.is_none() {
                        i64::MAX
                    } else {
                        duration.as_secs() as i64
                    }
                );
            }
            (Some(error), Err(rust)) => assert_eq!(error.message, rust.to_string()),
            _ => panic!("{request:?}: {response:?} / {rust:?}"),
        }
        let after_go = records(gs.as_ref(), &gp, actual).await;
        let after_rust = records(rs.as_ref(), &rp, actual).await;
        assert_eq!(bytes(&after_go), bytes(&after_rust));
        if rust.is_err() {
            for i in 0..6 {
                assert_eq!(before_go[i]["entries"], after_go[i]["entries"]);
                assert_eq!(before_rust[i]["entries"], after_rust[i]["entries"]);
            }
        }
        report.push(json!({"request":format!("{request:?}"),"go":format!("{response:?}"),"rust":format!("{rust:?}"),"rust_clock":format!("{now:?}"),"clock_policy":"finite expiry: observed expiry minus integer TTL minus 0.5 seconds, inside its equivalent rounding interval; exact subsecond boundary verified natively","before_go":before_go,"before_rust":before_rust,"after_go":after_go,"after_rust":after_rust}));
        if rust.is_ok() && delete {
            let before = wall();
            let removed = client
                .delete_gc_barrier(pdpb::DeleteGcBarrierRequest {
                    header: Some(pdpb::RequestHeader {
                        cluster_id: cluster,
                        ..Default::default()
                    }),
                    keyspace_scope: scope(id),
                    barrier_id: name.into(),
                })
                .await
                .unwrap()
                .into_inner();
            let after = wall();
            assert!(removed.header.as_ref().unwrap().error.is_none());
            let rust = m.delete_gc_barrier(id, name).await.unwrap().unwrap();
            let info = removed.deleted_barrier_info.as_ref().unwrap();
            assert_eq!(
                (&info.barrier_id, info.barrier_ts),
                (&rust.barrier_id, rust.barrier_ts)
            );
            let bounds = if let Some(expiration) = rust.expiration_time {
                let exp = expiration.unix_seconds() as i128 * 1_000_000_000;
                let low = ((exp - after).div_euclid(1_000_000_000)).max(0);
                let high = ((exp - before).div_euclid(1_000_000_000)).max(0);
                assert!(
                    (low..=high).contains(&(info.ttl_seconds as i128)),
                    "delete TTL must belong to actual request wall-time interval"
                );
                Some((low, high))
            } else {
                assert_eq!(info.ttl_seconds, i64::MAX);
                None
            };
            let again = client
                .delete_gc_barrier(pdpb::DeleteGcBarrierRequest {
                    header: Some(pdpb::RequestHeader {
                        cluster_id: cluster,
                        ..Default::default()
                    }),
                    keyspace_scope: scope(id),
                    barrier_id: name.into(),
                })
                .await
                .unwrap()
                .into_inner();
            assert!(again.header.as_ref().unwrap().error.is_none());
            assert!(again.deleted_barrier_info.is_none());
            assert!(m.delete_gc_barrier(id, name).await.unwrap().is_none());
            let go_state = records(gs.as_ref(), &gp, actual).await;
            let rust_state = records(rs.as_ref(), &rp, actual).await;
            assert_eq!(bytes(&go_state), bytes(&rust_state));
            report.push(json!({"delete_go":format!("{removed:?}"),"delete_rust":format!("{rust:?}"),"ttl_wall_bounds":bounds,"absent_go":format!("{again:?}"),"go_state":go_state,"rust_state":rust_state}));
        }
    }
    for (store, p) in [(&gs, &gp), (&rs, &rp)] {
        Base::new(store.as_ref())
            .save(
                p.gc_barrier_path(NULL_KEYSPACE_ID, "gc_worker").as_bytes(),
                b"corrupt",
            )
            .await
            .unwrap();
    }
    let forced = crate::meta_group_assignment_oracle_test::http(
        &format!("{}/pd/api/v1/gc/safepoint/gc_worker", go.url),
        "DELETE",
        Value::Null,
    );
    assert_eq!(forced.0, 200);
    m.force_delete_service_gc_safe_point("gc_worker")
        .await
        .unwrap();
    let go_state = records(gs.as_ref(), &gp, NULL_KEYSPACE_ID).await;
    let rust_state = records(rs.as_ref(), &rp, NULL_KEYSPACE_ID).await;
    assert_eq!(bytes(&go_state), bytes(&rust_state));
    report.push(json!({"force_http":forced,"go_state":go_state,"rust_state":rust_state}));
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
    if let Some(path) = std::env::var_os("MCS_045_ORACLE_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
}
