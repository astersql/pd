// Copyright 2026 AsterSQL.
use crate::store::*;
use astersql_pd_core::{StoreInfo as CoreStore, StoreMeta, StoreStats};
use serde_json::{Value, json};
fn encode(store: &CoreStore) -> Value {
    serde_json::to_value(build_store_info(&StoreResponseConfig::default(), store)).unwrap()
}
#[test]
fn core_task_66_defaults_and_empty_list() {
    let v = encode(&CoreStore::new(StoreMeta::default()));
    let fixture: Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_66.json")).unwrap();
    assert_eq!(v["status"], fixture["default_status"]);
    assert_eq!(v["store"], json!({"state_name":"Up"}));
    assert_eq!(
        serde_json::to_value(StoresInfo::default()).unwrap(),
        json!({"count":0,"stores":null})
    );
    assert_eq!(
        serde_json::to_value(StoresInfo {
            count: 0,
            stores: Some(vec![])
        })
        .unwrap(),
        json!({"count":0,"stores":[]})
    );
}
#[test]
fn core_task_66_full_status() {
    let mut s = CoreStore::new(StoreMeta {
        id: u64::MAX,
        start_timestamp: 1,
        last_heartbeat: 3_661_000_000_001,
        ..Default::default()
    });
    s.set_leader_count(6);
    s.set_region_count(9);
    s.set_leader_size(12);
    s.set_region_size(18);
    s.set_leader_weight(2.0);
    s.set_region_weight(3.0);
    s.set_learner_count(2);
    s.set_witness_count(3);
    s.set_pending_peer_count(4);
    s.update_raw_stats(StoreStats {
        capacity: 1024,
        available: 512,
        used_size: 256,
        slow_score: 80,
        network_slow_scores: [(u64::MAX, 7)].into(),
        is_busy: true,
        sending_snap_count: 2,
        receiving_snap_count: 3,
        slow_trend: Some(astersql_pd_core::SlowTrend {
            cause_value: 1.0,
            cause_rate: -2.0,
            result_value: 3.0,
            result_rate: 4.0,
        }),
        ..Default::default()
    });
    let v = encode(&s);
    assert_eq!(v["store"]["id"], json!(u64::MAX));
    let st = &v["status"];
    for (key, expected) in [
        ("capacity", json!("1KiB")),
        ("available", json!("512B")),
        ("used_size", json!("256B")),
        ("leader_count", json!(6)),
        ("leader_score", json!(3.0)),
        ("region_count", json!(9)),
        ("region_size", json!(18)),
        ("learner_count", json!(2)),
        ("witness_count", json!(3)),
        ("pending_peer_count", json!(4)),
        ("slow_score", json!(80)),
        ("sending_snap_count", json!(2)),
        ("receiving_snap_count", json!(3)),
        ("is_busy", json!(true)),
        ("uptime", json!("1h1m0.000000001s")),
        ("start_ts", json!("1970-01-01T00:00:01Z")),
        ("last_heartbeat_ts", json!("1970-01-01T01:01:01.000000001Z")),
    ] {
        assert_eq!(st[key], expected, "{key}");
    }
    assert_eq!(st["network_slow_score"][u64::MAX.to_string()], json!(7));
    assert_eq!(
        st["slow_trend"],
        json!({"cause_value":1.0,"cause_rate":-2.0,"result_value":3.0,"result_rate":4.0})
    );
    assert_eq!(st["region_score"], json!(s.region_score("v2", 0.7, 0.8, 0)));
    assert!(
        !st.as_object()
            .unwrap()
            .contains_key("pause_leader_transfer_in")
    );
}
#[test]
fn core_task_66_time_boundaries_and_presence() {
    let s = CoreStore::new(StoreMeta {
        start_timestamp: -1,
        last_heartbeat: -1,
        ..Default::default()
    });
    let v = encode(&s);
    assert_eq!(v["status"]["start_ts"], json!("1969-12-31T23:59:59Z"));
    assert_eq!(
        v["status"]["last_heartbeat_ts"],
        json!("1969-12-31T23:59:59.999999999Z")
    );
    assert_eq!(v["status"]["uptime"], json!("999.999999ms"));
    let s = CoreStore::new(StoreMeta {
        start_timestamp: 2,
        last_heartbeat: 1,
        ..Default::default()
    });
    assert!(encode(&s)["status"].get("uptime").is_none());
    let list = StoresInfo {
        count: 2,
        stores: Some(vec![
            None,
            Some(build_store_info(&StoreResponseConfig::default(), &s)),
        ]),
    };
    let v = serde_json::to_value(list).unwrap();
    assert!(v["stores"][0].is_null());
    assert!(v["stores"][1].is_object());
}

#[test]
fn core_task_66_metadata_pause_and_invalid_encoding() {
    use astersql_pd_core::{NodeState, StoreLabel, StoreState, TxnProtocolVersionRange};
    use astersql_pd_core_constant::Direction;
    let mut s = CoreStore::new(StoreMeta {
        id: 7,
        address: "a".into(),
        peer_address: "p".into(),
        status_address: "s".into(),
        version: "v".into(),
        git_hash: "g".into(),
        deploy_path: "d".into(),
        physically_destroyed: true,
        state: StoreState::Offline,
        node_state: NodeState::Removing,
        txn_protocol_version_range: Some(TxnProtocolVersionRange::default()),
        labels: vec![
            StoreLabel::default(),
            StoreLabel {
                key: "zone".into(),
                value: "z1".into(),
            },
        ],
        ..Default::default()
    });
    s.pause_leader_transfer(Direction::IN);
    s.pause_leader_transfer(Direction::OUT);
    let v = encode(&s);
    assert_eq!(
        v["store"],
        json!({"id":7,"address":"a","peer_address":"p","status_address":"s","version":"v","git_hash":"g","deploy_path":"d","physically_destroyed":true,"state":1,"node_state":2,"state_name":"Offline","txn_protocol_version_range":{},"labels":[{},{"key":"zone","value":"z1"}]})
    );
    assert_eq!(v["status"]["pause_leader_transfer_in"], true);
    assert_eq!(v["status"]["pause_leader_transfer_out"], true);
    s.set_leader_weight(f64::NAN);
    assert!(serde_json::to_vec(&build_store_info(&StoreResponseConfig::default(), &s)).is_err());
    assert!(serde_json::to_vec(&StoreTimestamp(i64::MAX as i128 * 1_000_000_000)).is_err());
    assert_eq!(
        serde_json::to_value(StoreInfo::default()).unwrap(),
        json!({"store":null,"status":null})
    );
}
#[test]
fn core_task_66_config_and_calendar_boundaries() {
    let mut s = CoreStore::new(StoreMeta {
        start_timestamp: 951_782_400,
        ..Default::default()
    });
    s.set_leader_count(6);
    s.set_leader_size(12);
    s.set_leader_weight(2.0);
    let mut opt = StoreResponseConfig::default();
    opt.leader_schedule_policy = "size".into();
    let v = serde_json::to_value(build_store_info(&opt, &s)).unwrap();
    assert_eq!(v["status"]["leader_score"], 6.0);
    assert_eq!(v["status"]["start_ts"], "2000-02-29T00:00:00Z");
    opt.leader_schedule_policy = "unknown".into();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| build_store_info(&opt, &s)))
            .is_err()
    );
}
