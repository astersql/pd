// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_cache::{TTLUint64, TtlDuration};
use astersql_pd_core::{
    BasicCluster, KeyRange, Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta,
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule::{Coordinator, CoordinatorInputs};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_hbstream::routing::StreamRouter;
use astersql_pd_schedule_scatter::ScatterBatchMetrics;
use astersql_pd_schedule_schedulers::{FactoryContext, JobTime};
use astersql_pd_storage_domain::MemoryStore;
use astersql_pd_storage_domain::StorageEndpoint;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
fn setup(
    config: serde_json::Value,
    backend: Arc<dyn astersql_pd_storage_domain::MetaStore>,
) -> (Coordinator<ProviderSnapshot<()>>, Arc<TTLUint64<()>>) {
    let mut cluster = BasicCluster::new();
    let peers: Vec<_> = (1..=3)
        .map(|id| Peer {
            id,
            store_id: id,
            ..Default::default()
        })
        .collect();
    let region = Arc::new(RegionInfo::new(
        RegionMeta {
            id: 1,
            peers: peers.clone(),
            key_range: KeyRange {
                start_key: b"a".to_vec(),
                end_key: b"b".to_vec(),
            },
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![],
    ));
    let mut region = (*region).clone();
    region.stats_mut().approximate_size = 96;
    let region = Arc::new(region);
    let (origin, overlaps, changed) = cluster.regions.set_region(region.clone());
    cluster
        .regions
        .update_sub_tree(region, origin.as_deref(), &overlaps, changed);
    for id in 2..=4 {
        cluster.regions.set_region(Arc::new(RegionInfo::new(
            RegionMeta {
                id,
                key_range: KeyRange {
                    start_key: vec![id as u8],
                    end_key: vec![id as u8 + 1],
                },
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        )));
    }
    let cluster = Arc::new(cluster);
    for id in 1..=3 {
        cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        })));
    }
    let (heartbeat, _router) = StreamRouter::new(Arc::new(astersql_pd_core::StoresInfo::new()));
    let pending = Arc::new(TTLUint64::new(
        tokio::runtime::Handle::current(),
        CancellationToken::new(),
        Duration::from_secs(60),
        TtlDuration::from_nanos(180_000_000_000),
    ));
    let c = Coordinator::new(CoordinatorInputs {
        cluster,
        config: Arc::new({
            let mut config = ProviderSnapshot::<()>::default();
            config.schedule.scheduler_max_waiting_operator = 10;
            config.schedule.switch_witness_interval = TtlDuration::from_nanos(10);
            config
        }),
        heartbeat,
        pending: pending.clone(),
        factory: FactoryContext::new(
            Arc::new(StorageEndpoint::new(backend)),
            ClusterPaths::new(114),
            JobTime(0),
        ),
        scheduler_configuration: config,
        scatter_metrics: ScatterBatchMetrics::new(&prometheus::Registry::new()).unwrap(),
        start: Duration::ZERO,
        now_unix_nanos: 0,
        split_merge_interval_nanos: 0,
        total_regions: Box::new(|| Ok(0)),
        unix_seconds: Arc::new(|| 0),
    });
    (c, pending)
}
use astersql_pd_schedule_operator::*;
use std::time::Instant;
fn operator(id: u64, kind: OpKind) -> Arc<Operator> {
    Arc::new(Operator::new(
        "query",
        "",
        id,
        None,
        kind,
        1,
        vec![
            TransferLeader {
                from_store: 1,
                to_store: 2,
                ..Default::default()
            }
            .into(),
        ],
    ))
}
fn ids(ops: Vec<Arc<Operator>>) -> Vec<u64> {
    let mut ids: Vec<_> = ops.iter().map(|o| o.region_id()).collect();
    ids.sort_unstable();
    ids
}
#[tokio::test]
async fn task_118_behavior_cases() {
    let backend = Arc::new(MemoryStore::default());
    let (c, pending) = setup(serde_json::json!({}), backend.clone());
    let h = Handler::new(Some(&c));
    let ctl = h.get_operator_controller().unwrap();
    assert!(std::ptr::eq(ctl, c.get_operator_controller()));
    let a = operator(
        1,
        OpKind(OpKind::ADMIN.0 | OpKind::LEADER.0 | OpKind::REGION.0),
    );
    a.set_priority_level(astersql_pd_core_constant::PriorityLevel::LOW);
    ctl.add_operators(c.get_cluster(), &[a.clone()]).unwrap();
    let w = operator(1, OpKind::LEADER);
    // A queued urgent replacement remains distinct from the active operator.
    w.set_priority_level(astersql_pd_core_constant::PriorityLevel::URGENT);
    ctl.enqueue_waiting(c.get_cluster(), &[w.clone()]).unwrap();
    assert!(Arc::ptr_eq(&h.get_operator(1).unwrap(), &a));
    assert!(Arc::ptr_eq(&h.get_waiting_operators().unwrap()[0], &w));
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_118.json"
    ))
    .unwrap();
    for case in fixture["kinds"].as_array().unwrap() {
        let kinds: Vec<_> = case["input"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let expected: Vec<_> = case["ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap())
            .collect();
        assert_eq!(
            ids(h.get_operators_by_kinds(&kinds).unwrap()),
            expected,
            "{case}"
        );
    }
    let now = Instant::now();
    assert_eq!(h.get_operator_status(1, now).unwrap().status, STARTED);
    assert_eq!(
        h.get_operator(99).err(),
        Some(HandlerError::OperatorNotFound)
    );
    assert_eq!(
        h.get_operator_status(99, now).err(),
        Some(HandlerError::OperatorNotFound)
    );
    assert_eq!(
        h.remove_operator(99, now),
        Err(HandlerError::OperatorNotFound)
    );
    assert!(h.get_history(now, now).unwrap().is_empty());
    assert_eq!(
        h.get_records(now, now).err(),
        Some(HandlerError::OperatorNotFound)
    );
    h.remove_operator(1, now).unwrap();
    assert!(h.get_operators().unwrap().is_empty());
    assert_eq!(ctl.operator_count(OpKind::ADMIN), 0);
    assert_eq!(a.status(), CANCELED);
    assert!(a.additional_info().log().contains("admin stop"));
    assert_eq!(h.get_operator_status(1, now).unwrap().status, CANCELED);
    assert_eq!(
        h.remove_operator(1, now),
        Err(HandlerError::OperatorNotFound)
    );
    let records = h.get_records(now, now).unwrap();
    assert_eq!(records.len(), 1);
    assert!(Arc::ptr_eq(&records[0].operator, &a));
    assert_eq!(records[0].record().finish_time, now);
    let history = h.get_history(now, now).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(
        (history[0].from, history[0].to, history[0].finish_time),
        (1, 2, now)
    );
    assert_eq!(
        h.get_records(now + Duration::from_nanos(1), now).err(),
        Some(HandlerError::OperatorNotFound)
    );
    assert!(
        h.get_history(now + Duration::from_nanos(1), now)
            .unwrap()
            .is_empty()
    );
    // RemoveAll leaves waiting queue intact, including when no active operation exists.
    h.remove_operators(now).unwrap();
    assert_eq!(h.get_waiting_operators().unwrap().len(), 1);
    ctl.promote_waiting(c.get_cluster(), 0.0).unwrap().unwrap();
    // Active replacement overrides the archived canceled status.
    assert!(Arc::ptr_eq(
        &h.get_operator_status(1, now).unwrap().operator,
        &w
    ));
    h.remove_operators(now + Duration::from_secs(1)).unwrap();
    assert_eq!(w.status(), CANCELED);
    assert_eq!(
        h.get_records(now, now + Duration::from_secs(1))
            .unwrap()
            .len(),
        1
    );
    let expiry = now + Duration::from_secs(1) + OPERATOR_STATUS_REMAIN_TIME;
    assert!(h.get_operator_status(1, expiry).is_ok());
    assert_eq!(
        h.get_records(now, expiry).err(),
        Some(HandlerError::OperatorNotFound)
    );
    assert_eq!(
        h.get_operator_status(1, expiry + Duration::from_nanos(1))
            .err(),
        Some(HandlerError::OperatorNotFound)
    );
    // The facade performs no metadata writes.
    use astersql_pd_storage_domain::{Consistency, MetaStore, Read};
    assert!(
        backend
            .read(Read::prefix(b"", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    pending.shutdown().await;
}
#[test]
fn task_118_uninitialized_errors() {
    let h = Handler::<ProviderSnapshot<()>>::new(None);
    let n = Instant::now();
    let mut errors = vec![
        h.get_operator_controller().err(),
        h.get_operator(1).err(),
        h.get_operator_status(1, n).err(),
        h.get_operators().err(),
        h.get_waiting_operators().err(),
        h.get_admin_operators().err(),
        h.get_leader_operators().err(),
        h.get_region_operators().err(),
        h.get_records(n, n).err(),
        h.get_history(n, n).err(),
        h.remove_operator(1, n).err(),
        h.remove_operators(n).err(),
    ];
    for kind in ["admin", "leader", "region", "waiting"] {
        errors.push(h.get_operators_by_kinds(&[kind]).err());
    }
    assert_eq!(errors.len(), 16);
    for e in errors {
        assert_eq!(e, Some(HandlerError::NotBootstrapped));
    }
    for kinds in [vec![], vec!["unknown"], vec!["unknown", "unknown"]] {
        assert!(h.get_operators_by_kinds(&kinds).unwrap().is_empty());
    }
    assert_eq!(
        HandlerError::NotBootstrapped.code(),
        "PD:cluster:ErrNotBootstrapped"
    );
    assert_eq!(
        HandlerError::OperatorNotFound.code(),
        "PD:operator:ErrOperatorNotFound"
    );
}
#[tokio::test]
async fn task_118_merge_removal_and_kind_masks() {
    let (c, pending) = setup(serde_json::json!({}), Arc::new(MemoryStore::default()));
    let h = Handler::new(Some(&c));
    let ctl = h.get_operator_controller().unwrap();
    let n = Instant::now();
    for result in [
        h.get_operators(),
        h.get_waiting_operators(),
        h.get_admin_operators(),
        h.get_leader_operators(),
        h.get_region_operators(),
    ] {
        assert!(result.unwrap().is_empty());
    }
    h.remove_operators(n).unwrap();
    let a = operator(2, OpKind::REGION);
    let b = operator(3, OpKind::REGION);
    let l = operator(4, OpKind::LEADER);
    a.additional_info().set(RELATED_MERGE_REGION, "3");
    b.additional_info().set(RELATED_MERGE_REGION, "2");
    ctl.add_operators(c.get_cluster(), &[a.clone(), b.clone()])
        .unwrap();
    ctl.add_operators(c.get_cluster(), &[l.clone()]).unwrap();
    assert!(h.get_admin_operators().unwrap().is_empty());
    assert_eq!(ids(h.get_leader_operators().unwrap()), [4]);
    assert_eq!(ids(h.get_region_operators().unwrap()), [2, 3]);
    assert_eq!(ids(h.get_operators().unwrap()), [2, 3, 4]);
    h.remove_operator(2, n).unwrap();
    assert_eq!(ids(h.get_operators().unwrap()), [4]);
    assert_eq!(ctl.operator_count(OpKind::REGION), 0);
    assert_eq!(
        a.additional_info().get("cancel-reason").as_deref(),
        Some("admin stop")
    );
    assert_eq!(
        b.additional_info().get("cancel-reason").as_deref(),
        Some("related merge region")
    );
    assert_eq!(h.get_records(n, n).unwrap().len(), 2);
    // A terminal success is preserved when the management action detaches it.
    l.check(&RegionInfo::new(
        RegionMeta::default(),
        Some(Peer {
            store_id: 2,
            ..Default::default()
        }),
        vec![],
        vec![],
    ));
    assert_eq!(l.status(), SUCCESS);
    h.remove_operators(n).unwrap();
    assert_eq!(h.get_operator_status(4, n).unwrap().status, SUCCESS);
    assert_eq!(h.get_records(n, n).unwrap().len(), 3);
    assert_eq!(h.get_history(n, n).unwrap().len(), 3);
    assert!(h.get_operators().unwrap().is_empty());
    pending.shutdown().await;
}
#[tokio::test]
async fn task_118_removal_acknowledges_store_limit() {
    use astersql_pd_core::StoreLimiter;
    use astersql_pd_core_constant::PriorityLevel;
    use astersql_pd_storelimit::SlidingWindows;
    let (c, pending) = setup(serde_json::json!({}), Arc::new(MemoryStore::default()));
    c.get_cluster().stores.patch_store(3, |s| {
        s.set_store_limit(Some(StoreLimiter::V2(SlidingWindows::new())))
    });
    let ctl = c.get_operator_controller();
    let h = Handler::new(Some(&c));
    let n = Instant::now();
    for remove_all in [false, true] {
        let o = Arc::new(Operator::new(
            "snapshot",
            "",
            1,
            None,
            OpKind::REGION,
            96,
            vec![
                AddLearner {
                    to_store: 3,
                    peer_id: 4,
                    send_store: 3,
                    ..Default::default()
                }
                .into(),
            ],
        ));
        o.set_priority_level(PriorityLevel::LOW);
        ctl.add_limited_operators(c.get_cluster(), &**c.get_config(), &[o.clone()])
            .await
            .unwrap();
        let handle = c
            .get_cluster()
            .stores
            .get_store(3)
            .unwrap()
            .limiter()
            .unwrap()
            .clone();
        let used = || match &*handle.read().unwrap() {
            StoreLimiter::V2(w) => w.used()[0],
            _ => unreachable!(),
        };
        assert_eq!(used(), 96);
        assert_eq!(
            h.remove_operator(999, n),
            Err(HandlerError::OperatorNotFound)
        );
        assert_eq!(used(), 96);
        if remove_all {
            h.remove_operators(n).unwrap();
        } else {
            h.remove_operator(1, n).unwrap();
        }
        assert_eq!(used(), 0);
        assert_eq!(h.get_operator_status(1, n).unwrap().status, CANCELED);
        assert_eq!(ctl.operator_count(OpKind::REGION), 0);
    }
    pending.shutdown().await;
}
