// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_schedule_schedulers::{DiagnosticRecorder, SchedulerRegistry};
use astersql_pd_storage_domain::MemoryStore;
use serde_json::{Value, json};
use std::sync::Arc;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_122.json"
    ))
    .unwrap()
}
#[tokio::test]
async fn task_122_behavior_cases() {
    let f = fixture();
    let (mut c, pending) = setup_admin(false, Arc::new(MemoryStore::default()));
    let mut registry = SchedulerRegistry::default();
    registry.register_builtins().unwrap();
    for name in f["names"].as_array().unwrap() {
        let typ = registry.find_type_by_name(name.as_str().unwrap()).unwrap();
        let s = registry
            .create_args(&typ, &[], c.get_schedulers_controller().context())
            .unwrap();
        c.get_schedulers_controller_mut()
            .add(s, &[], false)
            .await
            .unwrap();
    }
    c.get_schedulers_controller_mut()
        .replace_scheduler_configs(vec![json!({"type":"balance-region","disable":true})]);
    pause_or_resume_scheduler(Some(&mut c), "balance-leader-scheduler", 10, 100).unwrap();
    let h = Handler::new(Some(&c));
    assert!(std::ptr::eq(
        h.get_schedulers_controller().unwrap(),
        c.get_schedulers_controller()
    ));
    assert_eq!(json!(h.get_scheduler_names().unwrap()), f["names"]);
    for case in f["filters"].as_array().unwrap() {
        assert_eq!(
            h.get_scheduler_by_status(
                case["status"].as_str().unwrap(),
                case["timestamps"].as_bool().unwrap(),
                101
            )
            .unwrap(),
            SchedulerQuery::Names(serde_json::from_value(case["names"].clone()).unwrap()),
            "{case}"
        );
    }
    assert_eq!(
        h.get_scheduler_by_status("paused", true, 100).unwrap(),
        SchedulerQuery::PausedPeriods(vec![SchedulerPausedPeriod {
            name: "balance-leader-scheduler".into(),
            paused_at: 100,
            resume_at: 110
        }])
    );
    assert_eq!(
        h.get_scheduler_by_status("paused", true, 110).unwrap(),
        SchedulerQuery::PausedPeriods(vec![])
    );
    pause_or_resume_scheduler(Some(&mut c), "all", 20, 200).unwrap();
    assert_eq!(
        Handler::new(Some(&c))
            .get_scheduler_by_status("paused", false, 201)
            .unwrap(),
        SchedulerQuery::Names(serde_json::from_value(f["names"].clone()).unwrap())
    );
    // Unknown mutations preserve both deadlines.
    assert!(matches!(
        pause_or_resume_scheduler(Some(&mut c), "missing", 10, 202),
        Err(SchedulerAdminError::Control(_))
    ));
    assert_eq!(
        c.get_schedulers_controller()
            .delay("balance-leader-scheduler", 202)
            .unwrap(),
        (200, 220)
    );
    for ttl in [0, -1] {
        pause_or_resume_scheduler(Some(&mut c), "all", ttl, 202).unwrap();
        assert_eq!(
            Handler::new(Some(&c))
                .get_scheduler_by_status("paused", false, 202)
                .unwrap(),
            SchedulerQuery::Names(vec![])
        );
    }
    pending.shutdown().await;
}
#[tokio::test]
async fn task_122_checker_states_and_errors() {
    let (c, pending) = setup_admin(false, Arc::new(MemoryStore::default()));
    let h = Handler::new(Some(&c));
    for name in fixture()["checker_names"].as_array().unwrap() {
        let name = name.as_str().unwrap();
        assert!(!h.get_checker_status(name, 100).unwrap().paused);
        h.pause_or_resume_checker(name, 10, 100).unwrap();
        assert!(h.get_checker_status(name, 109).unwrap().paused);
        assert!(!h.get_checker_status(name, 110).unwrap().paused);
        h.pause_or_resume_checker(name, 30, 100).unwrap();
        h.pause_or_resume_checker(name, 0, 101).unwrap();
        assert!(!h.get_checker_status(name, 101).unwrap().paused);
        h.pause_or_resume_checker(name, -1, 101).unwrap();
        assert!(!h.get_checker_status(name, 101).unwrap().paused);
        assert!(
            !c.get_checker_controller()
                .lock()
                .unwrap()
                .pause_controller(name)
                .unwrap()
                .is_paused_at(101)
        );
    }
    h.pause_or_resume_checker("learner", 30, 100).unwrap();
    for name in ["all", "missing", ""] {
        assert_eq!(
            h.get_checker_status(name, 101).unwrap_err().code(),
            "PD:checker:ErrCheckerNotFound"
        );
        assert_eq!(
            h.pause_or_resume_checker(name, 0, 101).unwrap_err().code(),
            "PD:checker:ErrCheckerNotFound"
        );
    }
    assert!(h.get_checker_status("learner", 101).unwrap().paused);
    h.pause_or_resume_checker("learner", i64::MAX, 100).unwrap();
    assert!(!h.get_checker_status("learner", 101).unwrap().paused);
    pending.shutdown().await;
}
#[test]
fn task_122_uninitialized_and_diagnostic_precedence() {
    let h = Handler::<ProviderSnapshot<()>>::new(None);
    for e in [
        h.get_scheduler_names().err(),
        h.get_schedulers_controller().err(),
        h.get_scheduler_by_status("paused", true, 100).err(),
        h.get_checker_status("missing", 100).err(),
        h.pause_or_resume_checker("learner", 10, 100).err(),
        pause_or_resume_scheduler::<ProviderSnapshot<()>>(None, "all", 10, 100).err(),
        h.get_diagnostic_result("balance-leader-scheduler", 100)
            .err(),
    ] {
        assert_eq!(e.unwrap().code(), "PD:cluster:ErrNotBootstrapped");
    }
    for name in ["missing", "shuffle-leader-scheduler", "balance-leader"] {
        assert_eq!(
            h.get_diagnostic_result(name, 100).unwrap_err().code(),
            "PD:diagnostic:ErrSchedulerUndiagnosable"
        );
    }
}
#[tokio::test]
async fn task_122_diagnostic_real_recorder() {
    let (c, pending) = setup_admin(false, Arc::new(MemoryStore::default()));
    assert_eq!(
        Handler::new(Some(&c))
            .get_diagnostic_result("balance-leader-scheduler", 100)
            .unwrap_err()
            .code(),
        "PD:diagnostic:ErrDiagnosticDisabled"
    );
    // Diagnostic configuration is a canonical Coordinator input, configured in this harness.
    // Build a separate real coordinator with diagnostics enabled.
    pending.shutdown().await;
    let (mut enabled, pending) = setup_diagnostic().await;
    let name = "balance-leader-scheduler";
    assert_eq!(
        Handler::new(Some(&enabled))
            .get_diagnostic_result(name, 100)
            .unwrap()
            .status,
        "disabled"
    );
    let mut r = SchedulerRegistry::default();
    r.register_builtins().unwrap();
    let typ = r.find_type_by_name(name).unwrap();
    let s = r
        .create_args(&typ, &[], enabled.get_schedulers_controller().context())
        .unwrap();
    enabled
        .get_schedulers_controller_mut()
        .add(s, &[], false)
        .await
        .unwrap();
    assert_eq!(
        Handler::new(Some(&enabled))
            .get_diagnostic_result(name, 101)
            .unwrap_err()
            .code(),
        "PD:diagnostic:ErrSchedulerUndiagnosable"
    );
    enabled
        .diagnostic_recorders_mut()
        .insert(name.into(), DiagnosticRecorder::new(typ).unwrap());
    assert_eq!(
        Handler::new(Some(&enabled))
            .get_diagnostic_result(name, 101)
            .unwrap_err()
            .code(),
        "PD:diagnostic:ErrNoDiagnosticResult"
    );
    for (status, ts) in [("pending", 102), ("paused", 103), ("scheduling", 104)] {
        enabled
            .diagnostic_recorders_mut()
            .get_mut(name)
            .unwrap()
            .set_result_from_status(status, ts);
        let out = Handler::new(Some(&enabled))
            .get_diagnostic_result(name, 200)
            .unwrap();
        assert_eq!((out.status.as_str(), out.timestamp), (status, 200));
    }
    // Query does not fabricate a paused result before the scheduling path records it.
    pause_or_resume_scheduler(Some(&mut enabled), name, 10, 200).unwrap();
    assert_eq!(
        Handler::new(Some(&enabled))
            .get_diagnostic_result(name, 201)
            .unwrap()
            .status,
        "scheduling"
    );
    pending.shutdown().await;
}

use astersql_pd_cache::{TTLUint64, TtlDuration};
use astersql_pd_core::BasicCluster;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule::{Coordinator, CoordinatorInputs};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_hbstream::routing::StreamRouter;
use astersql_pd_schedule_scatter::ScatterBatchMetrics;
use astersql_pd_schedule_schedulers::{FactoryContext, JobTime};
use astersql_pd_storage_domain::StorageEndpoint;

use std::time::Duration;
use tokio_util::sync::CancellationToken;

fn setup_admin(
    diagnostic: bool,
    backend: Arc<dyn astersql_pd_storage_domain::MetaStore>,
) -> (Coordinator<ProviderSnapshot<()>>, Arc<TTLUint64<()>>) {
    let cluster = Arc::new(BasicCluster::new());
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
            config.schedule.enable_diagnostic = diagnostic;
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
        scheduler_configuration: json!({}),
        scatter_metrics: ScatterBatchMetrics::new(&prometheus::Registry::new()).unwrap(),
        start: Duration::ZERO,
        now_unix_nanos: 0,
        split_merge_interval_nanos: 0,
        total_regions: Box::new(|| Ok(0)),
        unix_seconds: Arc::new(|| 0),
    });
    (c, pending)
}
async fn setup_diagnostic() -> (Coordinator<ProviderSnapshot<()>>, Arc<TTLUint64<()>>) {
    setup_admin(true, Arc::new(MemoryStore::default()))
}

#[tokio::test]
async fn task_122_boundaries_and_metadata_preservation() {
    use astersql_pd_storage_domain::{Consistency, MetaStore, Read};
    let backend = Arc::new(MemoryStore::default());
    let (mut c, pending) = setup_admin(true, backend.clone());
    for (status, ts) in [("paused", false), ("disabled", true), ("unknown", true)] {
        assert_eq!(
            Handler::new(Some(&c))
                .get_scheduler_by_status(status, ts, 100)
                .unwrap(),
            SchedulerQuery::Names(vec![])
        );
    }
    assert_eq!(
        Handler::new(Some(&c))
            .get_scheduler_by_status("paused", true, 100)
            .unwrap(),
        SchedulerQuery::PausedPeriods(vec![])
    );
    pause_or_resume_scheduler(Some(&mut c), "all", 10, 100).unwrap();
    let mut r = SchedulerRegistry::default();
    r.register_builtins().unwrap();
    for name in ["balance-leader-scheduler", "balance-region-scheduler"] {
        let typ = r.find_type_by_name(name).unwrap();
        let s = r
            .create_args(&typ, &[], c.get_schedulers_controller().context())
            .unwrap();
        c.get_schedulers_controller_mut()
            .add(s, &[], false)
            .await
            .unwrap();
    }
    let before = backend
        .read(Read::prefix(b"", Consistency::Linearizable))
        .await
        .unwrap();
    pause_or_resume_scheduler(Some(&mut c), "balance-leader-scheduler", 10, 100).unwrap();
    pause_or_resume_scheduler(Some(&mut c), "balance-leader-scheduler", 30, 101).unwrap();
    assert_eq!(
        c.get_schedulers_controller()
            .delay("balance-leader-scheduler", 102)
            .unwrap(),
        (101, 131)
    );
    assert!(
        !c.get_schedulers_controller()
            .is_paused("balance-region-scheduler", 102)
            .unwrap()
    );
    for ttl in [0, -10] {
        pause_or_resume_scheduler(Some(&mut c), "balance-leader-scheduler", ttl, 102).unwrap();
        assert_eq!(
            c.get_schedulers_controller()
                .delay("balance-leader-scheduler", 102)
                .unwrap(),
            (0, 0)
        );
    }
    pause_or_resume_scheduler(Some(&mut c), "balance-leader-scheduler", i64::MAX, 100).unwrap();
    assert!(
        !c.get_schedulers_controller()
            .is_paused("balance-leader-scheduler", 100)
            .unwrap()
    );
    Handler::new(Some(&c))
        .pause_or_resume_checker("replica", 10, 100)
        .unwrap();
    assert!(
        !Handler::new(Some(&c))
            .get_checker_status("learner", 101)
            .unwrap()
            .paused
    );
    assert_eq!(
        Handler::new(Some(&c))
            .get_diagnostic_result("balance-region-scheduler", 101)
            .unwrap_err()
            .code(),
        "PD:diagnostic:ErrSchedulerUndiagnosable"
    );
    let after = backend
        .read(Read::prefix(b"", Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(
        (before.revision, before.entries),
        (after.revision, after.entries)
    );
    pending.shutdown().await;
}
