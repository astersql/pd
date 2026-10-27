// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::BasicCluster;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::{ProviderSnapshot, SchedulerConfig};
use astersql_pd_schedule_schedulers::*;
use astersql_pd_schedule_types::*;
use astersql_pd_storage_domain::MemoryStore;
use astersql_pd_storage_domain::StorageEndpoint;
use std::{collections::BTreeMap, sync::Arc};

async fn controller() -> SchedulerController {
    let ctx = FactoryContext::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        ClusterPaths::new(111),
        JobTime(0),
    );
    let mut controller = SchedulerController::new(
        Some(Arc::new(BasicCluster::new())),
        ctx,
        serde_json::json!({}),
    );
    let mut registry = SchedulerRegistry::default();
    registry.register_builtins().unwrap();
    for typ in [BALANCE_LEADER_SCHEDULER, SHUFFLE_LEADER_SCHEDULER] {
        let scheduler = registry
            .create_args(&typ, &[], controller.context())
            .unwrap();
        controller.add(scheduler, &[], false).await.unwrap();
    }
    controller
}

#[tokio::test]
async fn task_111_behavior_cases() {
    let controller = controller().await;
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_111.json"
    ))
    .unwrap();
    for row in fixture["cases"].as_array().unwrap() {
        let mut config = ProviderSnapshot::<()>::default();
        config.schedule.enable_diagnostic = row["allowed"].as_bool().unwrap();
        config.schedule.schedulers = Some(vec![SchedulerConfig {
            r#type: "balance-leader".into(),
            disable: row["disabled"].as_bool().unwrap(),
            ..Default::default()
        }]);
        let mut recorders = BTreeMap::new();
        if row["recorder"].as_bool().unwrap() {
            let mut recorder = DiagnosticRecorder::new(BALANCE_LEADER_SCHEDULER).unwrap();
            if let Some(status) = row["recorded"].as_str() {
                recorder.set_result_from_status("halted", 1);
                recorder.set_result_from_status(status, 2);
            }
            recorders.insert("balance-leader-scheduler".into(), recorder);
        }
        let manager = Manager::new(&controller, &config, &recorders);
        let name = row["name"].as_str().unwrap();
        let result = manager.get_diagnostic_result(name, row["timestamp"].as_u64().unwrap());
        if let Some(code) = row["error"].as_str() {
            let error = result.unwrap_err();
            assert_eq!(error.code(), code, "{row}");
            assert_eq!(
                error.to_string(),
                format!("[{code}]{}", row["message"].as_str().unwrap())
            );
        } else {
            let result = result.unwrap();
            assert_eq!(result.status, row["status"].as_str().unwrap(), "{row}");
            assert_eq!(result.timestamp, row["timestamp"].as_u64().unwrap());
            assert_eq!(result.name, name);
            assert_eq!(result.summary, row["summary"].as_str().unwrap());
            assert!(result.store_status.is_empty());
        }
    }
}

#[tokio::test]
async fn task_111_live_state_and_ownership() {
    let mut controller = controller().await;
    let mut config = ProviderSnapshot::<()>::default();
    config.schedule.enable_diagnostic = true;
    let name = "balance-leader-scheduler";
    let mut recorders = BTreeMap::new();
    let mut recorder = DiagnosticRecorder::new(BALANCE_LEADER_SCHEDULER).unwrap();
    recorder.set_result_from_status(PENDING, 1);
    recorders.insert(name.into(), recorder);
    let mut first = Manager::new(&controller, &config, &recorders)
        .get_diagnostic_result(name, 10)
        .unwrap();
    first.status.clear();
    first.summary.clear();
    assert_eq!(
        Manager::new(&controller, &config, &recorders)
            .get_diagnostic_result(name, 11)
            .unwrap()
            .summary,
        "balance-leader-scheduler reach limit"
    );
    recorders
        .get_mut(name)
        .unwrap()
        .set_result_from_status(PAUSED, 4);
    assert_eq!(
        Manager::new(&controller, &config, &recorders)
            .get_diagnostic_result(name, 12)
            .unwrap()
            .status,
        PAUSED
    );
    // Removing a real scheduler must override even a still-retained recorder.
    controller.remove(name, false).await.unwrap();
    assert_eq!(
        Manager::new(&controller, &config, &recorders)
            .get_diagnostic_result(name, 13)
            .unwrap()
            .status,
        DISABLED
    );
    config.schedule.enable_diagnostic = false;
    assert_eq!(
        Manager::new(&controller, &config, &recorders)
            .get_diagnostic_result(name, 14)
            .unwrap_err()
            .code(),
        "PD:diagnostic:ErrDiagnosticDisabled"
    );
    assert_eq!(recorders[name].len(), 2);
}

#[tokio::test]
async fn task_111_failed_recording_and_recovery() {
    use astersql_pd_core::{StoreInfo, StoreMeta};
    use astersql_pd_schedule_plan::{BalanceSchedulerPlan, Status, StatusCode};
    let controller = controller().await;
    let mut config = ProviderSnapshot::<()>::default();
    config.schedule.enable_diagnostic = true;
    let name = "balance-leader-scheduler";
    let mut recorder = DiagnosticRecorder::new(BALANCE_LEADER_SCHEDULER).unwrap();
    let mut plan = BalanceSchedulerPlan {
        status: Status::new(StatusCode::STORE_LOW_SPACE, &[]),
        ..Default::default()
    };
    // Actual canonical analysis panics on absent source; failed analysis is atomic.
    let fault = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        recorder.set_result_from_plans(&[], &[&plan], 1)
    }));
    assert!(fault.is_err());
    let mut recorders = BTreeMap::from([(name.into(), recorder)]);
    assert_eq!(
        Manager::new(&controller, &config, &recorders)
            .get_diagnostic_result(name, 2)
            .unwrap_err()
            .code(),
        "PD:diagnostic:ErrNoDiagnosticResult"
    );
    plan.source = Some(Arc::new(StoreInfo::new(StoreMeta {
        id: 1,
        ..Default::default()
    })));
    recorders
        .get_mut(name)
        .unwrap()
        .set_result_from_plans(&[], &[&plan], 3);
    let result = Manager::new(&controller, &config, &recorders)
        .get_diagnostic_result(name, 4)
        .unwrap();
    assert_eq!(result.status, PENDING);
    assert_eq!(result.summary, "1 store(s) StoreLowSpace; ");
    assert_eq!(result.timestamp, 4);
    assert_eq!(recorders[name].len(), 1);
}
