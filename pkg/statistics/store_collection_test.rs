// Copyright 2026 AsterSQL.
use crate::{StoreMetricSample, StoreStatistics, StoreStatisticsMap};
use astersql_pd_core::{NodeState, SlowTrend, StoreInfo, StoreLabel, StoreMeta, StoreStats};
use astersql_pd_schedule_config::{ProviderSnapshot, StoreLimitConfig};
use serde_json::Value;
use std::collections::BTreeMap;

const SECOND: i128 = 1_000_000_000;
const NOW: i128 = 10_000 * SECOND;
fn config() -> ProviderSnapshot<()> {
    let mut c = ProviderSnapshot::default();
    c.replication.location_labels = ["zone", "host"].into();
    c.schedule.max_store_down_time = "30m".parse().unwrap();
    c.schedule.low_space_ratio = 0.8;
    c.schedule.high_space_ratio = 0.7;
    c.schedule.region_score_formula_version = "v2".into();
    c.schedule.leader_schedule_policy = "count".into();
    c
}
fn store(id: u64, state: NodeState, elapsed: i128) -> StoreInfo {
    StoreInfo::new(StoreMeta {
        id,
        node_state: state,
        last_heartbeat: (NOW - elapsed) as i64,
        address: format!("mock://tikv-{id}:{id}"),
        ..Default::default()
    })
}
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../tests/rust-compat/fixtures/state/task-27.json"
    ))
    .unwrap()
}
fn samples(values: &[StoreMetricSample], family: &str) -> BTreeMap<Vec<String>, f64> {
    values
        .iter()
        .filter(|s| s.family == family)
        .map(|s| (s.labels.clone(), s.value))
        .collect()
}
fn expected(f: &Value, field: &str) -> BTreeMap<Vec<String>, f64> {
    f[field]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            (
                v["labels"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|s| s.as_str().unwrap().into())
                    .collect(),
                v["value"].as_f64().unwrap(),
            )
        })
        .collect()
}
fn go_stores() -> Vec<StoreInfo> {
    (0..10)
        .map(|i| {
            let id = if i == 8 { 8 } else { i + 1 };
            let state = match i {
                3 => NodeState::Removing,
                8 => NodeState::Removed,
                _ => NodeState::Preparing,
            };
            let mut s = store(id, state, if i == 4 { 3600 * SECOND } else { 0 });
            let mut labels = Vec::new();
            if i < 6 {
                labels.push(StoreLabel {
                    key: "zone".into(),
                    value: format!("z{}", i / 2 + 1),
                });
            }
            labels.push(StoreLabel {
                key: "host".into(),
                value: format!("h{}", if i < 8 { i % 2 + 1 } else { i - 5 }),
            });
            if i == 9 {
                labels.push(StoreLabel {
                    key: "zone".into(),
                    value: "z4".into(),
                });
                labels.push(StoreLabel {
                    key: "engine".into(),
                    value: "tiflash".into(),
                });
            }
            s.set_labels(labels);
            if i == 5 {
                s.update_raw_stats(StoreStats {
                    capacity: 512 * 1024 * 1024,
                    available: 100 * 1024 * 1024,
                    ..Default::default()
                });
            }
            if i == 8 {
                s.set_leader_count(100);
            }
            s
        })
        .collect()
}
#[test]
fn state_task_27_store_collection_go_snapshots() {
    let c = config();
    let f = fixture();
    let mut m = StoreStatisticsMap::new(&c);
    for (i, s) in go_stores().iter().enumerate() {
        let status = StoreStatistics::observe_store_status(s, &c, NOW);
        let want: BTreeMap<String, f64> =
            serde_json::from_value(f["status"][format!("STORE {i}")].clone()).unwrap();
        assert_eq!(status, want, "Go STORE {i}");
        m.observe(s, NOW);
    }
    let got = m.collect();
    assert_eq!(samples(&got, "pd_cluster_status"), expected(&f, "cluster"));
    assert_eq!(
        samples(&got, "pd_cluster_placement_status"),
        expected(&f, "placement")
    );
    assert_eq!(m.stats.label_counter["host:h1"], vec![1, 3, 5, 7]);
    assert_eq!(m.stats.label_counter["zone:unknown"], vec![7, 8]);
}
#[test]
fn state_task_27_store_collection_go_health_priority() {
    let c = config();
    let f = fixture();
    let mut s = go_stores().remove(5);
    s.update_raw_stats(StoreStats {
        slow_score: 80,
        ..s.store_stats()
    });
    for (stage, elapsed) in [
        ("SLOW", 0),
        ("DISCONNECTED", 30),
        ("UNHEALTHY", 630),
        ("DOWN", 4230),
    ] {
        s.set_last_heartbeat_unix_nanos((NOW - elapsed * SECOND) as i64);
        let want: BTreeMap<String, f64> =
            serde_json::from_value(f["status"][stage].clone()).unwrap();
        assert_eq!(
            StoreStatistics::observe_store_status(&s, &c, NOW),
            want,
            "{stage}"
        );
    }
    for (stage, state) in [
        ("SERVING", NodeState::Serving),
        ("REMOVED", NodeState::Removed),
        ("UNKNOWN", NodeState::Unknown(99)),
    ] {
        let s = store(1, state, 0);
        let want: BTreeMap<String, f64> =
            serde_json::from_value(f["status"][stage].clone()).unwrap();
        assert_eq!(
            StoreStatistics::observe_store_status(&s, &c, NOW),
            want,
            "{stage}"
        );
    }
}
#[test]
fn state_task_27_store_collection_exact_clock_boundaries() {
    let c = config();
    for (elapsed, active) in [
        (-1, "up"),
        (20 * SECOND, "up"),
        (20 * SECOND + 1, "disconnected"),
        (600 * SECOND, "disconnected"),
        (600 * SECOND + 1, "unhealth"),
        (1800 * SECOND - 1, "unhealth"),
        (1800 * SECOND, "down"),
    ] {
        let s = store(1, NodeState::Serving, elapsed);
        let got = StoreStatistics::observe_store_status(&s, &c, NOW);
        assert_eq!(
            got[&format!("store_{active}_count")],
            1.0,
            "elapsed {elapsed}"
        );
        assert_eq!(got["store_serving_count"], 1.0);
        assert_eq!(got.values().sum::<f64>(), 2.0);
    }
    let mut c = c;
    c.schedule.max_store_down_time = "20s".parse().unwrap();
    assert_eq!(
        StoreStatistics::observe_store_status(
            &store(1, NodeState::Preparing, 20 * SECOND),
            &c,
            NOW
        )["store_down_count"],
        1.0
    );
}
#[test]
fn state_task_27_store_collection_low_space_and_removed_retention() {
    let c = config();
    let mut m = StoreStatisticsMap::new(&c);
    let mut s = store(42, NodeState::Serving, 0);
    s.set_new_store_stats(StoreStats {
        capacity: 1000,
        available: 0,
        used_size: 333,
        ..Default::default()
    });
    s.set_region_count(12);
    s.set_leader_count(8);
    s.set_witness_count(2);
    s.set_learner_count(1);
    m.observe(&s, NOW);
    let before = samples(&m.collect(), "pd_cluster_status");
    assert_eq!(
        before[&vec!["storage_size".into(), "tikv".into(), "42".into()]],
        333.0
    );
    for state in [
        NodeState::Serving,
        NodeState::Removing,
        NodeState::Removed,
        NodeState::Unknown(55),
    ] {
        s.edit_meta(|m| m.node_state = state);
        let status = StoreStatistics::observe_store_status(&s, &c, NOW);
        assert_eq!(
            status["store_low_space_count"],
            if state == NodeState::Removed {
                0.0
            } else {
                1.0
            }
        );
    }
    s.edit_meta(|m| m.node_state = NodeState::Removed);
    s.set_new_store_stats(StoreStats::default());
    s.set_leader_count(100);
    m.observe(&s, NOW);
    let after = samples(&m.collect(), "pd_cluster_status");
    assert_eq!(
        after[&vec!["leader_count".into(), "tikv".into(), "42".into()]],
        8.0
    );
    assert_eq!(
        after[&vec!["storage_capacity".into(), "tikv".into(), "42".into()]],
        1000.0
    );
    assert_eq!(
        after[&vec!["store_removed_count".into(), "tikv".into(), "42".into()]],
        1.0
    );
    let mut fresh = StoreStatisticsMap::new(&c);
    fresh.observe(&s, NOW);
    assert_eq!(samples(&fresh.collect(), "pd_cluster_status").len(), 12);
    assert!(fresh.stats.label_counter.is_empty());
}
#[test]
fn state_task_27_store_collection_group_config_isolation() {
    let mut c = config();
    c.schedule.leader_schedule_limit = 11;
    c.schedule.enable_make_up_replica = true;
    c.schedule.store_limit = Some(BTreeMap::from([(
        7,
        StoreLimitConfig {
            add_peer: 2.0,
            remove_peer: 3.0,
            transfer_leader_in: 4.0,
        },
    )]));
    let mut d = config();
    d.replication.location_labels = ["rack"].into();
    d.schedule.leader_schedule_limit = 22;
    let mut a = StoreStatisticsMap::new(&c);
    let mut b = StoreStatisticsMap::new(&d);
    let mut s = store(7, NodeState::Serving, 0);
    a.observe(&s, NOW);
    a.observe(&s, NOW);
    s.edit_meta(|m| {
        m.labels = vec![StoreLabel {
            key: "rack".into(),
            value: "r2".into(),
        }]
    });
    b.observe(&s, NOW);
    assert_eq!(a.stats.label_counter["zone:unknown"], vec![7, 7]);
    assert_eq!(b.stats.label_counter.len(), 1);
    assert_eq!(b.stats.label_counter["rack:r2"], vec![7]);
    let av = a.collect();
    let bv = b.collect();
    assert_eq!(
        samples(&av, "pd_config_status")[&vec!["leader-schedule-limit".into()]],
        11.0
    );
    assert_eq!(
        samples(&bv, "pd_config_status")[&vec!["leader-schedule-limit".into()]],
        22.0
    );
    assert_eq!(
        samples(&av, "pd_config_status")[&vec!["enable-makeup-replica".into()]],
        1.0
    );
    assert_eq!(
        samples(&av, "pd_cluster_store_limit")[&vec!["7".into(), "transfer-leader-in".into()]],
        4.0
    );
    assert!(samples(&bv, "pd_cluster_store_limit").is_empty());
    let mut detached = av.clone();
    detached.clear();
    assert_eq!(a.collect(), av);
    assert_eq!(
        a.collect()
            .iter()
            .filter(|s| s.family == "pd_cluster_placement_status")
            .count(),
        2
    );
}
#[test]
fn state_task_27_store_collection_resource_samples() {
    let c = config();
    let mut m = StoreStatisticsMap::new(&c);
    let mut s = store(u64::MAX, NodeState::Serving, 0);
    s.set_region_count(8);
    s.set_region_size(32);
    s.set_leader_count(3);
    s.set_leader_size(6);
    s.set_new_store_stats(StoreStats {
        capacity: 1000,
        available: 800,
        used_size: 200,
        slow_trend: Some(SlowTrend {
            cause_value: 1.0,
            cause_rate: 2.0,
            result_value: 3.0,
            result_rate: 4.0,
        }),
        ..Default::default()
    });
    m.observe(&s, NOW);
    let got = samples(&m.collect(), "pd_scheduler_store_status");
    let v = |typ: &str| got[&vec![s.address().into(), s.id().to_string(), typ.into()]];
    assert_eq!(
        v("leader_score"),
        s.leader_score(astersql_pd_core_constant::SchedulePolicy::BY_COUNT, 0)
    );
    assert_eq!(v("region_score"), s.region_score("v2", 0.7, 0.8, 0));
    assert_eq!(v("store_used"), 200.0);
    assert_eq!(v("store_available"), 800.0);
    assert_eq!(v("store_slow_trend_result_rate"), 4.0);
}

#[test]
fn state_task_27_store_collection_space_predicate_and_windows() {
    use astersql_pd_core::StoreLimiter;
    let c = config();
    let mut s = store(1, NodeState::Serving, 0);
    for (capacity, available, regions, low) in [
        (0, 0, 100, false),
        (1000, 200, 100, false),
        (1000, 199, 100, true),
        (100 << 30, 9 << 30, 99, false),
        (100 << 30, 9 << 30, 100, true),
        (100 << 30, 8 << 30, 99, true),
    ] {
        s.set_region_count(regions);
        s.set_new_store_stats(StoreStats {
            capacity,
            available,
            ..Default::default()
        });
        assert_eq!(
            StoreStatistics::observe_store_status(&s, &c, NOW)["store_low_space_count"],
            f64::from(low)
        );
    }
    let mut m = StoreStatisticsMap::new(&c);
    m.observe(&s, NOW);
    assert!(
        !samples(&m.collect(), "pd_scheduler_store_status")
            .keys()
            .any(|k| k[2].starts_with("windows_"))
    );
    s.set_store_limit(Some(StoreLimiter::V2(Default::default())));
    m.observe(&s, NOW);
    let values = samples(&m.collect(), "pd_scheduler_store_status");
    assert_eq!(
        values[&vec![s.address().into(), "1".into(), "windows_size".into()]],
        100.0
    );
    for priority in ["low", "medium", "high", "urgent"] {
        assert_eq!(
            values[&vec![
                s.address().into(),
                "1".into(),
                format!("windows_used_level_{priority}")
            ]],
            0.0
        );
    }
    s.set_store_limit(None);
    let mut fresh = StoreStatisticsMap::new(&c);
    fresh.observe(&s, NOW);
    assert!(
        !samples(&fresh.collect(), "pd_scheduler_store_status")
            .keys()
            .any(|k| k[2].starts_with("windows_"))
    );
}
