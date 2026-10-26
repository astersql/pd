// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{BasicCluster, NodeState, SlowTrend, StoreInfo, StoreMeta, StoreStats};
use std::sync::Arc;

fn store(id: u64, value: f64, cause: f64, result: f64) -> StoreInfo {
    let mut s = StoreInfo::new(StoreMeta {
        id,
        last_heartbeat: 100,
        ..Default::default()
    });
    s.set_new_store_stats(StoreStats {
        slow_trend: Some(SlowTrend {
            cause_value: value,
            cause_rate: cause,
            result_rate: result,
            ..Default::default()
        }),
        ..Default::default()
    });
    s
}
fn cluster(n: u64) -> BasicCluster {
    let c = BasicCluster::new();
    for id in 1..=n {
        c.stores.put_store(Arc::new(store(
            id,
            if id == 1 { 100.0 } else { 10.0 },
            if id == 1 { 1.0 } else { 0.0 },
            -1.0,
        )));
    }
    c
}
#[test]
fn task_083_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_083.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let c = cluster(case["stores"].as_u64().unwrap());
        c.stores.put_store(Arc::new(store(
            1,
            case["value"].as_f64().unwrap(),
            case["cause"].as_f64().unwrap(),
            case["result"].as_f64().unwrap(),
        )));
        let last = case["last"]
            .as_bool()
            .unwrap()
            .then_some(SlowTrendCandidate {
                store_id: 1,
                capture_ts: 0,
                recover_ts: 0,
            });
        let got = choose_slow_trend_candidate(
            &c,
            case["raft_v2"].as_bool().unwrap(),
            case["ratio"].as_f64().unwrap(),
            last.as_ref(),
            case["now"].as_i64().unwrap(),
        )
        .map(|s| s.id())
        .unwrap_or(0);
        assert_eq!(got, case["expected"].as_u64().unwrap(), "{}", case["name"]);
    }
    println!(
        "task_083: {} frozen source-derived candidate cases",
        fixture["cases"].as_array().unwrap().len()
    );
}
#[test]
fn task_083_updates_and_comparison_boundaries() {
    for (n, expected) in [
        (0, false),
        (1, false),
        (2, true),
        (3, false),
        (4, false),
        (5, false),
    ] {
        let c = cluster(n);
        assert_eq!(slow_trend_stores_updated(&c, 1, 101), expected, "n={n}");
        if n >= 3 {
            let mut s = store(2, 10.0, 0.0, -1.0);
            s.set_last_heartbeat_unix_nanos(101);
            c.stores.put_store(Arc::new(s));
            assert_eq!(slow_trend_stores_updated(&c, 1, 101), n <= 4);
            c.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
                id: 3,
                node_state: NodeState::Removing,
                ..Default::default()
            })));
            assert!(slow_trend_stores_updated(&c, 1, 101));
        }
    }
    let c = cluster(3);
    let target = c.stores.get_store(1).unwrap();
    assert!(slow_trend_slower_than_others(&c, &target));
    assert!(!slow_trend_faster_than_others(&c, &target));
    for (v, expected) in [
        (11.0, true),
        (11.000001, false),
        (0.0, false),
        (f64::NAN, false),
    ] {
        assert_eq!(
            slow_trend_faster_than_others(&c, &store(1, v, 0.0, 0.0)),
            expected
        );
    }
    let missing = StoreInfo::new(StoreMeta {
        id: 1,
        ..Default::default()
    });
    assert!(!slow_trend_slower_than_others(&c, &missing));
    assert!(!slow_trend_faster_than_others(&c, &missing));
    for state in [
        NodeState::Removed,
        NodeState::Removing,
        NodeState::Unknown(9),
    ] {
        c.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id: 2,
            node_state: state,
            ..Default::default()
        })));
        assert!(!slow_trend_slower_than_others(&c, &target));
    }
    c.stores.put_store(Arc::new(store(2, 0.0, 0.0, -1.0)));
    assert!(!slow_trend_slower_than_others(&c, &target));
    // Multiple candidates must never depend on registry iteration order.
    c.stores.put_store(Arc::new(store(2, 90.0, 1.0, -1.0)));
    assert!(choose_slow_trend_candidate(&c, false, 0.0, None, 0).is_none());
    println!(
        "task_083: heartbeat quorum, exact timestamp, lifecycle, missing/zero/NaN data and ambiguous candidates"
    );
}
#[test]
fn task_083_capture_pop_and_time() {
    let mut state = SlowTrendCandidates::default();
    state.mark_recovered(10);
    assert_eq!(state.last, None);
    state.capture(1, 100, 101);
    let first = state.current.unwrap();
    assert_eq!(state.last, Some(first));
    assert_eq!(first.captured_secs(999_999_999), 0);
    assert_eq!(first.captured_secs(1_000_000_100), 1);
    assert_eq!(state.pop(true), 1);
    assert_eq!(state.current, None);
    state.mark_recovered(200);
    assert_eq!(state.last.unwrap().recover_ts, 200);
    state.capture(2, 300, 301);
    assert_eq!(state.last.unwrap().store_id, 1);
    assert_eq!(state.pop(false), 2);
    assert_eq!(state.last.unwrap().store_id, 1);
    state.capture(3, 400, 401);
    assert_eq!(state.pop(true), 3);
    assert_eq!(state.last.unwrap().store_id, 3);
    assert_eq!(state.pop(false), 0);
    assert_eq!(state.last.unwrap().store_id, 3);
    assert_eq!(state.pop(true), 0);
    assert_eq!(state.last, None);
}
#[test]
fn task_083_candidate_heartbeat_flow() {
    let c = cluster(3);
    let mut state = SlowTrendCandidates::default();
    let chosen = choose_slow_trend_candidate(&c, false, 0.5, None, 200).unwrap();
    state.capture(chosen.id(), 200, 201);
    assert!(!slow_trend_stores_updated(&c, chosen.id(), 200));
    let mut updated = store(2, 10.0, 0.0, -1.0);
    updated.set_last_heartbeat_unix_nanos(200);
    c.stores.put_store(Arc::new(updated));
    assert!(slow_trend_stores_updated(&c, chosen.id(), 200));
    assert_eq!(state.pop(true), 1);
    state.capture(1, 300, 301);
    c.stores.put_store(Arc::new(store(1, 10.0, 0.0, 0.0)));
    assert!(slow_trend_faster_than_others(
        &c,
        &c.stores.get_store(1).unwrap()
    ));
    assert_eq!(state.pop(false), 1);
    assert_eq!(state.last.unwrap().capture_ts, 200);
}

#[test]
fn task_083_lifecycle_and_missing_reports() {
    for state in [
        NodeState::Preparing,
        NodeState::Serving,
        NodeState::Removing,
        NodeState::Removed,
        NodeState::Unknown(9),
    ] {
        let c = cluster(3);
        let mut s = store(1, 100.0, 1.0, -1.0);
        let stats = s.store_stats();
        s = StoreInfo::new(StoreMeta {
            id: 1,
            node_state: state,
            ..Default::default()
        });
        s.set_new_store_stats(stats);
        c.stores.put_store(Arc::new(s));
        assert_eq!(
            choose_slow_trend_candidate(&c, false, 0.5, None, 0).is_some(),
            matches!(state, NodeState::Preparing | NodeState::Serving)
        );
        assert!(!slow_trend_stores_updated(&c, 99, 101));
    }
    let c = cluster(3);
    c.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 1,
        ..Default::default()
    })));
    assert!(choose_slow_trend_candidate(&c, false, 0.0, None, 0).is_none());
    let c = cluster(3);
    c.stores.put_store(Arc::new(store(2, 1e-9, 0.0, -1.0)));
    assert!(!slow_trend_slower_than_others(
        &c,
        &c.stores.get_store(1).unwrap()
    ));
    // Removed nodes remain in the quorum denominator, although never compared.
    c.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 4,
        node_state: NodeState::Removed,
        ..Default::default()
    })));
    assert!(choose_slow_trend_candidate(&c, false, 0.5, None, 0).is_none());
}
