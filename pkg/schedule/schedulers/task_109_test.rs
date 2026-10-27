// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{StoreInfo, StoreMeta};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_plan::{BalanceSchedulerPlan, Status, StatusCode};
use astersql_pd_schedule_types::*;
use std::{any::Any, sync::Arc};

fn plan(id: u64, code: i64) -> BalanceSchedulerPlan {
    BalanceSchedulerPlan {
        source: Some(Arc::new(StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        }))),
        status: Status::new(StatusCode(code), &[]),
        ..Default::default()
    }
}
fn record(d: &mut DiagnosticRecorder, plans: &[BalanceSchedulerPlan], ts: u64) {
    let refs: Vec<&dyn Any> = plans.iter().map(|p| p as &dyn Any).collect();
    d.set_result_from_plans(&[], &refs, ts);
}
#[test]
fn task_109_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_109.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        for tp in [BALANCE_LEADER_SCHEDULER, BALANCE_REGION_SCHEDULER] {
            let mut d = DiagnosticRecorder::new(tp.clone()).unwrap();
            assert!(d.get_last_result(10).is_none());
            let plans: Vec<_> = case["codes"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(i, c)| plan(i as u64 + 1, c.as_i64().unwrap()))
                .collect();
            record(&mut d, &plans, 5);
            let out = d.get_last_result(10).unwrap();
            assert_eq!(
                out.status,
                case["status"].as_str().unwrap(),
                "{}",
                case["name"]
            );
            assert_eq!(out.summary, case["summary"].as_str().unwrap());
            assert_eq!(out.name, tp.as_str());
            assert_eq!(out.timestamp, 10);
            assert!(out.store_status.is_empty());
            let json = serde_json::to_value(&out).unwrap();
            assert!(json.get("store_status").is_none());
            assert_eq!(json.as_object().unwrap().len(), 4);
        }
    }
}
#[test]
fn task_109_allow_status_and_faults() {
    for tp in SCHEDULERS {
        let d = DiagnosticRecorder::new(tp.clone());
        assert_eq!(
            d.is_some(),
            tp == BALANCE_LEADER_SCHEDULER || tp == BALANCE_REGION_SCHEDULER
        );
    }
    assert!(DiagnosticRecorder::new(CheckerSchedulerType::from("unknown")).is_none());
    let mut config = ProviderSnapshot::<()>::default();
    let mut d = DiagnosticRecorder::new(BALANCE_LEADER_SCHEDULER).unwrap();
    config.schedule.enable_diagnostic = false;
    assert!(!d.is_allowed(&config));
    config.schedule.enable_diagnostic = true;
    assert!(d.is_allowed(&config));
    for status in [
        DISABLED, PAUSED, HALTED, SCHEDULING, PENDING, NORMAL, "custom",
    ] {
        d.set_result_from_status(status, 1);
        let out = d.get_last_result(2).unwrap();
        assert_eq!(out.status, status);
        assert_eq!(
            out.summary,
            if status == PENDING {
                "balance-leader-scheduler reach limit"
            } else {
                ""
            }
        );
    }
    // Go discards summary's wrong-plan error and stores pending without store statuses.
    d.set_result_from_plans(&[], &[&42u64], 3);
    assert_eq!(
        d.get_last_result(4).unwrap().summary,
        "balance-leader-scheduler reach limit"
    );
    // Failed summary cannot leak a partially accumulated store map.
    d.set_result_from_plans(&[], &[&plan(1, 403), &42u64], 3);
    assert_eq!(
        d.get_last_result(4).unwrap().summary,
        "balance-leader-scheduler reach limit"
    );
}
#[test]
fn task_109_history_weights_capacity_and_ownership() {
    let mut d = DiagnosticRecorder::new(BALANCE_LEADER_SCHEDULER).unwrap();
    // Newest weight 3 outweighs the two older weights 2+1 only by deterministic tie.
    record(&mut d, &[plan(1, 403)], 1);
    record(&mut d, &[plan(1, 403)], 1);
    record(&mut d, &[plan(1, 400)], 1);
    assert_eq!(
        d.get_last_result(9).unwrap().summary,
        "1 store(s) StoreBusy; "
    );
    for _ in 0..7 {
        record(&mut d, &[plan(1, 403)], 1);
    }
    d.set_result_from_status(PAUSED, 2);
    record(&mut d, &[plan(1, 500)], 2);
    assert_eq!(
        d.get_last_result(9).unwrap().summary,
        "1 store(s) StoreLowSpace; "
    );
    // Ten empty pending entries evict older store evidence even with repeated timestamps.
    for _ in 0..10 {
        d.set_result_from_status(PENDING, 2);
    }
    assert_eq!(d.len(), 10);
    assert_eq!(
        d.get_last_result(9).unwrap().summary,
        "balance-leader-scheduler reach limit"
    );
    let mut p = plan(1, 403);
    record(&mut d, &[p.clone()], 3);
    p.status = Status::new(StatusCode::STORE_LOW_SPACE, &[]);
    let mut out = d.get_last_result(9).unwrap();
    assert_eq!(out.summary, "1 store(s) StoreDown; ");
    out.summary.clear();
    assert_eq!(
        d.get_last_result(9).unwrap().summary,
        "1 store(s) StoreDown; "
    );
}

#[tokio::test]
async fn task_109_real_dry_run() {
    use astersql_pd_core::{BasicCluster, KeyRange, Peer, RegionInfo, RegionMeta};
    use astersql_pd_keypath::ClusterPaths;
    use astersql_pd_schedule_filter::AffinityFilter;
    use astersql_pd_schedule_operator::{OpInfluence, OpStep};
    use astersql_pd_schedule_placement::RuleManager;
    use astersql_pd_statistics::StatisticsSnapshot;
    use astersql_pd_storage_domain::MemoryStore;
    use astersql_pd_storage_domain::StorageEndpoint;
    let mut cluster = BasicCluster::new();
    for id in 1..=3 {
        let mut s = StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        });
        s.set_leader_count(if id == 1 { 30 } else { 0 });
        s.set_last_heartbeat_unix_nanos(1_000_000_000_000);
        cluster.stores.put_store(Arc::new(s));
    }
    let peers: Vec<_> = (1..=3)
        .map(|id| Peer {
            id,
            store_id: id,
            ..Default::default()
        })
        .collect();
    let mut region = RegionInfo::new(
        RegionMeta {
            id: 1,
            key_range: KeyRange {
                start_key: b"a".to_vec(),
                end_key: b"b".to_vec(),
            },
            peers: peers.clone(),
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![],
    );
    region.stats_mut().approximate_size = 10;
    let region = Arc::new(region);
    let (o, v, b) = cluster.regions.set_region(region.clone());
    cluster
        .regions
        .update_sub_tree(region.clone(), o.as_deref(), &v, b);
    let mut config = ProviderSnapshot::<()>::default();
    config.schedule.tolerant_size_ratio = 1.0;
    config.schedule.leader_schedule_policy = "count".into();
    config.replication.enable_placement_rules = false;
    let storage = Arc::new(MemoryStore::default());
    let endpoint = StorageEndpoint::new(storage.clone());
    let mut rules = RuleManager::new(
        Arc::new(StorageEndpoint::new(storage)),
        ClusterPaths::new(109),
    );
    rules
        .initialize(3, &[], "", false, &config, &[])
        .await
        .unwrap();
    let labels = astersql_pd_schedule_labeler::RegionLabeler::load(
        endpoint,
        ClusterPaths::new(109),
        Default::default(),
    )
    .await
    .unwrap();
    let statistics = StatisticsSnapshot::default();
    let key_ranges = astersql_pd_schedule_keyrange::Manager::new();
    let influence = OpInfluence::new();
    let before = influence.snapshot();
    let mut index = |_: usize| 0;
    let mut shuffle = |_: &mut [usize]| {};
    let mut scheduler = BalanceLeaderScheduler::default();
    let mut ctx = LeaderBalanceContext {
        cluster: &cluster,
        config: &config,
        rules: &mut rules,
        affinity: AffinityFilter {
            manager: None,
            labels: &labels,
            now: 1000,
        },
        statistics: &statistics,
        hot_degree: 1,
        key_ranges: &key_ranges,
        influence: &influence,
        now_unix_nanos: 1_000_000_000_000,
        random_index: &mut index,
        shuffle: &mut shuffle,
    };
    let output = scheduler.schedule(&mut ctx, true).await;
    assert!(!output.operators.is_empty());
    assert!(matches!(
        output.operators[0].step(0),
        Some(OpStep::TransferLeader(step)) if step.from_store == 1
    ));
    let mut d = DiagnosticRecorder::new(BALANCE_LEADER_SCHEDULER).unwrap();
    // Operators win even over an invalid plan; no summary is evaluated.
    let analyzed = d.analyze(&output.operators, &[&42u64], 10);
    assert_eq!(analyzed.status, SCHEDULING);
    assert!(d.is_empty());
    let refs: Vec<&dyn Any> = output.plans.iter().map(|p| p as &dyn Any).collect();
    d.set_result_from_plans(&output.operators, &refs, 10);
    assert_eq!(d.get_last_result(11).unwrap().status, SCHEDULING);
    // Repeated summaries inspect real plans without submitting operations or mutating input state.
    let a = d.analyze(&[], &refs, 12);
    let b = d.analyze(&[], &refs, 12);
    assert_eq!(a, b);
    assert_eq!(influence.snapshot(), before);
    assert_eq!(
        cluster.regions.get_region(1).unwrap().leader(),
        region.leader()
    );
    assert_eq!(cluster.stores.get_store(1).unwrap().leader_count(), 30);
    assert_eq!(d.len(), 1);
    // Rejected stores generate actual failed plans; successful Go balance-leader
    // scheduling may return operators with an empty diagnostic plan list.
    for id in 1..=3 {
        let mut store = (*cluster.stores.get_store(id).unwrap()).clone();
        store.set_store_state(astersql_pd_core::StoreState::Tombstone, Some(false));
        cluster.stores.put_store(Arc::new(store));
    }
    let mut ctx = LeaderBalanceContext {
        cluster: &cluster,
        config: &config,
        rules: &mut rules,
        affinity: AffinityFilter {
            manager: None,
            labels: &labels,
            now: 1000,
        },
        statistics: &statistics,
        hot_degree: 1,
        key_ranges: &key_ranges,
        influence: &influence,
        now_unix_nanos: 1_000_000_000_000,
        random_index: &mut index,
        shuffle: &mut shuffle,
    };
    let failed = scheduler.schedule(&mut ctx, true).await;
    assert!(failed.operators.is_empty());
    assert!(!failed.plans.is_empty());
    let refs: Vec<&dyn Any> = failed.plans.iter().map(|p| p as &dyn Any).collect();
    d.set_result_from_plans(&failed.operators, &refs, 20);
    let out = d.get_last_result(21).unwrap();
    assert_eq!(out.status, PENDING);
    assert_eq!(out.summary, "3 store(s) StoreRemoved; ");
    assert_eq!(influence.snapshot(), before);
    assert_eq!(
        cluster.regions.get_region(1).unwrap().leader(),
        region.leader()
    );
}

#[test]
fn task_109_deepest_step_and_fault_atomicity() {
    let mut d = DiagnosticRecorder::new(BALANCE_REGION_SCHEDULER).unwrap();
    let mut p = plan(1, 403);
    let mut deeper = plan(1, 100);
    deeper.step = 2;
    deeper.target = p.source.clone();
    record(&mut d, &[deeper.clone(), p.clone()], 1);
    assert_eq!(d.get_last_result(2).unwrap().status, NORMAL);
    // Missing source preserves canonical plan's programmer-error contract, without recording a result.
    p.source = None;
    let n = d.len();
    let failure =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| record(&mut d, &[p], 3)));
    assert!(failure.is_err());
    assert_eq!(d.len(), n);
    assert_eq!(d.get_last_result(4).unwrap().status, NORMAL);
}
