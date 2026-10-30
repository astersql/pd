// Copyright 2026 AsterSQL.
use std::sync::Arc;
#[tokio::test]
async fn task_048_coordinator_keeps_state_and_uses_new_generation() {
    let (mut coordinator, pending) = crate::task_117_test::setup(
        serde_json::json!({"schedule":{"schedulers-v2":[{"type":"balance-leader"}]}}),
        Arc::new(astersql_pd_storage_domain::MemoryStore::default()),
    );
    coordinator
        .init_schedulers(true, &tokio_util::sync::CancellationToken::new())
        .await
        .unwrap();
    coordinator
        .get_schedulers_controller_mut()
        .pause_or_resume("balance-leader-scheduler", 60, 0)
        .unwrap();
    coordinator.record_op_step_with_ttl(1, 0);
    assert!(!coordinator.get_rule_checker().is_recently_promoted(1, 20));
    let operators = coordinator.operator_handle();
    use astersql_pd_schedule_operator::{OpKind, Operator};
    let queued = || {
        Arc::new(Operator::new(
            "updated-limit",
            "",
            1,
            None,
            OpKind::REGION,
            1,
            vec![],
        ))
    };
    assert!(
        operators
            .enqueue_waiting(coordinator.get_cluster(), &[queued()])
            .is_err()
    );
    let mut next = (**coordinator.get_config()).clone();
    next.schedule.switch_witness_interval = astersql_pd_cache::TtlDuration::from_nanos(100);
    next.schedule.scheduler_max_waiting_operator = 1;
    coordinator.update_config(Arc::new(next));
    operators
        .enqueue_waiting(coordinator.get_cluster(), &[queued()])
        .unwrap();
    assert_eq!(operators.waiting_count("updated-limit"), 1);
    let mut next = (**coordinator.get_config()).clone();
    next.schedule.scheduler_max_waiting_operator = 0;
    coordinator.update_config(Arc::new(next));
    assert_eq!(operators.waiting_count("updated-limit"), 1);
    assert!(
        operators
            .enqueue_waiting(coordinator.get_cluster(), &[queued()])
            .is_err()
    );
    coordinator.record_op_step_with_ttl(2, 0);
    assert!(coordinator.get_rule_checker().is_recently_promoted(2, 20));
    assert!(Arc::ptr_eq(&operators, &coordinator.operator_handle()));
    assert!(coordinator.are_schedulers_initialized());
    assert_eq!(
        coordinator.get_schedulers_controller().names(),
        vec!["balance-leader-scheduler"]
    );
    pending.shutdown().await;
}
