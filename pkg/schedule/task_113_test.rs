// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_cache::{TTLUint64, TtlDuration};
use astersql_pd_core::{BasicCluster, KeyRange, Peer, RegionInfo, RegionMeta};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_filter::RegionReplicationContext;
use astersql_pd_schedule_hbstream::routing::StreamRouter;
use astersql_pd_schedule_operator::{OpKind, Operator, RELATED_MERGE_REGION};
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_schedule_scatter::{ScatterBatchError, ScatterBatchMetrics, ScatterBatchRequest};
use astersql_pd_schedule_schedulers::{
    DiagnosticRecorder, FactoryContext, JobTime, SchedulerRegistry,
};
use astersql_pd_schedule_splitter::SplitRetry;
use astersql_pd_schedule_types::BALANCE_LEADER_SCHEDULER;
use astersql_pd_storage_domain::MemoryStore;
use astersql_pd_storage_domain::StorageEndpoint;
use std::{
    cell::RefCell,
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;
fn region(id: u64) -> Arc<RegionInfo> {
    let p = Peer {
        id: 1,
        store_id: 1,
        ..Default::default()
    };
    Arc::new(RegionInfo::new(
        RegionMeta {
            id,
            peers: vec![p.clone()],
            key_range: KeyRange {
                start_key: b"a".to_vec(),
                end_key: b"z".to_vec(),
            },
            ..Default::default()
        },
        Some(p),
        vec![],
        vec![],
    ))
}
fn setup(
    limit: u64,
) -> (
    Coordinator<ProviderSnapshot<()>>,
    Arc<TTLUint64<()>>,
    Arc<AtomicBool>,
) {
    setup_mode(limit, false)
}
fn setup_mode(
    limit: u64,
    rules: bool,
) -> (
    Coordinator<ProviderSnapshot<()>>,
    Arc<TTLUint64<()>>,
    Arc<AtomicBool>,
) {
    let mut cluster = BasicCluster::new();
    cluster.regions.set_region(region(1));
    let cluster = Arc::new(cluster);
    let (heartbeat, _router) = StreamRouter::new(Arc::new(astersql_pd_core::StoresInfo::new()));
    let pending = Arc::new(TTLUint64::new(
        tokio::runtime::Handle::current(),
        CancellationToken::new(),
        Duration::from_secs(60),
        TtlDuration::from_nanos(180_000_000_000),
    ));
    let mut config = ProviderSnapshot::<()>::default();
    config.schedule.affinity_schedule_limit = limit;
    config.schedule.scheduler_max_waiting_operator = 10;
    config.schedule.switch_witness_interval = TtlDuration::from_nanos(10);
    config.schedule.enable_diagnostic = true;
    config.replication.max_replicas = 3;
    config.replication.enable_placement_rules = rules;
    config.schedule.replica_schedule_limit = 10;
    let fail = Arc::new(AtomicBool::new(false));
    let f = fail.clone();
    let coordinator = Coordinator::new(CoordinatorInputs {
        cluster,
        config: Arc::new(config),
        heartbeat,
        pending: pending.clone(),
        factory: FactoryContext::new(
            Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
            ClusterPaths::new(113),
            JobTime(0),
        ),
        scheduler_configuration: serde_json::json!({}),
        scatter_metrics: ScatterBatchMetrics::new(&prometheus::Registry::new()).unwrap(),
        start: Duration::ZERO,
        now_unix_nanos: 0,
        split_merge_interval_nanos: 0,
        total_regions: Box::new(move || {
            if f.load(Ordering::SeqCst) {
                Err("unavailable".into())
            } else {
                Ok(0)
            }
        }),
        unix_seconds: Arc::new(|| 100),
    });
    (coordinator, pending, fail)
}
#[tokio::test]
async fn task_113_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_113.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let (c, pending, _) = setup(case["limit"].as_u64().unwrap());
        let checker = c.get_checker_controller().clone();
        assert!(Arc::ptr_eq(&checker, c.get_checker_controller()));
        let op = Arc::new(Operator::new(
            "affinity-merge",
            "",
            1,
            None,
            OpKind::AFFINITY,
            1,
            vec![],
        ));
        op.additional_info()
            .set(RELATED_MERGE_REGION, &case["related"].to_string());
        match case["status"].as_str().unwrap() {
            "success" => {
                assert!(op.start());
                op.check(&region(1));
                assert_eq!(op.status(), astersql_pd_schedule_operator::SUCCESS);
            }
            "cancel" => {
                op.cancel();
            }
            "replace" => {
                op.start();
                op.replace();
            }
            _ => unreachable!(),
        }
        assert!(
            c.get_operator_records()
                .bury_operator(op.clone(), Instant::now())
                .is_some()
        );
        assert!(
            c.get_operator_records()
                .bury_operator(op, Instant::now())
                .is_none()
        );
        let check = checker.lock().unwrap();
        for id in [1, 2] {
            assert_eq!(
                check.recently_affinity_merged(id, 100),
                case["cached"].as_bool().unwrap(),
                "{case}"
            );
        }
        assert!(!check.recently_affinity_merged(1, 161));
        drop(check);
        pending.shutdown().await;
    }
}
#[tokio::test]
async fn task_113_ttl_getters_and_admission_fault() {
    let (c, pending, fail) = setup(10);
    let checker = c.get_checker_controller().clone();
    assert!(!c.is_pending_region(1));
    assert!(std::ptr::eq(
        c.get_prepare_checker(),
        c.get_prepare_checker()
    ));
    {
        let check = checker.lock().unwrap();
        assert!(std::ptr::eq(check.rule_checker(), check.rule_checker()));
        assert!(std::ptr::eq(check.merge_checker(), check.merge_checker()));
    }
    c.record_op_step_with_ttl(1, 100);
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_113.json"
    ))
    .unwrap();
    for case in fixture["ttl_cases"].as_array().unwrap() {
        assert_eq!(
            checker
                .lock()
                .unwrap()
                .rule_checker()
                .is_recently_promoted(1, case["now"].as_i64().unwrap() as i128),
            case["recent"].as_bool().unwrap()
        );
    }
    assert!(
        !checker
            .lock()
            .unwrap()
            .rule_checker()
            .is_recently_promoted(2, 100)
    );
    fail.store(true, Ordering::SeqCst);
    assert!(!c.should_run(Duration::ZERO));
    assert!(!c.get_prepare_checker().is_prepared());
    fail.store(false, Ordering::SeqCst);
    assert!(c.should_run(Duration::ZERO));
    assert!(c.get_prepare_checker().is_prepared());
    fail.store(true, Ordering::SeqCst);
    assert!(c.should_run(Duration::ZERO));
    pending.shutdown().await;
}
#[tokio::test]
async fn task_113_scatter_splitter_shared_pending_and_admission() {
    let (mut c, pending, _) = setup(10);
    let checker = c.get_checker_controller().clone();
    let conf = ProviderSnapshot::<()>::default();
    let mut rules = RuleManager::new(
        c.get_schedulers_controller().context().storage.clone(),
        ClusterPaths::new(113),
    );
    let hot = BTreeSet::new();
    let canonical = c.get_operator_controller() as *const _;
    {
        let mut scatter = c.get_region_scatterer(&mut rules, &hot, None, 0, false);
        assert!(std::ptr::eq(scatter.batch.controller, canonical));
        assert_eq!(
            scatter
                .scatter(
                    ScatterBatchRequest::Ids(vec![]),
                    "",
                    0,
                    false,
                    &CancellationToken::new(),
                    || panic!("no allocation")
                )
                .await
                .unwrap_err(),
            ScatterBatchError::EmptyRegion
        );
        let result = scatter
            .scatter(
                ScatterBatchRequest::Ids(vec![1]),
                "",
                0,
                false,
                &CancellationToken::new(),
                || panic!("unreplicated"),
            )
            .await
            .unwrap();
        assert_eq!(result.suspect, [1].into());
    }
    assert_eq!(checker.lock().unwrap().pending_processed_regions(), vec![1]);
    checker.lock().unwrap().clear_pending_processed_regions();
    let stores = c.get_cluster().stores.get_stores();
    let rep = RegionReplicationContext {
        conf: &conf,
        stores: &stores,
        manager: RefCell::new(&mut rules),
        now: 0,
    };
    let split = c.get_region_splitter();
    assert!(std::ptr::eq(split.cluster, &**c.get_cluster()));
    assert!(std::ptr::eq(split.controller, c.get_operator_controller()));
    let mut retry = SplitRetry::new(vec![b"m".to_vec()], 0);
    let submissions = split.start_attempt(&mut retry, 0, &rep).unwrap();
    assert!(submissions.admitted.is_empty());
    assert_eq!(checker.lock().unwrap().pending_processed_regions(), vec![1]);
    // The same splitter uses its canonical admission owner for a replicated input.
    drop(rep);
    let mut conf = conf;
    conf.replication.max_replicas = 1;
    conf.replication.enable_placement_rules = false;
    let rep = RegionReplicationContext {
        conf: &conf,
        stores: &stores,
        manager: RefCell::new(&mut rules),
        now: 0,
    };
    let mut retry = SplitRetry::new(vec![b"m".to_vec()], 0);
    let submissions = split.start_attempt(&mut retry, 0, &rep).unwrap();
    assert_eq!(submissions.admitted.len(), 1);
    assert!(Arc::ptr_eq(
        &submissions.admitted[0].admitted[0],
        &c.get_operator_controller().get_operator(1).unwrap()
    ));
    pending.shutdown().await;
}
#[tokio::test]
async fn task_113_diagnostic_live_scheduler_state() {
    let (mut c, pending, _) = setup(10);
    let name = "balance-leader-scheduler";
    assert_eq!(
        c.get_diagnostic_manager()
            .get_diagnostic_result(name, 1)
            .unwrap()
            .status,
        "disabled"
    );
    let mut registry = SchedulerRegistry::default();
    registry.register_builtins().unwrap();
    let scheduler = registry
        .create_args(
            &BALANCE_LEADER_SCHEDULER,
            &[],
            c.get_schedulers_controller().context(),
        )
        .unwrap();
    c.get_schedulers_controller_mut()
        .add(scheduler, &[], false)
        .await
        .unwrap();
    assert!(
        c.get_diagnostic_manager()
            .get_diagnostic_result(name, 2)
            .is_err()
    );
    let mut recorder = DiagnosticRecorder::new(BALANCE_LEADER_SCHEDULER).unwrap();
    recorder.set_result_from_status("pending", 3);
    c.diagnostic_recorders_mut().insert(name.into(), recorder);
    assert_eq!(
        c.get_diagnostic_manager()
            .get_diagnostic_result(name, 4)
            .unwrap()
            .status,
        "pending"
    );
    c.diagnostic_recorders_mut()
        .get_mut(name)
        .unwrap()
        .set_result_from_status("scheduling", 5);
    assert_eq!(
        c.get_diagnostic_manager()
            .get_diagnostic_result(name, 6)
            .unwrap()
            .status,
        "scheduling"
    );
    pending.shutdown().await;
}

#[tokio::test]
async fn task_113_rule_pending_facade_reads_dispatcher_state() {
    use astersql_pd_schedule_affinity::manager::Manager as AffinityManager;
    use astersql_pd_schedule_checker::{
        AffinityContext, CheckerDispatchContext, ReplicaFaultContext, RuleMissingContext,
    };
    use astersql_pd_schedule_labeler::RegionLabeler;
    let (c, pending, _) = setup_mode(10, true);
    let storage = c.get_schedulers_controller().context().storage.clone();
    let paths = ClusterPaths::new(113);
    let labels = RegionLabeler::load(
        StorageEndpoint::new(Arc::new(MemoryStore::default())),
        paths.clone(),
        0,
    )
    .await
    .unwrap();
    let mut rules = RuleManager::new(storage.clone(), paths.clone());
    rules
        .initialize(3, &[], "", false, &**c.get_config(), &[])
        .await
        .unwrap();
    let hot = BTreeSet::new();
    let ctx = CheckerDispatchContext {
        affinity: AffinityContext {
            rules: RuleMissingContext {
                replica: ReplicaFaultContext {
                    config: &**c.get_config(),
                    stores: &c.get_cluster().stores,
                    rules: &[],
                    now_unix_nanos: 0,
                    supports_conf_change_v2: false,
                },
                witness_enabled: false,
            },
            labels: &labels,
            storage: &storage,
            paths: &paths,
            region_exists: true,
        },
        cluster: c.get_cluster(),
        operators: c.get_operator_controller(),
        hot_regions: &hot,
    };
    assert!(!c.is_pending_region(1));
    let checker = c.get_checker_controller().clone();
    let mut guard = checker.lock().unwrap();
    let result = guard
        .check_region(
            &ctx,
            &mut rules,
            &mut AffinityManager::default(),
            &region(1),
            &mut || panic!("no eligible store"),
        )
        .await;
    assert!(result.operators.is_empty());
    assert!(guard.is_pending_region(1));
    let rule_ptr = guard.rule_checker() as *const _;
    let merge_ptr = guard.merge_checker() as *const _;
    drop(guard);
    assert!(c.is_pending_region(1));
    assert!(!c.is_pending_region(99));
    assert!(std::ptr::eq(&*c.get_rule_checker(), rule_ptr));
    assert!(std::ptr::eq(&*c.get_merge_checker(), merge_ptr));
    c.record_op_step_with_ttl(1, 100);
    assert!(c.get_rule_checker().is_recently_promoted(1, 109));
    assert!(!c.get_rule_checker().is_recently_promoted(1, 110));
    pending.shutdown().await;
}
