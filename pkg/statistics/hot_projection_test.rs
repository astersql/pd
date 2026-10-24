// Copyright 2026 AsterSQL.
use crate::*;
use astersql_pd_core::{StoreInfo, StoreMeta};
use astersql_pd_core_constant::ResourceKind;
use astersql_pd_statistics_utils::{RWType, StoreLoadKind as S};
use serde_json::{Value, json};
use std::{collections::HashMap, time::Duration};

fn stores() -> Vec<StoreInfo> {
    (1..=3)
        .map(|id| {
            StoreInfo::new(StoreMeta {
                id,
                ..Default::default()
            })
        })
        .collect()
}
fn peer(store_id: u64, region_id: u64, degree: i64, leader: bool, loads: Vec<f64>) -> HotPeerStat {
    HotPeerStat {
        store_id,
        region_id,
        hot_degree: degree,
        anti_count: 2,
        is_leader: leader,
        loads,
        ..Default::default()
    }
}
fn peers() -> HashMap<u64, Vec<HotPeerStat>> {
    HashMap::from([
        (
            1,
            vec![
                peer(1, 11, 10, true, vec![11.4, 12.6, -1.5, 3.5]),
                peer(1, 12, 0, false, vec![10., 20., 30., 0.]),
                peer(1, 13, -1, false, vec![10., 20., 30., 0.]),
            ],
        ),
        (2, vec![peer(2, 21, 10, false, vec![10., 20., 30., 0.])]),
        (3, vec![]),
        (99, vec![peer(99, 991, 10, true, vec![1000.; 4])]),
    ])
}
fn fixture(name: &str) -> Value {
    let f: Value = serde_json::from_str(include_str!(
        "../../tests/rust-compat/fixtures/state/task-25.json"
    ))
    .unwrap();
    f["snapshots"][name].clone()
}
// JSON numeric tokens may be integer in Go and floating point in serde. All
// fixture magnitudes are exactly representable; IDs above 2^53 have a separate test.
fn equal_json(actual: &Value, expected: &Value) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(a.as_f64(), b.as_f64()),
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
            for (key, v) in a {
                equal_json(v, &b[key]);
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                equal_json(a, b);
            }
        }
        _ => assert_eq!(actual, expected),
    }
}
#[test]
fn state_task_25_hot_projection_collect_oracle() {
    let result = collect_hot_peer_infos(&stores(), &peers());
    equal_json(&serde_json::to_value(result).unwrap(), &fixture("collect"));
}
#[test]
fn state_task_25_hot_projection_status_oracle_and_learner() {
    let loads = (1..=3).map(|id| (id, [0.; 12])).collect();
    let mut result = get_hot_status(&stores(), &loads, &peers(), RWType::WRITE, false);
    equal_json(&serde_json::to_value(&result).unwrap(), &fixture("status"));
    // The Go coordinator enriches learner metadata after the statistics query.
    result.as_peer.get_mut(&2).unwrap().stats.as_mut().unwrap()[0].is_learner = true;
    equal_json(&serde_json::to_value(result).unwrap(), &fixture("learner"));
}
#[test]
fn state_task_25_hot_projection_empty_and_missing() {
    let empty = json!({"as_peer":{},"as_leader":{}});
    assert_eq!(
        serde_json::to_value(collect_hot_peer_infos(&[], &HashMap::new())).unwrap(),
        empty
    );
    assert_eq!(
        serde_json::to_value(get_hot_status(
            &[],
            &HashMap::new(),
            &HashMap::new(),
            RWType::READ,
            false
        ))
        .unwrap(),
        empty
    );
    assert!(
        collect_hot_peer_infos(&stores(), &HashMap::new())
            .as_peer
            .is_empty()
    );
    assert!(
        get_hot_status(&stores(), &HashMap::new(), &peers(), RWType::READ, false)
            .as_peer
            .is_empty()
    );
    let result = get_hot_status(
        &stores(),
        &HashMap::from([(1, [0.; 12])]),
        &HashMap::new(),
        RWType::READ,
        false,
    );
    assert_eq!(result.as_peer[&1].stats.as_ref().unwrap().len(), 0);
    assert_eq!(result.as_peer[&1].count, 0);
}
#[test]
fn state_task_25_hot_projection_detail_current_rates_and_owned_values() {
    let mut loads = [0.; 12];
    for (key, v) in [
        (S::READ_BYTES, 100.25),
        (S::READ_KEYS, 10.5),
        (S::READ_QUERY, 5.75),
        (S::READ_CPU, 0.25),
    ] {
        loads[key.as_i32() as usize] = v;
    }
    let mut input = peers();
    input.get_mut(&1).unwrap()[0].stores = vec![1, 2, 3];
    let mut details = summary_stores_load(
        &summary_store_infos(&stores()),
        &HashMap::from([(1, loads)]),
        None,
        &input,
        false,
        RWType::READ,
        ResourceKind::REGION,
        Duration::ZERO,
    );
    let mut d = details.remove(&1).unwrap();
    d.load_pred.future.loads = [999.; 4];
    let result = d.to_hot_peers_stat();
    assert_eq!(
        [
            result.store_byte_rate,
            result.store_key_rate,
            result.store_query_rate,
            result.store_cpu_rate
        ],
        [100.25, 10.5, 5.75, 0.25]
    );
    assert_eq!(result.count, 1);
    assert_eq!(
        [
            result.total_bytes_rate,
            result.total_keys_rate,
            result.total_query_rate,
            result.total_cpu_rate
        ],
        [11., 13., -2., 4.]
    );
    d.hot_peers[0].stores.clear();
    let show = &result.stats.as_ref().unwrap()[0];
    assert_eq!(show.stores, Some(vec![1, 2, 3]));
    assert_eq!(show.anti_count, 2);
    assert!(!show.is_learner);
    assert_eq!(show.last_update_time, "0001-01-01T00:00:00Z");
}
#[test]
fn state_task_25_hot_projection_ids_and_cold_only() {
    let mut p = peers();
    let raw = p.get_mut(&1).unwrap();
    raw[0].hot_degree = 0;
    raw[0].region_id = u64::MAX;
    let mut result = get_hot_status(
        &stores(),
        &HashMap::from([(1, [0.; 12])]),
        &p,
        RWType::READ,
        false,
    );
    assert_eq!(result.as_peer[&1].count, 0);
    assert_eq!(result.as_peer[&1].total_bytes_rate, 0.);
    p.get_mut(&1).unwrap()[0].hot_degree = 1;
    result = get_hot_status(
        &stores(),
        &HashMap::from([(1, [0.; 12])]),
        &p,
        RWType::READ,
        false,
    );
    assert_eq!(
        serde_json::to_value(result).unwrap()["as_peer"]["1"]["statistics"][0]["region_id"]
            .as_u64(),
        Some(u64::MAX)
    );
}
