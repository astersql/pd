// Copyright 2026 AsterSQL.
use astersql_pd_gc::GCStateManager;
use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
use astersql_pd_keyspace::KeyspaceManager;
use astersql_pd_proto::pdpb::{self, keyspace_scope, pd_client::PdClient};
use astersql_pd_storage_domain::{
    BarrierTime, Base, Consistency, GCBarrier, GlobalGCBarrier, Keyspace, KeyspaceIdentity,
    MetaStore, Read, StorageEndpoint,
};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc, time::Duration};
#[derive(Clone, Debug)]
struct Case {
    name: &'static str,
    id: u32,
    old: u64,
    target: u64,
    locals: Vec<(&'static str, u64, i64)>,
    globals: Vec<(&'static str, u64, i64)>,
    min: Option<&'static str>,
    bad_gc: bool,
}
impl Case {
    fn new(name: &'static str, old: u64, target: u64) -> Self {
        Self {
            name,
            id: NULL_KEYSPACE_ID,
            old,
            target,
            locals: vec![],
            globals: vec![],
            min: None,
            bad_gc: false,
        }
    }
}
async fn records(store: &dyn MetaStore, p: &ClusterPaths, id: u32) -> Value {
    let keys = [
        p.gc_state_revision_path(),
        p.txn_safe_point_path(id),
        p.gc_safe_point_path(id),
        p.gc_barrier_prefix(id),
        p.global_gc_barrier_prefix(),
        p.compatible_tidb_min_start_ts_prefix(id),
    ];
    let mut values = vec![];
    for key in keys {
        let snapshot = store
            .read(Read::prefix(key.as_bytes(), Consistency::Linearizable))
            .await
            .unwrap();
        values.push(json!({"key":key,"header_revision":snapshot.revision,"entries":snapshot.entries.iter().map(|e|json!({"key":e.key,"suffix":e.key[key.len()..],"bytes":e.value,"create_revision":e.create_revision,"mod_revision":e.mod_revision})).collect::<Vec<_>>()}));
    }
    json!(values)
}
fn bytes(state: &Value) -> Value {
    json!(
        state
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["entries"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| json!({"suffix":e["suffix"],"bytes":e["bytes"]}))
                .collect::<Vec<_>>())
            .collect::<Vec<_>>()
    )
}
async fn seed(store: Arc<dyn MetaStore>, p: &ClusterPaths, c: &Case) {
    let scope = if c.id > 0xffffff {
        NULL_KEYSPACE_ID
    } else {
        c.id
    };
    let ep = StorageEndpoint::new(store.clone());
    for id in ["a", "b", "expired", "finite", "gc_worker", "服务"] {
        Base::new(store.as_ref())
            .remove(p.gc_barrier_path(scope, id).as_bytes())
            .await
            .unwrap();
    }
    for id in ["global", "expired-global"] {
        Base::new(store.as_ref())
            .remove(p.global_gc_barrier_path(id).as_bytes())
            .await
            .unwrap();
    }
    let min = format!("{}server", p.compatible_tidb_min_start_ts_prefix(scope));
    Base::new(store.as_ref())
        .remove(min.as_bytes())
        .await
        .unwrap();
    let mut txn = ep.begin_gc_state_transaction(p).await.unwrap();
    txn.batch.set_txn_safe_point(scope, c.old);
    txn.batch.set_gc_safe_point(scope, 5);
    for &(id, ts, expiration) in &c.locals {
        let mut time = BarrierTime::from_unix(expiration, 0);
        txn.batch
            .set_gc_barrier(
                scope,
                &GCBarrier::new(
                    id,
                    ts,
                    if expiration == 0 {
                        None
                    } else {
                        Some(&mut time)
                    },
                ),
            )
            .unwrap();
    }
    for &(id, ts, expiration) in &c.globals {
        let mut time = BarrierTime::from_unix(expiration, 0);
        txn.batch
            .set_global_gc_barrier(&GlobalGCBarrier::new(
                id,
                ts,
                if expiration == 0 {
                    None
                } else {
                    Some(&mut time)
                },
            ))
            .unwrap();
    }
    txn.commit().await.unwrap();
    if let Some(value) = c.min {
        Base::new(store.as_ref())
            .save(min.as_bytes(), value.as_bytes())
            .await
            .unwrap();
    }
    if c.bad_gc {
        Base::new(store.as_ref())
            .save(p.gc_safe_point_path(scope).as_bytes(), b"invalid")
            .await
            .unwrap();
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires admitted read-only Go binary and official etcd"]
async fn mcs_044_txn_advance_go_differential() {
    let (mut go, cluster) = crate::group_merge_oracle_test::start_bootstrapped_go_with_environment(
        None,
        &[],
        &[("TZ".into(), "Asia/Shanghai".into())],
    )
    .await;
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
                        name: format!("txn44_{id}"),
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
    }
    let keys = Arc::new(KeyspaceManager::new(
        astersql_pd_storage_endpoint::StorageEndpoint::new(rs.clone()).keyspace_storage(rp.clone()),
    ));
    let m = GCStateManager::new(
        StorageEndpoint::new(rs.clone()),
        rp.clone(),
        keys,
        tokio::runtime::Handle::current(),
    )
    .with_diagnostic_time_formatter(Arc::new(|time| {
        chrono::DateTime::from_timestamp(time.unix_seconds(), 0)
            .unwrap()
            .with_timezone(&chrono_tz::Asia::Shanghai)
            .format("%Y-%m-%d %H:%M:%S %z %Z")
            .to_string()
    }));
    let role = m.on_node_becomes_leader().await;
    let mut cases = vec![
        Case::new("advance", 0, 10),
        Case::new("same", 10, 10),
        Case::new("decrease", 10, 9),
    ];
    let mut c = Case::new("local", 10, 30);
    c.locals = vec![("a", 20, 0)];
    cases.push(c);
    let mut c = Case::new("ties", 10, 30);
    c.locals = vec![("a", 20, 0), ("b", 20, 0)];
    c.globals = vec![("global", 20, 0)];
    c.min = Some("20");
    cases.push(c.clone());
    c.name = "min-start";
    c.min = Some("15");
    cases.push(c.clone());
    c.name = "global";
    c.globals = vec![("global", 12, 0)];
    cases.push(c.clone());
    c.name = "old-blocker";
    c.globals = vec![("global", 5, 0)];
    cases.push(c);
    let mut c = Case::new("expired-cleanup", 10, 40);
    c.locals = vec![("expired", 12, 1), ("a", 30, 0)];
    c.globals = vec![("expired-global", 13, 1)];
    cases.push(c);
    let mut c = Case::new("finite-local-time", 10, 30);
    c.locals = vec![("finite", 20, 1_900_000_000)];
    cases.push(c);
    let mut c = Case::new("worker-unicode", 10, 30);
    c.locals = vec![("gc_worker", 7, 1), ("服务", 20, 0)];
    cases.push(c);
    cases.push(Case::new("u64-maximum", u64::MAX - 1, u64::MAX));
    let mut c = Case::new("bad-min-start", 10, 30);
    c.locals = vec![("expired", 12, 1)];
    c.min = Some("bad");
    cases.push(c);
    let mut c = Case::new("bad-gc-precedes-decrease", 10, 9);
    c.bad_gc = true;
    cases.push(c);
    let mut c = Case::new("keyspace", 10, 30);
    c.id = 1001;
    c.locals = vec![("a", 20, 0)];
    cases.push(c);
    let mut c = Case::new("unified-rejected", 10, 30);
    c.id = 1002;
    cases.push(c);
    let mut c = Case::new("missing", 10, 30);
    c.id = 1009;
    cases.push(c);
    let mut c = Case::new("null-alias", 10, 30);
    c.id = 0x01000000;
    c.locals = vec![("a", 20, 0)];
    cases.push(c);
    let mut client = PdClient::connect(go.url.clone()).await.unwrap();
    let mut report = vec![];
    for c in &cases {
        seed(gs.clone(), &gp, c).await;
        seed(rs.clone(), &rp, c).await;
        let scope = if c.id > 0xffffff {
            NULL_KEYSPACE_ID
        } else {
            c.id
        };
        let before_go = records(gs.as_ref(), &gp, scope).await;
        let before_rust = records(rs.as_ref(), &rp, scope).await;
        let request = pdpb::AdvanceTxnSafePointRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: cluster,
                ..Default::default()
            }),
            keyspace_scope: if c.id == NULL_KEYSPACE_ID {
                None
            } else {
                Some(pdpb::KeyspaceScope {
                    keyspace: Some(keyspace_scope::Keyspace::KeyspaceId(c.id)),
                })
            },
            target: c.target,
        };
        let response = client
            .advance_txn_safe_point(request.clone())
            .await
            .unwrap()
            .into_inner();
        let rust = m
            .advance_txn_safe_point(c.id, c.target, BarrierTime::from_unix(1_800_000_000, 0))
            .await;
        match (&response.header.as_ref().unwrap().error, &rust) {
            (None, Ok(result)) => {
                assert_eq!(
                    (response.old_txn_safe_point, response.new_txn_safe_point),
                    (result.old_txn_safe_point, result.new_txn_safe_point),
                    "{}",
                    c.name
                );
                assert_eq!(
                    response.blocker_description, result.blocker_description,
                    "{}",
                    c.name
                );
            }
            (Some(error), Err(rust)) => {
                if c.bad_gc || c.min == Some("bad") {
                    assert!(
                        error.message.contains("ParseUint") || error.message.contains("parse uint")
                    );
                    assert!(matches!(rust, astersql_pd_gc::GCReadError::SafePoint(_)));
                } else {
                    assert_eq!(error.message, rust.to_string(), "{}", c.name);
                }
            }
            _ => panic!("{} response mismatch {response:?} / {rust:?}", c.name),
        }
        let after_go = records(gs.as_ref(), &gp, scope).await;
        let after_rust = records(rs.as_ref(), &rp, scope).await;
        assert_eq!(
            bytes(&after_go),
            bytes(&after_rust),
            "persistent bytes {}",
            c.name
        );
        if rust.is_err() {
            for (before, after) in [(&before_go, &after_go), (&before_rust, &after_rust)] {
                for i in 0..6 {
                    assert_eq!(
                        before[i]["entries"], after[i]["entries"],
                        "failed operation modified storage {}",
                        c.name
                    );
                }
            }
        }
        report.push(json!({"case":format!("{c:?}"),"request":format!("{request:?}"),"go":format!("{response:?}"),"rust":format!("{rust:?}"),"before_go":before_go,"before_rust":before_rust,"after_go":after_go,"after_rust":after_rust,"diagnostic_timezone":"Asia/Shanghai"}));
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
    if let Some(path) = std::env::var_os("MCS_044_ORACLE_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
}
