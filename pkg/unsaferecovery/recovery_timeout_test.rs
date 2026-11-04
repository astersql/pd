// Copyright 2026 AsterSQL.
use super::*;
use std::sync::Arc;
#[test]
fn mcs_024_behavior() {
    let mut c = Controller::new(Arc::new(RecoverySteps::new(24)));
    c.stage = Stage::ForceLeader;
    c.expected_reports.insert(1, None);
    c.abort(1).unwrap();
    assert_eq!(
        c.stage(),
        Stage::ExitForceLeader,
        "abort must dispatch an actual best-effort exit phase before failing"
    );
    assert_eq!(c.dispatch_plan(1, 2).unwrap().force_leader, None);
    assert_eq!(
        c.show(2)[0].details,
        ["triggered by error: aborted by operator"]
    );
}
#[test]
fn mcs_024_strict_deadline_and_earliest_error_survive_exit_timeout() {
    let mut c = Controller::new(Arc::new(RecoverySteps::new(24)));
    c.stage = Stage::CollectReport;
    c.expected_reports = [(1, None), (2, None)].into();
    c.timeout_unix_nanos = 100;
    c.plan_execution_timeout_nanos = 5;
    assert!(
        c.check_timeout(100, |_| panic!("deadline equality is still valid"))
            .is_ok()
    );
    let err = c
        .check_timeout(101, |t| format!("original-deadline-{t}"))
        .unwrap_err();
    assert!(!c.handle_error(err, 101));
    assert_eq!(c.timeout_unix_nanos, 111);
    assert_eq!(c.stage(), Stage::ExitForceLeader);
    let step = c.dispatch_plan(1, 102).unwrap().step;
    c.abort(105).unwrap();
    assert_eq!(c.step(), step);
    assert_eq!(
        c.timeout_unix_nanos, 111,
        "repeat abort cannot extend the exit deadline"
    );
    assert!(c.check_timeout(111, |_| unreachable!()).is_ok());
    let second = c
        .check_timeout(112, |_| "exit-deadline".into())
        .unwrap_err();
    assert!(c.handle_error(second, 112));
    assert_eq!(c.stage(), Stage::Failed);
    assert!(c.dispatch_plan(1, 113).is_none());
    assert_eq!(
        c.show(113).last().unwrap().info,
        "Unsafe recovery Failed: Exceeds timeout original-deadline-100"
    );
    assert_eq!(
        c.show(113).last().unwrap().details,
        [
            "no newly created empty regions",
            "Stores that have not dispatched plan: 2",
            "Stores that have reported to PD: ",
            "Stores that have not reported to PD: 1"
        ]
    );
    assert!(
        c.abort(114)
            .unwrap_err()
            .to_string()
            .contains("no ongoing unsafe recovery")
    );
    assert_eq!(c.show(113), c.show(115));
}
#[test]
fn mcs_024_errors_discard_old_force_payload_and_clear_confirmations() {
    let mut c = Controller::new(Arc::new(RecoverySteps::new(24)));
    assert!(c.abort(0).is_err());
    c.stage = Stage::ForceLeader;
    c.step = 24;
    c.expected_reports.insert(
        1,
        Some(StoreReport {
            step: 24,
            ..Default::default()
        }),
    );
    c.num_stores_reported = 1;
    c.plans.insert(
        1,
        RecoveryPlan {
            force_leader: Some(ForceLeaderPlan {
                enter_force_leaders: vec![7],
                failed_stores: vec![2],
            }),
            ..Default::default()
        },
    );
    assert!(!c.handle_error(
        RecoveryError::Backend(
            "can't find available store(exclude tiflash) to create new region".into()
        ),
        0
    ));
    assert!(c.reports()[&1].is_none());
    assert_eq!(c.reported_store_count(), 0);
    let plan = c.dispatch_plan(1, 1).unwrap();
    assert!(plan.force_leader.is_none());
    assert!(plan.creates.is_empty());
    assert!(
        !c.collect_report(
            1,
            Some(StoreReport {
                step: 24,
                ..Default::default()
            })
        )
        .unwrap()
    );
    assert_eq!(c.reported_store_count(), 0);
}
