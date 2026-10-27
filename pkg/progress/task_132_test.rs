// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{NodeState, StoreInfo, StoreMeta};
use prometheus::Registry;
use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio_util::sync::CancellationToken;
fn store(id: u64, node_state: NodeState) -> StoreInfo {
    StoreInfo::new(StoreMeta {
        id,
        node_state,
        ..Default::default()
    })
}
fn manager(interval: u64) -> (Manager, Registry) {
    let r = Registry::new();
    let m = Manager::new(
        Arc::new(|| Duration::from_secs(10)),
        Duration::from_secs(interval),
        &r,
    )
    .unwrap();
    (m, r)
}
fn at(s: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(s)
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-10 * b.abs().max(1.0), "{a} != {b}");
}
#[test]
fn task_132_behavior_cases() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_132.json"
    ))
    .unwrap();
    for case in f["cases"].as_array().unwrap() {
        let (m, _) = manager(1);
        let mut s = store(
            1,
            if case["action"] == "preparing" {
                NodeState::Preparing
            } else {
                NodeState::Removing
            },
        );
        for row in case["updates"].as_array().unwrap() {
            m.update_progress_at(Some(&s), row[0].as_f64().unwrap(), 100.0, at(1));
            let p = m.get_progress_by_store_id(1).unwrap();
            close(p.progress_percent, row[1].as_f64().unwrap());
            close(p.current_speed, row[2].as_f64().unwrap());
            close(p.left_second, row[3].as_f64().unwrap());
        }
        s.edit_meta(|v| v.node_state = NodeState::Serving);
        m.update_progress_at(Some(&s), 0.0, 0.0, at(2));
        assert!(m.get_progress_by_store_id(1).is_none());
        let p = m.completed_progress_by_store_id(1).unwrap();
        close(p.progress_percent, 1.0);
        close(p.current_speed, case["finished_speed"].as_f64().unwrap());
    }
}
#[test]
fn task_132_queries_targets_reset_and_switch() {
    let (m, r) = manager(1);
    assert!(m.get_progress_by_store_id(99).is_none());
    assert!(m.get_average_progress_by_action("preparing").is_none());
    m.update_progress_at(None, 0.0, 0.0, at(0));
    let mut a = store(1, NodeState::Preparing);
    let b = store(2, NodeState::Preparing);
    m.update_progress_at(Some(&a), 10.0, 100.0, at(0));
    m.update_progress_at(Some(&b), 20.0, 100.0, at(0));
    assert_eq!(r.gather().len(), 0);
    let p = m.get_average_progress_by_action("preparing").unwrap();
    close(p.progress_percent, 0.15);
    assert_eq!(p.left_second, f64::MAX);
    let snapshot = m.get_progress_by_store_id(1).unwrap();
    m.update_progress_at(Some(&a), 30.0, 200.0, at(1));
    m.update_progress_at(Some(&a), 40.0, 100.0, at(2));
    close(m.get_progress_by_store_id(1).unwrap().progress_percent, 0.2);
    close(snapshot.progress_percent, 0.1);
    a.edit_meta(|v| v.node_state = NodeState::Removing);
    m.update_progress_at(Some(&a), 100.0, 0.0, at(3));
    assert_eq!(
        m.completed_progress_by_store_id(1).unwrap().action,
        "preparing"
    );
    close(m.get_progress_by_store_id(1).unwrap().progress_percent, 0.0);
    m.reset();
    assert!(m.get_progress_by_store_id(1).is_none());
    assert!(m.completed_progress_by_store_id(1).is_some());
    assert!(r.gather().is_empty());
    assert_eq!(m.gc_completed_progress_at(at(100)), 1);
    assert!(
        Manager::new(
            Arc::new(|| Duration::ZERO),
            Duration::ZERO,
            &Registry::new()
        )
        .is_err()
    );
}
#[test]
fn task_132_gc_metrics_strict_ttl() {
    let (m, r) = manager(1);
    let mut s = store(1, NodeState::Removing);
    m.update_progress_at(Some(&s), 100.0, 0.0, at(0));
    m.update_progress_at(Some(&s), 100.0, 0.0, at(1));
    let before = r.gather();
    assert_eq!(before.len(), 3);
    for f in &before {
        let labels = f.get_metric()[0].get_label();
        assert_eq!(
            labels
                .iter()
                .map(|x| (x.name(), x.value()))
                .collect::<Vec<_>>(),
            vec![("action", "removing"), ("address", ""), ("store", "1")]
        );
    }
    s.edit_meta(|v| v.node_state = NodeState::Removed);
    m.update_progress_at(Some(&s), 0.0, 0.0, at(2));
    assert_eq!(before, r.gather());
    assert_eq!(m.gc_completed_progress_at(at(1)), 0);
    assert_eq!(m.gc_completed_progress_at(at(32)), 0);
    assert_eq!(
        m.gc_completed_progress_at(at(32) + Duration::from_nanos(1)),
        1
    );
    assert!(r.gather().is_empty());
    assert!(m.completed_progress_by_store_id(1).is_none());
    assert_eq!(m.gc_completed_progress_at(at(100)), 0);
}
#[test]
fn task_132_dynamic_getter() {
    let (m, _) = manager(60);
    let s = store(1, NodeState::Removing);
    for i in 0..=120 {
        m.update_progress_at(Some(&s), 1000.0 - i as f64, 0.0, at(i));
    }
    close(
        m.get_progress_by_store_id(1).unwrap().current_speed,
        1.0 / 60.0,
    );
    for _ in 0..5 {
        m.update_progress_at(Some(&s), 880.0, 0.0, at(121));
    }
    close(
        m.get_progress_by_store_id(1).unwrap().current_speed,
        0.5 / 60.0,
    );
    m.set_patrol_regions_duration_getter(Arc::new(|| Duration::from_secs(1140)));
    for _ in 0..4 {
        m.update_progress_at(Some(&s), 880.0, 0.0, at(122));
    }
    let p = m.get_progress_by_store_id(1).unwrap();
    close(p.current_speed, 10.0 / 19.0 / 60.0);
    close(p.left_second, 1672.0 * 60.0);
}
#[tokio::test(start_paused = true)]
async fn task_132_gc_cancel_join() {
    let (m, r) = manager(1);
    let m = Arc::new(m);
    let mut s = store(1, NodeState::Removing);
    m.update_progress_at(Some(&s), 100.0, 0.0, at(0));
    m.update_progress_at(Some(&s), 100.0, 0.0, at(1));
    s.edit_meta(|v| v.node_state = NodeState::Removed);
    m.update_progress_at(Some(&s), 0.0, 0.0, at(2));
    let cancel = CancellationToken::new();
    let worker = {
        let m = m.clone();
        let c = cancel.clone();
        tokio::spawn(async move { m.gc(&c).await })
    };
    tokio::task::yield_now().await;
    assert_eq!(r.gather().len(), 3);
    tokio::time::advance(Duration::from_secs(119)).await;
    tokio::task::yield_now().await;
    assert_eq!(r.gather().len(), 3);
    tokio::time::advance(Duration::from_secs(1)).await;
    tokio::task::yield_now().await;
    assert!(r.gather().is_empty());
    cancel.cancel();
    worker.await.unwrap();
    let c = CancellationToken::new();
    c.cancel();
    m.gc(&c).await;
    // Cancellation before first tick retains completed state and never waits 120s.
    m.update_progress_at(Some(&store(2, NodeState::Preparing)), 1.0, 10.0, at(0));
    m.update_progress_at(Some(&store(2, NodeState::Serving)), 1.0, 10.0, at(1));
    m.gc(&c).await;
    assert!(m.completed_progress_by_store_id(2).is_some());
}

#[test]
fn task_132_registration_failure_rolls_back() {
    let r = Registry::new();
    let existing = prometheus::GaugeVec::new(
        prometheus::Opts::new(
            "pd_cluster_speed",
            "The current speed of corresponding action",
        ),
        &["address", "store", "action"],
    )
    .unwrap();
    r.register(Box::new(existing.clone())).unwrap();
    assert!(Manager::new(Arc::new(|| Duration::ZERO), Duration::from_secs(1), &r).is_err());
    // Our earlier progress registration was rolled back; the host speed survives.
    r.unregister(Box::new(existing)).unwrap();
    assert!(Manager::new(Arc::new(|| Duration::ZERO), Duration::from_secs(1), &r).is_ok());
}

#[test]
fn task_132_average_zero_and_repeat() {
    let (m, _) = manager(1);
    for id in [1, 2] {
        m.update_progress_at(
            Some(&store(id, NodeState::Preparing)),
            if id == 1 { 10.0 } else { 20.0 },
            100.0,
            at(0),
        );
        m.update_progress_at(Some(&store(id, NodeState::Preparing)), 30.0, 100.0, at(1));
    }
    let p = m.get_average_progress_by_action("preparing").unwrap();
    close(p.progress_percent, 0.3);
    close(p.current_speed, 15.0);
    close(p.left_second, 5.25);
    assert!(m.get_average_progress_by_action("removing").is_none());
    for _ in 0..2 {
        m.update_progress_at(Some(&store(3, NodeState::Removing)), 0.0, 0.0, at(0));
        m.update_progress_at(Some(&store(3, NodeState::Removing)), 0.0, 0.0, at(1));
        let p = m.get_progress_by_store_id(3).unwrap();
        assert_eq!(
            (p.progress_percent, p.current_speed, p.left_second),
            (1.0, 0.0, 0.0)
        );
        m.update_progress_at(Some(&store(3, NodeState::Removed)), 0.0, 0.0, at(2));
        assert!(m.get_progress_by_store_id(3).is_none());
    }
    m.update_progress_at(Some(&store(9, NodeState::Unknown(99))), 0.0, 0.0, at(0));
    assert!(m.get_progress_by_store_id(9).is_none());
    m.update_progress_at(Some(&store(10, NodeState::Preparing)), 1.0, 10.0, at(0));
    m.update_progress_at(Some(&store(10, NodeState::Removed)), 0.0, 0.0, at(1));
    assert_eq!(m.gc_completed_progress_at(at(100)), 2); // store 10 had no metric series.
}
