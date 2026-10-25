// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{BasicCluster, StoreInfo, StoreMeta, StoreState};
use prometheus::{CounterVec, Opts, Registry, proto::MetricType};
use serde_json::{Value, json};
use std::sync::Arc;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_011.json"
    ))
    .unwrap()
}
fn action(s: &str) -> FilterAction {
    match s {
        "source" => FilterAction::Source,
        "target" => FilterAction::Target,
        _ => panic!("invalid fixture action"),
    }
}
fn snapshot(registry: &Registry) -> Value {
    let mut rows = vec![];
    for family in registry.gather() {
        assert_eq!(family.get_field_type(), MetricType::COUNTER);
        let direction = if family.name() == "pd_schedule_source_filter" {
            "source"
        } else {
            assert_eq!(family.name(), "pd_schedule_target_filter");
            "target"
        };
        assert_eq!(family.help(), format!("Counter of the {direction} filter"));
        for metric in family.get_metric() {
            let labels: std::collections::BTreeMap<_, _> = metric
                .get_label()
                .iter()
                .map(|l| (l.name(), l.value()))
                .collect();
            assert_eq!(labels.len(), 3);
            rows.push(json!({"direction":direction,"scope":labels["scope"],"filter":labels["type"],"store":labels[direction],"value":metric.get_counter().get_value() as u64}));
        }
    }
    rows.sort_by_key(Value::to_string);
    Value::Array(rows)
}

#[test]
fn task_011_behavior_cases() {
    let fixture = fixture();
    let registry = Registry::new();
    let metrics = FilterMetrics::register(&registry).unwrap();
    let cluster = BasicCluster::new();
    for id in fixture["initial_stores"].as_array().unwrap() {
        cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id: id.as_u64().unwrap(),
            ..Default::default()
        })));
    }
    let mut counter = FilterCounter::new("balance-leader-scheduler");
    for case in fixture["cases"].as_array().unwrap() {
        for op in case["operations"].as_array().unwrap() {
            match op[0].as_str().unwrap() {
                "inc" => counter.inc(
                    action(op[1].as_str().unwrap()),
                    FilterType(op[2].as_i64().unwrap()),
                    op[3].as_u64().unwrap(),
                ),
                "flush" => counter.flush(
                    &metrics,
                    if op[1].as_bool().unwrap() {
                        Some(&cluster)
                    } else {
                        None
                    },
                ),
                "scope" => counter.set_scope(op[1].as_str().unwrap()),
                "delete" => metrics.delete_store_metrics(op[1].as_str().unwrap()),
                "reset" => metrics.reset_filter_metrics(),
                "remove" => {
                    assert!(
                        cluster
                            .stores
                            .patch_store(op[1].as_u64().unwrap(), |store| store
                                .set_store_state(StoreState::Tombstone, None))
                    );
                }
                _ => panic!("invalid fixture operation"),
            }
        }
        let mut expected = case["metrics"].as_array().unwrap().clone();
        expected.sort_by_key(Value::to_string);
        assert_eq!(
            snapshot(&registry),
            Value::Array(expected),
            "{}",
            case["id"]
        );
        for (key, value) in case["pending_keys"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["pending"].as_array().unwrap())
        {
            let a = if key[0] == 0 {
                FilterAction::Source
            } else {
                FilterAction::Target
            };
            assert_eq!(
                counter.pending(
                    a,
                    FilterType(key[1].as_i64().unwrap()),
                    key[2].as_u64().unwrap()
                ),
                value.as_u64().unwrap(),
                "{}",
                case["id"]
            );
        }
    }
    for (i, name) in fixture["filter_names"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        assert_eq!(FilterType(i as i64).as_str(), name.as_str().unwrap());
    }
    assert_eq!(FilterType(26).as_str(), "unknown");
    assert_eq!(FilterAction::Source.as_str(), "filter-source");
    assert_eq!(FilterAction::Target.as_str(), "filter-target");
    println!(
        "task_011: {} frozen lifecycle cases; 26 filter names",
        fixture["cases"].as_array().unwrap().len()
    );
}

#[test]
fn task_011_registry_isolation_and_registration_rollback() {
    let first = Registry::new();
    let second = Registry::new();
    let metrics = FilterMetrics::register(&first).unwrap();
    let other = FilterMetrics::register(&second).unwrap();
    assert!(FilterMetrics::register(&first).is_err());
    let mut counter = FilterCounter::new("scope");
    counter.inc(FilterAction::Source, FilterType::EXCLUDED, u64::MAX);
    counter.flush(&metrics, None);
    assert_eq!(snapshot(&first).as_array().unwrap().len(), 1);
    assert_eq!(snapshot(&second), json!([]));
    // A clone shares one host's vectors. Deletion invalidates cached handles.
    metrics.clone().delete_store_metrics(&u64::MAX.to_string());
    assert_eq!(snapshot(&first), json!([]));
    counter.inc(FilterAction::Source, FilterType::EXCLUDED, u64::MAX);
    counter.flush(&metrics, None);
    assert_eq!(snapshot(&first)[0]["value"], 1);
    other.reset_filter_metrics();
    assert_eq!(snapshot(&first).as_array().unwrap().len(), 1);
    metrics.reset_filter_metrics();
    assert_eq!(snapshot(&first), json!([]));
    counter.inc(FilterAction::Source, FilterType::EXCLUDED, u64::MAX);
    counter.flush(&metrics, None);
    assert_eq!(snapshot(&first)[0]["value"], 1);
    // If registering the target fails, the source registration is rolled back.
    let conflict = Registry::new();
    conflict
        .register(Box::new(
            CounterVec::new(
                Opts::new("pd_schedule_target_filter", "Counter of the target filter"),
                &["scope", "type", "target"],
            )
            .unwrap(),
        ))
        .unwrap();
    assert!(FilterMetrics::register(&conflict).is_err());
    conflict
        .register(Box::new(
            CounterVec::new(
                Opts::new("pd_schedule_source_filter", "Counter of the source filter"),
                &["scope", "type", "source"],
            )
            .unwrap(),
        ))
        .unwrap();
}

#[test]
fn task_011_removed_pending_never_reappears() {
    let registry = Registry::new();
    let metrics = FilterMetrics::register(&registry).unwrap();
    let cluster = BasicCluster::new();
    cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 1,
        ..Default::default()
    })));
    let mut counter = FilterCounter::new("scope");
    counter.inc(FilterAction::Target, FilterType::EXCLUDED, 1);
    cluster
        .stores
        .patch_store(1, |s| s.set_store_state(StoreState::Tombstone, None));
    counter.flush(&metrics, Some(&cluster));
    assert_eq!(snapshot(&registry), json!([]));
    counter.flush(&metrics, None);
    assert_eq!(snapshot(&registry), json!([]));
    counter.inc(FilterAction::Source, FilterType::EXCLUDED, 99);
    counter.flush(&metrics, Some(&cluster));
    counter.flush(&metrics, None);
    assert_eq!(snapshot(&registry), json!([]));
}

#[test]
#[should_panic(expected = "negative Go filter identity")]
fn task_011_negative_filter_identity() {
    FilterType(-1).as_str();
}

#[test]
fn task_011_shared_metrics_concurrent_batches() {
    let registry = Registry::new();
    let metrics = FilterMetrics::register(&registry).unwrap();
    let workers: Vec<_> = (0..4)
        .map(|scope| {
            let metrics = metrics.clone();
            std::thread::spawn(move || {
                let mut counter = FilterCounter::new(scope.to_string());
                for _ in 0..100 {
                    counter.inc(FilterAction::Source, FilterType::EXCLUDED, 1);
                    counter.inc(FilterAction::Target, FilterType::EXCLUDED, 1);
                    counter.flush(&metrics, None);
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    let rows = snapshot(&registry);
    assert_eq!(rows.as_array().unwrap().len(), 8);
    assert!(rows.as_array().unwrap().iter().all(|r| r["value"] == 100));
    metrics.delete_store_metrics("01");
    assert_eq!(snapshot(&registry), rows);
    metrics.delete_store_metrics("1");
    assert_eq!(snapshot(&registry), json!([]));
}
