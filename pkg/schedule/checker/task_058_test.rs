// Copyright 2026 AsterSQL.
use super::*;
use prometheus::Registry;
use std::time::Duration;

fn metric(registry: &Registry, name: &str, labels: &[(&str, &str)]) -> prometheus::proto::Metric {
    registry
        .gather()
        .into_iter()
        .find(|f| f.name() == name)
        .unwrap()
        .get_metric()
        .iter()
        .find(|m| {
            labels.iter().all(|(key, value)| {
                m.get_label()
                    .iter()
                    .any(|l| l.name() == *key && l.value() == *value)
            })
        })
        .unwrap()
        .clone()
}
#[test]
fn task_058_behavior_cases() {
    let registry = Registry::new();
    let metrics = CheckerMetrics::register(&registry).unwrap();
    metrics.event(CheckerMetricKind::Joint, "check").unwrap();
    assert_eq!(
        metric(
            &registry,
            "pd_checker_event_count",
            &[("type", "joint_state_checker"), ("name", "check")]
        )
        .get_counter()
        .get_value(),
        1.0
    );
    metrics.observe_checker_metrics(CheckerMetricKind::Joint, Duration::from_millis(25));
    let duration = metric(
        &registry,
        "pd_checker_check_region_duration_seconds",
        &[("checker", "joint_state_checker")],
    );
    assert_eq!(duration.get_histogram().get_sample_count(), 1);
    assert_eq!(duration.get_histogram().get_sample_sum(), 0.025);
}

#[tokio::test(start_paused = true)]
async fn task_058_real_dispatch_and_patrol_lifecycle() {
    use tokio_util::sync::CancellationToken;
    for paused in [false, true] {
        let registry = Registry::new();
        let metrics = CheckerMetrics::register(&registry).unwrap();
        let host = crate::task_056_test::Host::new(2, 1, true).await;
        host.install_checker_metrics(metrics.clone(), paused).await;
        let mut patrol = Patrol::new();
        patrol.set_metrics(metrics);
        let cancel = CancellationToken::new();
        let (result, ()) = tokio::join!(patrol.run(&host, cancel.clone()), async {
            tokio::time::sleep(Duration::from_millis(35)).await;
            assert!(
                metric(
                    &registry,
                    "pd_checker_check_region_duration_seconds",
                    &[("checker", "joint_state_checker")]
                )
                .get_histogram()
                .get_sample_count()
                    > 0
            );
            assert!(
                metric(
                    &registry,
                    "pd_checker_event_count",
                    &[
                        ("type", "joint_state_checker"),
                        ("name", if paused { "paused" } else { "check" })
                    ]
                )
                .get_counter()
                .get_value()
                    > 0.0
            );
            assert!(
                metric(&registry, "pd_checker_patrol_regions_time", &[])
                    .get_gauge()
                    .get_value()
                    > 0.0
            );
            host.halt_for_metrics(true);
            tokio::time::sleep(Duration::from_millis(10)).await;
            cancel.cancel();
        });
        result.unwrap();
        assert_eq!(
            metric(&registry, "pd_checker_patrol_regions_time", &[])
                .get_gauge()
                .get_value(),
            0.0
        );
        assert_eq!(
            metric(&registry, "pd_checker_split_scatter_pending", &[])
                .get_gauge()
                .get_value(),
            0.0
        );
        host.shutdown().await;
    }
}

#[test]
fn task_058_schema_and_bounded_events() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_058.json"
    ))
    .unwrap();
    let registry = Registry::new();
    let metrics = CheckerMetrics::register(&registry).unwrap();
    let families = registry.gather();
    assert_eq!(families.len(), 9);
    for expected in fixture["families"].as_array().unwrap() {
        let family = families
            .iter()
            .find(|f| f.name() == expected[0].as_str().unwrap())
            .unwrap();
        assert_eq!(family.help(), expected[1].as_str().unwrap());
        assert_eq!(
            format!("{:?}", family.get_field_type()),
            expected[2].as_str().unwrap()
        );
        let mut labels: Vec<_> = family.get_metric()[0]
            .get_label()
            .iter()
            .map(|l| l.name())
            .collect();
        labels.sort();
        let mut expected_labels: Vec<_> = expected[3]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l.as_str().unwrap())
            .collect();
        expected_labels.sort();
        assert_eq!(labels, expected_labels);
    }
    for event in fixture["events"].as_array().unwrap() {
        let kind = [
            CheckerMetricKind::Rule,
            CheckerMetricKind::Joint,
            CheckerMetricKind::Learner,
            CheckerMetricKind::Merge,
            CheckerMetricKind::Replica,
            CheckerMetricKind::Split,
            CheckerMetricKind::SplitScatter,
            CheckerMetricKind::Affinity,
        ]
        .into_iter()
        .find(|k| k.as_str() == event[0].as_str().unwrap())
        .unwrap();
        metrics.event(kind, event[1].as_str().unwrap()).unwrap();
        assert_eq!(
            metric(
                &registry,
                "pd_checker_event_count",
                &[
                    ("type", kind.as_str()),
                    ("name", event[1].as_str().unwrap())
                ]
            )
            .get_counter()
            .get_value(),
            1.0
        );
    }
    assert!(
        metrics
            .event(CheckerMetricKind::Joint, "region-123456")
            .is_err()
    );
    let family = registry
        .gather()
        .into_iter()
        .find(|f| f.name() == "pd_checker_event_count")
        .unwrap();
    assert_eq!(
        family.get_metric().len(),
        fixture["events"].as_array().unwrap().len()
    );
    for phase in fixture["phases"].as_array().unwrap() {
        let histogram = metric(
            &registry,
            "pd_checker_patrol_phase_duration_seconds",
            &[("phase", phase.as_str().unwrap())],
        );
        let buckets: Vec<_> = histogram
            .get_histogram()
            .get_bucket()
            .iter()
            .map(|b| b.upper_bound())
            .collect();
        let expected: Vec<_> = fixture["default_buckets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        assert_eq!(buckets, expected);
    }
    println!(
        "task_058 schema: 9 families, {} bounded events, 6 phases, 11 default buckets",
        fixture["events"].as_array().unwrap().len()
    );
}

#[test]
fn task_058_registry_isolation_and_collision_rollback() {
    for (index, name, help) in [
        (0, "pd_checker_event_count", "Counter of checker events."),
        (
            8,
            "pd_checker_split_scatter_pending",
            "Number of split-scatter pending entries.",
        ),
    ] {
        let registry = Registry::new();
        // Match the exact descriptor including labels to force AlreadyReg.
        if index == 0 {
            registry
                .register(Box::new(
                    prometheus::CounterVec::new(
                        prometheus::Opts::new(name, help),
                        &["type", "name"],
                    )
                    .unwrap(),
                ))
                .unwrap();
        } else {
            registry
                .register(Box::new(prometheus::Gauge::new(name, help).unwrap()))
                .unwrap();
        }
        assert!(CheckerMetrics::register(&registry).is_err());
        // An empty host CounterVec is omitted by gather; its descriptor still reserves the name.
        assert_eq!(registry.gather().len(), usize::from(index != 0));
        if index == 8 {
            // Rollback must also release descriptors, permitting a clean retry.
            registry
                .unregister(Box::new(prometheus::Gauge::new(name, help).unwrap()))
                .unwrap();
            assert!(CheckerMetrics::register(&registry).is_ok());
        }
    }
    let first = Registry::new();
    let second = Registry::new();
    let a = CheckerMetrics::register(&first).unwrap();
    let b = CheckerMetrics::register(&second).unwrap();
    a.event(CheckerMetricKind::Split, "check").unwrap();
    a.clone().event(CheckerMetricKind::Split, "check").unwrap();
    b.event(CheckerMetricKind::Split, "check").unwrap();
    assert_eq!(
        metric(
            &first,
            "pd_checker_event_count",
            &[("type", "split_checker"), ("name", "check")]
        )
        .get_counter()
        .get_value(),
        2.0
    );
    assert_eq!(
        metric(
            &second,
            "pd_checker_event_count",
            &[("type", "split_checker"), ("name", "check")]
        )
        .get_counter()
        .get_value(),
        1.0
    );
}

#[test]
fn task_058_split_scatter_transitions() {
    let registry = Registry::new();
    let metrics = CheckerMetrics::register(&registry).unwrap();
    let mut retry = SplitScatterRetryController::default();
    retry.set_metrics(metrics);
    retry.record_batch(1, 1, &[2], Duration::ZERO, true);
    assert_eq!(
        metric(&registry, "pd_checker_split_scatter_pending", &[])
            .get_gauge()
            .get_value(),
        2.0
    );
    retry.record_batch(1, 1, &[2], Duration::ZERO, true);
    retry.cleanup_expired(SPLIT_SCATTER_PENDING_TTL);
    retry.cleanup_expired(SPLIT_SCATTER_PENDING_TTL);
    assert_eq!(
        metric(
            &registry,
            "pd_checker_split_scatter_pending_expired_total",
            &[("attempted", "false")]
        )
        .get_counter()
        .get_value(),
        2.0
    );
    assert_eq!(
        metric(&registry, "pd_checker_split_scatter_pending", &[])
            .get_gauge()
            .get_value(),
        0.0
    );
    let children: Vec<u64> = (2..=4096).collect();
    retry.record_batch(1, 1, &children, Duration::ZERO, true);
    retry.record_batch(5000, 1, &[5001], Duration::ZERO, true);
    assert_eq!(
        metric(
            &registry,
            "pd_checker_split_scatter_pending_dropped_total",
            &[]
        )
        .get_counter()
        .get_value(),
        2.0
    );
    retry.clear();
    assert_eq!(
        metric(&registry, "pd_checker_split_scatter_pending", &[])
            .get_gauge()
            .get_value(),
        0.0
    );
    assert_eq!(
        metric(
            &registry,
            "pd_checker_split_scatter_pending_dropped_total",
            &[]
        )
        .get_counter()
        .get_value(),
        2.0
    );
}

#[tokio::test(start_paused = true)]
async fn task_058_host_failure_and_pre_cancel() {
    use tokio_util::sync::CancellationToken;
    for fail in [false, true] {
        let registry = Registry::new();
        let metrics = CheckerMetrics::register(&registry).unwrap();
        let host = crate::task_056_test::Host::new(1, 1, true).await;
        host.install_checker_metrics(metrics.clone(), false).await;
        let mut patrol = Patrol::new();
        patrol.set_metrics(metrics);
        let cancel = CancellationToken::new();
        if fail {
            host.fail_for_metrics();
        } else {
            cancel.cancel();
        }
        let result = patrol.run(&host, cancel).await;
        if fail {
            assert_eq!(
                result,
                Err(PatrolError::Host(crate::task_056_test::Fault::Injected))
            );
        } else {
            result.unwrap();
        }
        assert_eq!(
            metric(&registry, "pd_checker_patrol_regions_time", &[])
                .get_gauge()
                .get_value(),
            0.0
        );
        assert_eq!(
            metric(&registry, "pd_checker_split_scatter_pending", &[])
                .get_gauge()
                .get_value(),
            0.0
        );
        // The failure occurs at host I/O before the dispatcher, so no checker call is fabricated.
        assert_eq!(
            metric(
                &registry,
                "pd_checker_check_region_duration_seconds",
                &[("checker", "joint_state_checker")]
            )
            .get_histogram()
            .get_sample_count(),
            0
        );
        host.shutdown().await;
    }
}

#[tokio::test(start_paused = true)]
async fn task_058_queue_snapshots_and_backlog() {
    use tokio_util::sync::CancellationToken;
    let registry = Registry::new();
    let metrics = CheckerMetrics::register(&registry).unwrap();
    let host = crate::task_056_test::Host::new(2, 0, true).await;
    host.install_checker_metrics(metrics.clone(), true).await;
    host.seed_metrics_queues().await;
    let mut patrol = Patrol::new();
    patrol.set_metrics(metrics);
    let cancel = CancellationToken::new();
    let (result, ()) = tokio::join!(patrol.run(&host, cancel.clone()), async {
        tokio::time::sleep(Duration::from_millis(35)).await;
        assert_eq!(
            metric(
                &registry,
                "pd_checker_region_list",
                &[("type", "priority_list")]
            )
            .get_gauge()
            .get_value(),
            2.0
        );
        assert_eq!(
            metric(
                &registry,
                "pd_checker_region_list",
                &[("type", "pending_processed_regions")]
            )
            .get_gauge()
            .get_value(),
            2.0
        );
        assert_eq!(
            metric(&registry, "pd_checker_patrol_region_channel_size", &[])
                .get_gauge()
                .get_value(),
            2.0
        );
        assert!(
            metric(
                &registry,
                "pd_checker_patrol_phase_duration_seconds",
                &[("phase", "wait_for_channel")]
            )
            .get_histogram()
            .get_sample_count()
                >= 2
        );
        assert_eq!(
            metric(
                &registry,
                "pd_checker_patrol_phase_duration_seconds",
                &[("phase", "scan_regions")]
            )
            .get_histogram()
            .get_sample_count(),
            1
        );
        assert_eq!(
            metric(
                &registry,
                "pd_checker_event_count",
                &[("type", "learner_checker"), ("name", "paused")]
            )
            .get_counter()
            .get_value(),
            2.0
        );
        // Stage duration is measured even when the schedule limit prevents calling affinity/merge.
        assert_eq!(
            metric(
                &registry,
                "pd_checker_check_region_duration_seconds",
                &[("checker", "merge_checker")]
            )
            .get_histogram()
            .get_sample_count(),
            2
        );
        assert_eq!(
            metric(
                &registry,
                "pd_checker_event_count",
                &[("type", "merge_checker"), ("name", "check")]
            )
            .get_counter()
            .get_value(),
            0.0
        );
        host.halt_for_metrics(true);
        tokio::time::sleep(Duration::from_millis(10)).await;
        cancel.cancel();
    });
    result.unwrap();
    // Go leaves queue gauges at their last sampled value; stopping does not fabricate a new sample.
    assert_eq!(
        metric(&registry, "pd_checker_patrol_region_channel_size", &[])
            .get_gauge()
            .get_value(),
        2.0
    );
    host.shutdown().await;
}

#[test]
fn task_058_split_missing_event_and_attempted_expiration() {
    use astersql_pd_core::{BasicCluster, KeyRange, RegionEpoch, RegionInfo, RegionMeta};
    use astersql_pd_schedule_operator::ControllerAdmission;
    use std::sync::Arc;
    let registry = Registry::new();
    let metrics = CheckerMetrics::register(&registry).unwrap();
    let mut retry = SplitScatterRetryController::default();
    retry.set_metrics(metrics);
    retry.record_batch(1, 1, &[2], Duration::ZERO, true);
    let mut cluster = BasicCluster::new();
    let ops = ControllerAdmission::new(10);
    assert!(
        retry
            .top_pending(&cluster.regions, &ops, Duration::ZERO, 2)
            .is_empty()
    );
    assert!(
        retry
            .top_pending(&cluster.regions, &ops, Duration::from_secs(9), 2)
            .is_empty()
    );
    assert_eq!(
        metric(
            &registry,
            "pd_checker_event_count",
            &[
                ("type", "split_scatter_checker"),
                ("name", "dispatch-region-missing")
            ]
        )
        .get_counter()
        .get_value(),
        2.0
    );
    for id in [1u64, 2] {
        cluster.regions.set_region(Arc::new(RegionInfo::new(
            RegionMeta {
                id,
                epoch: Some(RegionEpoch {
                    version: 1,
                    conf_ver: 1,
                }),
                key_range: KeyRange {
                    start_key: id.to_be_bytes().to_vec(),
                    end_key: (id + 1).to_be_bytes().to_vec(),
                },
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        )));
    }
    assert_eq!(
        retry
            .top_pending(&cluster.regions, &ops, Duration::from_secs(10), 1)
            .len(),
        1
    );
    retry.cleanup_expired(SPLIT_SCATTER_PENDING_TTL);
    assert_eq!(
        metric(
            &registry,
            "pd_checker_split_scatter_pending_expired_total",
            &[("attempted", "true")]
        )
        .get_counter()
        .get_value(),
        1.0
    );
    assert_eq!(
        metric(
            &registry,
            "pd_checker_split_scatter_pending_expired_total",
            &[("attempted", "false")]
        )
        .get_counter()
        .get_value(),
        1.0
    );
}
