// Copyright 2026 AsterSQL.
use crate::{StoreResponseConfig, build_store_info_at};
use astersql_pd_core::{StoreInfo, StoreMeta, StoreState};
#[test]
fn core_task_67_state_names_and_thresholds() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_67.json")).unwrap();
    let heartbeat = fixture["heartbeat"].as_i64().unwrap();
    for c in fixture["cases"].as_array().unwrap() {
        let state = match c["state"].as_i64().unwrap() {
            0 => StoreState::Up,
            1 => StoreState::Offline,
            2 => StoreState::Tombstone,
            n => StoreState::Unknown(n as i32),
        };
        let store = StoreInfo::new(StoreMeta {
            state,
            last_heartbeat: heartbeat,
            ..Default::default()
        });
        let response = build_store_info_at(
            &StoreResponseConfig::default(),
            &store,
            heartbeat as i128 + c["elapsed"].as_i64().unwrap() as i128,
            c["maxdown"].as_i64().unwrap(),
        );
        let json = serde_json::to_value(response).unwrap();
        assert_eq!(json["store"]["state_name"], c["expected"], "{}", c["name"]);
        assert_eq!(store.state(), state);
    }
}
#[test]
fn core_task_67_caller_clock_progression() {
    let store = StoreInfo::new(StoreMeta::default());
    let config = StoreResponseConfig::default();
    for (now, expected) in [
        (0, "Up"),
        (20_000_000_001, "Disconnected"),
        (60_000_000_001, "Down"),
        (0, "Up"),
    ] {
        assert_eq!(
            build_store_info_at(&config, &store, now, 60_000_000_000)
                .store
                .unwrap()
                .state_name,
            expected
        );
    }
}
