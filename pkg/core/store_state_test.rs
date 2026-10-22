// Copyright 2026 AsterSQL.
use crate::{NodeState, StoreInfo, StoreMeta, StoreState};

#[test]
fn core_task_12_state_matrix() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_12.json")).unwrap();
    for row in fixture["scenarios"].as_array().unwrap() {
        for destroyed in [false, true] {
            for state in [
                StoreState::Up,
                StoreState::Offline,
                StoreState::Tombstone,
                StoreState::Unknown(99),
            ] {
                let node_state = match row["node"].as_str().unwrap() {
                    "preparing" => NodeState::Preparing,
                    "serving" => NodeState::Serving,
                    "removing" => NodeState::Removing,
                    "removed" => NodeState::Removed,
                    _ => NodeState::Unknown(99),
                };
                let store = StoreInfo::new(StoreMeta {
                    state,
                    node_state,
                    physically_destroyed: destroyed,
                    ..Default::default()
                });
                let actual = [
                    store.is_up(),
                    store.is_preparing(),
                    store.is_serving(),
                    store.is_removing(),
                    store.is_removed(),
                ];
                let expected: Vec<bool> = row["expected"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_bool().unwrap())
                    .collect();
                assert_eq!(actual.as_slice(), expected);
                assert_eq!(store.is_physically_destroyed(), destroyed);
                assert_eq!(store.state(), state);
                assert_eq!(store.node_state(), node_state);
            }
        }
    }
}

#[test]
fn core_task_12_transitions() {
    let mut store = StoreInfo::new(StoreMeta::default());
    assert!(store.is_preparing());
    store.set_store_state(StoreState::Offline, Some(true));
    assert!(store.is_removing());
    assert!(store.is_physically_destroyed());
    store.set_store_state(StoreState::Up, Some(false));
    assert!(store.is_serving());
    assert!(store.is_physically_destroyed());
    store.set_store_state(StoreState::Tombstone, None);
    assert!(store.is_removed());
    assert!(store.is_physically_destroyed());
    store.set_store_state(StoreState::Unknown(99), Some(false));
    assert_eq!(store.state(), StoreState::Tombstone);
    assert!(store.is_removed());
    store.set_store_state(StoreState::Offline, Some(false));
    assert!(store.is_removing());
    assert!(!store.is_physically_destroyed());
}

#[test]
#[should_panic(expected = "physicallyDestroyed should be set")]
fn core_task_12_offline_requires_flag() {
    StoreInfo::new(StoreMeta::default()).set_store_state(StoreState::Offline, None);
}

#[test]
fn core_task_12_absent_metadata_defaults() {
    let store = StoreInfo::new(None);
    assert_eq!(store.state(), StoreState::Up);
    assert_eq!(store.node_state(), NodeState::Preparing);
    assert!(store.is_up());
    assert!(!store.is_physically_destroyed());
}
