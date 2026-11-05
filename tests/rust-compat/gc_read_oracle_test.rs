// Copyright 2026 AsterSQL.
use astersql_pd_gc::GCStateManager;
use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
use astersql_pd_keyspace::KeyspaceManager;
use astersql_pd_proto::pdpb::{self, keyspace_scope, pd_client::PdClient};
use astersql_pd_storage_domain::{
    BarrierTime, Base, GCBarrier, GlobalGCBarrier, Keyspace, KeyspaceIdentity, MetaStore,
    StorageEndpoint,
};
use serde_json::json;
use std::{collections::HashMap, sync::Arc, time::Duration};
fn scope(id: u32) -> pdpb::KeyspaceScope {
    pdpb::KeyspaceScope {
        keyspace: Some(keyspace_scope::Keyspace::KeyspaceId(id)),
    }
}
fn dto(s: &astersql_pd_gc::GCState) -> pdpb::GcState {
    pdpb::GcState {
        keyspace_scope: Some(scope(s.keyspace_id)),
        is_keyspace_level_gc: s.is_keyspace_level,
        txn_safe_point: s.txn_safe_point,
        gc_safe_point: s.gc_safe_point,
        gc_barriers: s
            .gc_barriers
            .iter()
            .map(|b| pdpb::GcBarrierInfo {
                barrier_id: b.barrier_id.clone(),
                barrier_ts: b.barrier_ts,
                ttl_seconds: if b.expiration_time.is_none() {
                    i64::MAX
                } else {
                    0
                },
            })
            .collect(),
    }
}
async fn physical(store: &dyn MetaStore, p: &ClusterPaths) -> serde_json::Value {
    let mut keys = vec![p.gc_state_revision_path(), p.global_gc_barrier_prefix()];
    for id in [NULL_KEYSPACE_ID, 1001, 1002, 1003] {
        keys.extend([
            p.gc_safe_point_path(id),
            p.txn_safe_point_path(id),
            p.gc_barrier_prefix(id),
        ]);
    }
    for id in [1001, 1002, 1003] {
        keys.extend([
            p.keyspace_meta_path(id),
            p.keyspace_id_path(&format!("gc43_{id}")),
        ]);
    }
    let mut records = vec![];
    for key in keys {
        let snapshot = store
            .read(astersql_pd_storage_domain::Read::prefix(
                key.as_bytes(),
                astersql_pd_storage_domain::Consistency::Linearizable,
            ))
            .await
            .unwrap();
        records.push(json!({"key":key,"entries":snapshot.entries.iter().map(|e|json!({"key":e.key,"bytes":e.value,"create_revision":e.create_revision,"mod_revision":e.mod_revision,"lease":e.lease})).collect::<Vec<_>>()}));
    }
    json!(records)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires admitted read-only Go binary and official etcd"]
async fn mcs_043_state_read_go_differential() {
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
        let wire = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
        for (id, level, state) in [(1001, true, 0), (1002, false, 0), (1003, true, 2)] {
            let meta = Keyspace {
                identity: Some(KeyspaceIdentity::Id(id)),
                name: format!("gc43_{id}"),
                state,
                config: if level {
                    HashMap::from([("gc_management_type".into(), "keyspace_level".into())])
                } else {
                    HashMap::new()
                },
                ..Default::default()
            };
            let mut txn = Base::new(store.as_ref()).begin_txn();
            wire.save_keyspace_meta(&mut txn, p, &meta).unwrap();
            txn.save(
                p.keyspace_id_path(&meta.name).as_bytes(),
                id.to_string().as_bytes(),
            )
            .unwrap();
            txn.commit::<String>().await.unwrap();
        }
        let ep = StorageEndpoint::new(store.clone());
        let mut transaction = ep.begin_gc_state_transaction(p).await.unwrap();
        for id in [NULL_KEYSPACE_ID, 1001, 1002, 1003] {
            transaction.batch.set_txn_safe_point(
                id,
                if id == NULL_KEYSPACE_ID {
                    99
                } else {
                    id as u64
                },
            );
            transaction.batch.set_gc_safe_point(
                id,
                if id == NULL_KEYSPACE_ID {
                    19
                } else {
                    id as u64 - 10
                },
            );
            let mut expiration = BarrierTime::from_unix(1, 0);
            transaction
                .batch
                .set_gc_barrier(id, &GCBarrier::new("expired", 2000, Some(&mut expiration)))
                .unwrap();
            transaction
                .batch
                .set_gc_barrier(id, &GCBarrier::new("forever", 3000, None))
                .unwrap();
            transaction
                .batch
                .set_gc_barrier(id, &GCBarrier::new("gc_worker", 1000, None))
                .unwrap();
        }
        transaction
            .batch
            .set_global_gc_barrier(&GlobalGCBarrier::new("global", 4000, None))
            .unwrap();
        transaction.commit().await.unwrap();
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
    let before_go = physical(gs.as_ref(), &gp).await;
    let before_rust = physical(rs.as_ref(), &rp).await;
    for id in [NULL_KEYSPACE_ID, 1001, 1002, 1003, 0x0100_0000] {
        for exclude in [false, true] {
            for globals in [false, true] {
                let request = pdpb::GetGcStateRequest {
                    header: Some(pdpb::RequestHeader {
                        cluster_id: cluster,
                        ..Default::default()
                    }),
                    keyspace_scope: if id == NULL_KEYSPACE_ID {
                        None
                    } else {
                        Some(scope(id))
                    },
                    exclude_gc_barriers: exclude,
                    include_global_gc_barriers: globals,
                };
                let response = client
                    .get_gc_state(request.clone())
                    .await
                    .unwrap()
                    .into_inner();
                assert!(
                    response.header.as_ref().unwrap().error.is_none(),
                    "{response:?}"
                );
                let (rust, global) = if globals {
                    m.get_gc_state_with_global_barriers(id, exclude)
                        .await
                        .unwrap()
                } else {
                    (m.get_gc_state(id, exclude).await.unwrap(), vec![])
                };
                assert_eq!(response.gc_state, Some(dto(&rust)));
                let expected_global = if globals {
                    Some(pdpb::GlobalGcBarriersInfo {
                        barriers: global
                            .iter()
                            .map(|b| pdpb::GlobalGcBarrierInfo {
                                barrier_id: b.barrier_id.clone(),
                                barrier_ts: b.barrier_ts,
                                ttl_seconds: i64::MAX,
                            })
                            .collect(),
                    })
                } else {
                    None
                };
                assert_eq!(response.global_gc_barriers, expected_global);
                report.push(json!({"input":format!("{request:?}"),"go":format!("{response:?}"),"rust":format!("{rust:?}"),"global":format!("{global:?}")}));
            }
        }
    }
    let missing = client
        .get_gc_state(pdpb::GetGcStateRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: cluster,
                ..Default::default()
            }),
            keyspace_scope: Some(scope(1009)),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    let rust_error = m.get_gc_state(1009, false).await.unwrap_err().to_string();
    assert_eq!(missing.header.unwrap().error.unwrap().message, rust_error);
    report.push(json!({"missing_error":rust_error}));
    let after_go = physical(gs.as_ref(), &gp).await;
    let after_rust = physical(rs.as_ref(), &rp).await;
    assert_eq!(before_go, after_go);
    assert_eq!(before_rust, after_rust);
    report.push(json!({"read_only_before_go":before_go,"read_only_after_go":after_go,"read_only_before_rust":before_rust,"read_only_after_rust":after_rust}));
    // Read errors preserve their original messages; compare categories, then repair and require a fresh fill.
    for (store, p) in [(&gs, &gp), (&rs, &rp)] {
        Base::new(store.as_ref())
            .save(p.txn_safe_point_path(1003).as_bytes(), b"invalid")
            .await
            .unwrap();
    }
    let go_error = client
        .get_gc_state(pdpb::GetGcStateRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: cluster,
                ..Default::default()
            }),
            keyspace_scope: Some(scope(1003)),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .error
        .unwrap()
        .message;
    let rust_error = m.get_gc_state(1003, false).await.unwrap_err();
    assert!(go_error.contains("ParseUint") || go_error.contains("parse uint"));
    assert!(matches!(
        rust_error,
        astersql_pd_gc::GCReadError::SafePoint(_)
    ));
    report.push(json!({"go_decode_error":go_error,"rust_decode_error":rust_error.to_string(),"comparison":"typed uint syntax failure; original strings retained"}));
    for (store, p) in [(&gs, &gp), (&rs, &rp)] {
        Base::new(store.as_ref())
            .save(p.txn_safe_point_path(1003).as_bytes(), b"2001")
            .await
            .unwrap();
    }
    let repaired = client
        .get_gc_state(pdpb::GetGcStateRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: cluster,
                ..Default::default()
            }),
            keyspace_scope: Some(scope(1003)),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        repaired.gc_state,
        Some(dto(&m.get_gc_state(1003, false).await.unwrap()))
    );
    report.push(json!({"repaired":format!("{repaired:?}")}));
    role.shutdown().await;
    go.stop().unwrap();
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    go.restart_with_args(&crate::readonly_oracle::pinned_go_binary().unwrap(), &[])
        .unwrap();
    go.ready().unwrap();
    let recovered = StorageEndpoint::new(Arc::new(fixture.store().await));
    assert_eq!(
        recovered.load_txn_safe_point(&rp, 1003).await.unwrap(),
        2001
    );
    let mut client = PdClient::connect(go.url.clone()).await.unwrap();
    let restarted = client
        .get_gc_state(pdpb::GetGcStateRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: cluster,
                ..Default::default()
            }),
            keyspace_scope: Some(scope(1003)),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(restarted.gc_state, repaired.gc_state);
    report.push(json!({"restarted":format!("{restarted:?}")}));
    if let Some(path) = std::env::var_os("MCS_043_ORACLE_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires admitted read-only Go binary and official etcd"]
async fn mcs_053_list_go_differential() {
    let (go, cluster) = crate::group_merge_oracle_test::start_bootstrapped_go(None).await;
    let gs: Arc<dyn MetaStore> = Arc::new(
        astersql_pd_storage_kv::EtcdMetaStore::connect(&[go.url.clone()], Duration::from_secs(2))
            .await
            .unwrap(),
    );
    let fixture = crate::harness::fixture::Fixture::start();
    let rs: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let gp = ClusterPaths::new(cluster);
    let rp = ClusterPaths::new(42);
    for (store, p) in [(&gs, &gp), (&rs, &rp)] {
        let wire = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
        for id in 1001..1125 {
            let meta = Keyspace {
                identity: Some(KeyspaceIdentity::Id(id)),
                name: format!("gc53_{id}"),
                state: if id == 1003 { 2 } else { 0 },
                config: if id != 1002 {
                    HashMap::from([("gc_management_type".into(), "keyspace_level".into())])
                } else {
                    HashMap::new()
                },
                ..Default::default()
            };
            let mut tx = Base::new(store.as_ref()).begin_txn();
            wire.save_keyspace_meta(&mut tx, p, &meta).unwrap();
            tx.commit::<String>().await.unwrap();
        }
        let ep = StorageEndpoint::new(store.clone());
        let mut tx = ep.begin_gc_state_transaction(p).await.unwrap();
        for id in [NULL_KEYSPACE_ID, 1001, 1002, 1003] {
            tx.batch.set_txn_safe_point(id, 99);
            tx.batch.set_gc_safe_point(id, 19);
            tx.batch
                .set_gc_barrier(
                    id,
                    &GCBarrier {
                        barrier_id: "forever".into(),
                        barrier_ts: 100,
                        expiration_time: None,
                    },
                )
                .unwrap();
        }
        tx.commit().await.unwrap();
    }
    let keys = Arc::new(KeyspaceManager::new(
        astersql_pd_storage_endpoint::StorageEndpoint::new(rs.clone()).keyspace_storage(rp.clone()),
    ));
    let manager = Arc::new(GCStateManager::new(
        StorageEndpoint::new(rs.clone()),
        rp.clone(),
        keys,
        tokio::runtime::Handle::current(),
    ));
    let role = manager.on_node_becomes_leader().await;
    let mut client = PdClient::connect(go.url.clone()).await.unwrap();
    let mut report = vec![];
    for exclude in [true, false, true] {
        let before_g = physical(gs.as_ref(), &gp).await;
        let before_r = physical(rs.as_ref(), &rp).await;
        let response = client
            .get_all_keyspaces_gc_states(pdpb::GetAllKeyspacesGcStatesRequest {
                header: Some(pdpb::RequestHeader {
                    cluster_id: cluster,
                    ..Default::default()
                }),
                exclude_gc_barriers: exclude,
                exclude_global_gc_barriers: true,
            })
            .await
            .unwrap()
            .into_inner();
        assert!(response.header.unwrap().error.is_none());
        let mut expected = response.gc_states;
        expected.retain(|s|matches!(s.keyspace_scope.as_ref().and_then(|s|s.keyspace.as_ref()),Some(keyspace_scope::Keyspace::KeyspaceId(id)) if *id>=1001));
        expected.sort_by_key(|s| format!("{:?}", s.keyspace_scope));
        let states = manager.get_all_keyspaces_gc_states(exclude).await.unwrap();
        let mut actual: Vec<_> = states.values().map(dto).collect();
        actual.sort_by_key(|s| format!("{:?}", s.keyspace_scope));
        assert_eq!(expected, actual);
        assert_eq!(before_g, physical(gs.as_ref(), &gp).await);
        assert_eq!(before_r, physical(rs.as_ref(), &rp).await);
        report.push(json!({"exclude":exclude,"go":format!("{expected:?}"),"rust":format!("{actual:?}"),"go_before":before_g,"rust_before":before_r}));
    }
    role.shutdown().await;
    if let Some(path) = std::env::var_os("PD_SA053_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
}
