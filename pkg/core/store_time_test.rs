// Copyright 2026 AsterSQL.
use crate::{StoreInfo, StoreMeta};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    heartbeat: i64,
    now: String,
    persist: Option<String>,
    awaken: Option<String>,
    start: i64,
    down: i64,
    need_persist: bool,
    disconnected: bool,
    unhealthy: bool,
    uptime: i64,
    need_awaken: bool,
}

#[test]
fn core_task_17_time_contract() {
    let fixture: Fixture =
        serde_json::from_str(include_str!("rust-fixtures/core_task_17.json")).unwrap();
    assert_eq!(fixture.cases.len(), 20);
    for c in fixture.cases {
        let mut store = StoreInfo::new(StoreMeta {
            start_timestamp: c.start,
            ..Default::default()
        });
        store.set_last_heartbeat_unix_nanos(c.heartbeat);
        if let Some(t) = c.persist {
            store.set_last_persist_unix_nanos(t.parse().unwrap());
        }
        if let Some(t) = c.awaken {
            store.set_last_awaken_unix_nanos(t.parse().unwrap());
        }
        let now = c.now.parse().unwrap();
        assert_eq!(store.down_time_nanos(now), c.down, "{} downtime", c.name);
        assert_eq!(store.need_persist(), c.need_persist, "{} persist", c.name);
        assert_eq!(
            store.is_disconnected(now),
            c.disconnected,
            "{} disconnect",
            c.name
        );
        assert_eq!(store.is_unhealthy(now), c.unhealthy, "{} unhealthy", c.name);
        assert_eq!(store.uptime_nanos(), c.uptime, "{} uptime", c.name);
        assert_eq!(
            store.need_awaken_store(),
            c.need_awaken,
            "{} awaken",
            c.name
        );
    }
}

#[test]
fn core_task_17_clone_and_clock_progression() {
    let mut store = StoreInfo::new(StoreMeta::default());
    store.set_last_heartbeat_unix_nanos(1_000_000_000);
    store.set_last_persist_unix_nanos(0);
    store.set_last_awaken_unix_nanos(0);
    for mut copy in [store.clone(), store.shallow_clone()] {
        assert_eq!(copy.last_heartbeat_unix_nanos(), 1_000_000_000);
        assert!(!copy.need_persist());
        assert!(!copy.need_awaken_store());
        copy.set_last_heartbeat_unix_nanos(700_000_000_000);
        assert!(copy.need_persist());
        assert!(copy.need_awaken_store());
        copy.set_last_persist_unix_nanos(700_000_000_000);
        copy.set_last_awaken_unix_nanos(700_000_000_000);
        assert_eq!(store.last_heartbeat_unix_nanos(), 1_000_000_000);
        assert!(!copy.need_persist());
        assert!(!copy.need_awaken_store());
    }
    assert!(!store.is_disconnected(21_000_000_000));
    assert!(store.is_disconnected(21_000_000_001));
    assert_eq!(store.uptime_nanos(), 1_000_000_000);
    store.set_last_heartbeat_unix_nanos(700_000_000_000);
    assert!(store.need_persist());
    assert!(store.need_awaken_store());
    let empty = StoreInfo::default();
    assert_eq!(empty.last_heartbeat_unix_nanos(), 0);
    assert!(empty.need_persist());
    assert!(empty.need_awaken_store());
}
