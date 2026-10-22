// Copyright 2026 AsterSQL.
use crate::{StoreInfo, StoreMeta};
use serde::Deserialize;

#[derive(Deserialize)]
struct Scenario {
    name: String,
    id: u64,
    address: String,
    status_address: String,
    version: String,
    deploy_path: String,
    start_timestamp: i64,
}

#[test]
fn core_task_11_metadata_roundtrip() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_11.json")).unwrap();
    let scenarios: Vec<Scenario> = serde_json::from_value(fixture["scenarios"].clone()).unwrap();
    assert_eq!(scenarios.len(), 3);
    for s in scenarios {
        let meta = StoreMeta {
            id: s.id,
            address: s.address.clone(),
            status_address: s.status_address.clone(),
            version: s.version.clone(),
            deploy_path: s.deploy_path.clone(),
            start_timestamp: s.start_timestamp,
            ..Default::default()
        };
        let store = StoreInfo::new(meta.clone());
        assert_eq!(store.meta(), Some(&meta), "{}", s.name);
        assert_eq!(store.id(), s.id);
        assert_eq!(store.address(), s.address);
        assert_eq!(store.status_address(), s.status_address);
        assert_eq!(store.version(), s.version);
        assert_eq!(store.deploy_path(), s.deploy_path);
        assert_eq!(store.start_time_unix_seconds(), s.start_timestamp);
        assert_eq!(store.leader_weight(), 1.0);
        assert_eq!(store.region_weight(), 1.0);
    }
}

#[test]
fn core_task_11_absent_metadata_and_zero_value() {
    let absent = StoreInfo::new(None);
    assert!(absent.meta().is_none());
    assert_eq!(absent.id(), 0);
    assert_eq!(absent.address(), "");
    assert_eq!(absent.status_address(), "");
    assert_eq!(absent.version(), "");
    assert_eq!(absent.deploy_path(), "");
    assert_eq!(absent.start_time_unix_seconds(), 0);
    assert_eq!(absent.leader_weight(), 1.0);
    assert_eq!(absent.region_weight(), 1.0);
    let zero = StoreInfo::default();
    assert!(zero.meta().is_none());
    assert_eq!(zero.leader_weight(), 0.0);
    assert_eq!(zero.region_weight(), 0.0);
    assert_eq!(StoreMeta::default().start_timestamp, 0);
}
