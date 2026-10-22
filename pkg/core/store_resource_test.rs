// Copyright 2026 AsterSQL.
use crate::{StoreInfo, StoreMeta};
use astersql_pd_core_constant::{ResourceKind, SchedulePolicy};

#[test]
fn core_task_20_resources_and_clone() {
    let mut store = StoreInfo::new(StoreMeta::default());
    assert_eq!(store.leader_count(), 0);
    store.set_leader_count(3);
    store.set_region_count(9);
    store.set_learner_count(2);
    store.set_witness_count(4);
    store.set_pending_peer_count(5);
    store.set_leader_size(30);
    store.set_region_size(90);
    store.set_leader_weight(2.0);
    store.set_region_weight(3.0);
    for mut copy in [store.clone(), store.shallow_clone()] {
        assert_eq!(
            (
                copy.leader_count(),
                copy.region_count(),
                copy.learner_count(),
                copy.witness_count(),
                copy.pending_peer_count()
            ),
            (3, 9, 2, 4, 5)
        );
        assert_eq!(copy.resource_count(ResourceKind::LEADER), 3);
        assert_eq!(copy.resource_count(ResourceKind::REGION), 9);
        assert_eq!(copy.resource_size(ResourceKind::LEADER), 30);
        assert_eq!(copy.resource_size(ResourceKind::REGION), 90);
        assert_eq!(copy.resource_weight(ResourceKind::LEADER), 2.0);
        assert_eq!(copy.resource_weight(ResourceKind::REGION), 3.0);
        copy.set_leader_count(-1);
        assert_eq!(copy.resource_count(ResourceKind::LEADER), u64::MAX);
        copy.set_region_count(-2);
        assert_eq!(copy.resource_count(ResourceKind::REGION), u64::MAX - 1);
    }
    assert_eq!(store.leader_count(), 3);
    for kind in [
        ResourceKind::WITNESS,
        ResourceKind::LEN,
        ResourceKind::from_i32(-1),
    ] {
        assert_eq!(store.resource_count(kind), 0);
        assert_eq!(store.resource_size(kind), 0);
        assert_eq!(store.resource_weight(kind), 0.0);
    }
}

#[test]
fn core_task_20_fixture_scores() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_20.json")).unwrap();
    for case in fixture["scores"].as_array().unwrap() {
        let mut store = StoreInfo::default();
        store.set_leader_count(case["count"].as_i64().unwrap());
        store.set_leader_size(case["size"].as_i64().unwrap());
        store.set_leader_weight(case["weight"].as_f64().unwrap());
        let score = store.leader_score(
            SchedulePolicy::from_i32(case["policy"].as_i64().unwrap() as i32),
            case["delta"].as_i64().unwrap(),
        );
        assert_eq!(score, case["expected"].as_f64().unwrap(), "{case}");
    }
}

#[test]
fn core_task_20_weight_edges() {
    let mut store = StoreInfo::default();
    for weight in [-2.0, 0.0, 1e-9, 2.0, f64::INFINITY, f64::NAN] {
        store.set_leader_weight(weight);
        store.set_region_weight(weight);
        let expected = if weight <= 0.0 { 1e-6 } else { weight };
        for kind in [ResourceKind::LEADER, ResourceKind::REGION] {
            let actual = store.resource_weight(kind);
            assert!(actual == expected || actual.is_nan() && expected.is_nan());
        }
        if weight.is_nan() {
            assert!(store.leader_score(SchedulePolicy::BY_COUNT, 1).is_nan());
        }
    }
    store.set_leader_count(i64::MAX);
    store.set_leader_size(i64::MIN);
    store.set_leader_weight(1.0);
    assert_eq!(
        store.leader_score(SchedulePolicy::BY_COUNT, 1),
        i64::MIN as f64
    );
    assert_eq!(
        store.leader_score(SchedulePolicy::BY_SIZE, -1),
        i64::MAX as f64
    );
}
