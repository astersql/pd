// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{BasicCluster, NodeState, StoreInfo, StoreMeta, StoreStats};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

fn put(c: &BasicCluster, id: u64, scores: HashMap<u64, u64>, state: NodeState) {
    let mut s = StoreInfo::new(StoreMeta {
        id,
        node_state: state,
        ..Default::default()
    });
    s.set_new_store_stats(StoreStats {
        network_slow_scores: scores,
        ..Default::default()
    });
    c.stores.put_store(Arc::new(s));
}
#[test]
fn task_080_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_080.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    for case in cases {
        let c = BasicCluster::new();
        for (id, scores) in case["scores"].as_object().unwrap() {
            let id = id.parse::<u64>().unwrap();
            let state = if case["removed"].as_array().unwrap().contains(&id.into()) {
                NodeState::Removed
            } else if case["states"][id.to_string()] == "removing" {
                NodeState::Removing
            } else {
                NodeState::Serving
            };
            put(
                &c,
                id,
                serde_json::from_value(scores.clone()).unwrap(),
                state,
            );
        }
        let recovering: HashSet<u64> = serde_json::from_value(case["recovering"].clone()).unwrap();
        let expected: Vec<u64> = serde_json::from_value(case["expected"].clone()).unwrap();
        assert_eq!(
            detect_network_slow_candidates(&c, &recovering),
            expected,
            "{}",
            case["name"]
        );
        for id in 1..=5 {
            if let Some(s) = c.stores.get_store(id) {
                assert!(!s.evicted_as_slow_store());
                assert_eq!(s.network_slow_triggers(), 0);
            }
        }
    }
    println!("task_080: {0}/{0} frozen source-derived cases", cases.len());
}
#[test]
fn task_080_filter_graph_and_skip() {
    let c = BasicCluster::new();
    put(
        &c,
        1,
        [(2, 95), (3, 0), (99, 100)].into(),
        NodeState::Serving,
    );
    put(&c, 2, [(1, 100), (3, 100)].into(), NodeState::Serving);
    put(&c, 3, HashMap::new(), NodeState::Removed);
    let recovering = HashSet::from([2]);
    let original = c.stores.get_store(1).unwrap().network_slow_scores();
    assert_eq!(
        filter_network_slow_scores(&c, &original, &recovering),
        HashMap::from([(99, 100)])
    );
    assert_eq!(
        c.stores.get_store(1).unwrap().network_slow_scores(),
        original
    );
    let live: Vec<_> = c
        .stores
        .get_stores()
        .into_iter()
        .filter(|s| !s.is_removed())
        .collect();
    assert!(build_problematic_network_map(&c, &live, &recovering).is_empty());
    let graph = build_problematic_network_map(&c, &live, &HashSet::new());
    assert_eq!(
        graph,
        HashMap::from([
            (1, HashSet::from([2, 99])),
            (2, HashSet::from([1])),
            (99, HashSet::from([1]))
        ])
    );
    assert!(!should_skip_store_evaluation(
        &c.stores.get_store(1).unwrap(),
        &graph,
        &recovering
    ));
    assert!(should_skip_store_evaluation(
        &c.stores.get_store(2).unwrap(),
        &graph,
        &recovering
    ));
    assert!(should_skip_store_evaluation(
        &c.stores.get_store(3).unwrap(),
        &graph,
        &recovering
    ));
}
#[test]
fn task_080_consensus_and_ambiguous_edges() {
    let c = BasicCluster::new();
    for id in 1..=4 {
        put(
            &c,
            id,
            HashMap::new(),
            if id == 4 {
                NodeState::Removed
            } else {
                NodeState::Serving
            },
        );
    }
    put(&c, 1, [(2, 10), (3, 5), (99, 5)].into(), NodeState::Serving);
    let live: Vec<_> = c
        .stores
        .get_stores()
        .into_iter()
        .filter(|s| !s.is_removed())
        .collect();
    let s = c.stores.get_store(1).unwrap();
    let graph = HashMap::from([(1, HashSet::from([2]))]);
    assert!(!is_network_slow_store(
        &c,
        &s,
        &live,
        &graph,
        &HashSet::from([4, 88])
    ));
    assert!(!is_network_slow_store(
        &c,
        &s,
        &live,
        &graph,
        &HashSet::from([3, 4, 88])
    )); // filtering leaves only two scores
    put(
        &c,
        1,
        [(2, 10), (3, 5), (99, 5), (100, 5)].into(),
        NodeState::Serving,
    );
    assert!(is_network_slow_store(
        &c,
        &c.stores.get_store(1).unwrap(),
        &live,
        &graph,
        &HashSet::from([3, 4, 88])
    ));
    assert!(!is_network_slow_store(
        &c,
        &s,
        &live,
        &HashMap::new(),
        &HashSet::new()
    ));
}
#[test]
fn task_080_heartbeat_loss_and_removed_snapshot() {
    let c = BasicCluster::new();
    for id in 1..=4 {
        put(&c, id, HashMap::new(), NodeState::Serving);
    }
    put(
        &c,
        1,
        [(2, 95), (3, 10), (4, 10)].into(),
        NodeState::Serving,
    );
    assert_eq!(detect_network_slow_candidates(&c, &HashSet::new()), vec![1]);
    let old = c.stores.get_store(1).unwrap();
    // Missing heartbeat scores must suppress a fresh decision; a previous snapshot stays immutable.
    put(&c, 1, HashMap::new(), NodeState::Serving);
    assert!(detect_network_slow_candidates(&c, &HashSet::new()).is_empty());
    assert_eq!(old.network_slow_scores().len(), 3);
    put(&c, 1, old.network_slow_scores(), NodeState::Removed);
    assert!(detect_network_slow_candidates(&c, &HashSet::new()).is_empty());
    c.stores.delete_store(1);
    assert!(detect_network_slow_candidates(&c, &HashSet::new()).is_empty());
}
