// Copyright 2026 AsterSQL.
use super::core_trace::*;
use prometheus::Registry;
use std::cell::Cell;
use std::time::Duration;

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("../core/rust-fixtures/core_task_59.json")).unwrap()
}
#[test]
fn core_task_59_each_stage_and_clock_regression() {
    let registry = Registry::new();
    let metrics = HeartbeatMetrics::register(&registry).unwrap();
    let time = Cell::new(100_u64);
    let mut tracer = HeartbeatTracer::new(&metrics, || Duration::from_millis(time.get()));
    tracer.begin();
    for (i, stage) in Stage::ALL.into_iter().enumerate() {
        time.set(100 + (i as u64 + 1) * 100);
        tracer.finish(stage);
        assert!(
            (metrics.duration(stage) - fixture()["expected"]["positive_seconds"].as_f64().unwrap())
                .abs()
                < 1e-12
        );
        assert_eq!(metrics.count(stage), 1.0);
    }
    for stage in Stage::ALL {
        tracer.begin();
        time.set(time.get() - 1);
        tracer.finish(stage);
        tracer.finish(stage); // equal clock still counts
        assert!(
            (metrics.duration(stage) - fixture()["expected"]["positive_seconds"].as_f64().unwrap())
                .abs()
                < 1e-12
        );
        assert_eq!(metrics.count(stage), 3.0);
    }
    assert_eq!(registry.gather().len(), 2);
    assert_eq!(
        fixture()["stage_labels"].as_array().unwrap().len(),
        Stage::ALL.len()
    );
    for (stage, label) in Stage::ALL
        .into_iter()
        .zip(fixture()["stage_labels"].as_array().unwrap())
    {
        assert_eq!(stage.label(), label.as_str().unwrap());
    }
}
#[test]
fn core_task_59_save_cache_resets_and_recovery() {
    let metrics = HeartbeatMetrics::register(&Registry::new()).unwrap();
    let time = Cell::new(0_u64);
    let mut tracer = HeartbeatTracer::new(&metrics, || Duration::from_millis(time.get()));
    tracer.begin();
    for row in fixture()["boundaries"].as_array().unwrap() {
        time.set(row["ms"].as_u64().unwrap());
        match row["action"].as_str().unwrap() {
            "save_begin" => tracer.on_save_cache_begin(),
            "save_end" => tracer.on_save_cache_finished(),
            "overlaps" => tracer.finish(Stage::CheckOverlaps),
            "other" => tracer.finish(Stage::Other),
            _ => unreachable!(),
        }
    }
    assert_eq!(
        metrics.duration(Stage::CheckOverlaps),
        fixture()["expected"]["overlaps_seconds"].as_f64().unwrap()
    );
    assert_eq!(
        metrics.duration(Stage::Other),
        fixture()["expected"]["other_seconds"].as_f64().unwrap()
    );
    assert_eq!(metrics.count(Stage::Other), 2.0);
    assert_eq!(metrics.count(Stage::PreCheck), 0.0);
}
#[test]
fn core_task_59_registry_ownership() {
    let a = Registry::new();
    let metrics = HeartbeatMetrics::register(&a).unwrap();
    assert!(HeartbeatMetrics::register(&a).is_err());
    let b = Registry::new();
    let independent = HeartbeatMetrics::register(&b).unwrap();
    let time = Cell::new(0_u64);
    let mut tracer = HeartbeatTracer::new(&metrics, || Duration::from_secs(time.get()));
    tracer.begin();
    time.set(2);
    tracer.finish(Stage::PreCheck);
    assert_eq!(metrics.duration(Stage::PreCheck), 2.0);
    assert_eq!(independent.count(Stage::PreCheck), 0.0);
    let family = a.gather();
    assert_eq!(family[0].get_metric().len(), 9);
    assert_eq!(family[0].get_metric()[0].get_label()[0].name(), "name");
}
