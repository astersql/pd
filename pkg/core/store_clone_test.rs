// Copyright 2026 AsterSQL.
use super::{StoreInfo, StoreLabel, StoreMeta, StoreStats};

#[test]
fn core_task_16_metadata_boundaries() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_16.json")).unwrap();
    let store = StoreInfo::new(StoreMeta {
        labels: vec![StoreLabel {
            key: "zone".into(),
            value: fixture["label"].as_str().unwrap().into(),
        }],
        ..Default::default()
    });
    let mut deep = store.clone();
    let mut shallow = store.shallow_clone();
    assert!(!std::ptr::eq(store.meta().unwrap(), deep.meta().unwrap()));
    assert!(std::ptr::eq(store.meta().unwrap(), shallow.meta().unwrap()));
    deep.set_labels(vec![]);
    shallow.set_labels(vec![StoreLabel {
        key: "zone".into(),
        value: "changed".into(),
    }]);
    assert_eq!(store.label_value("zone"), "z1");
    assert!(deep.labels().is_empty());
    assert_eq!(shallow.label_value("zone"), "changed");
    assert!(!std::ptr::eq(
        store.meta().unwrap(),
        shallow.meta().unwrap()
    ));
}

#[test]
fn core_task_16_shared_stats_and_smoothing() {
    let mut store = StoreInfo::new(None);
    store.update_raw_stats(StoreStats {
        available: 100,
        ..Default::default()
    });
    let mut deep = store.clone();
    let mut shallow = store.shallow_clone();
    deep.update_raw_stats(StoreStats {
        available: 200,
        capacity: 300,
        ..Default::default()
    });
    assert_eq!(store.available(), 200);
    assert_eq!(shallow.capacity(), 300);
    assert_eq!(store.avg_available(), deep.avg_available());
    shallow.update_raw_stats(StoreStats::default());
    assert_eq!(store.available(), 0);
    assert_eq!(deep.capacity(), 0);
    assert_eq!(store.avg_available(), shallow.avg_available());
    assert_eq!(store.leader_weight(), deep.leader_weight());
    assert_eq!(store.region_weight(), shallow.region_weight());
}

#[test]
fn core_task_16_shared_limiter() {
    use astersql_pd_storelimit::Type;
    let store = StoreInfo::new(None);
    let deep = store.clone();
    let shallow = store.shallow_clone();
    deep.limiter()
        .unwrap()
        .write()
        .unwrap()
        .reset(7.0, Type::ADD_PEER);
    assert_eq!(
        store
            .limiter()
            .unwrap()
            .read()
            .unwrap()
            .rate(Type::ADD_PEER),
        7.0
    );
    assert_eq!(
        shallow
            .limiter()
            .unwrap()
            .read()
            .unwrap()
            .rate(Type::ADD_PEER),
        7.0
    );
}

#[test]
fn core_task_16_zero_value() {
    let store = StoreInfo::default();
    for mut copy in [store.clone(), store.shallow_clone()] {
        assert!(copy.meta().is_none());
        assert!(copy.limiter().is_none());
        assert_eq!(copy.leader_weight(), 0.0);
        copy.update_raw_stats(StoreStats {
            available: 42,
            ..Default::default()
        });
        assert_eq!(store.avg_available(), 42);
    }
}

#[test]
fn core_task_16_concurrent_stats() {
    let store = StoreInfo::new(None);
    let mut copy = store.clone();
    std::thread::scope(|scope| {
        scope.spawn(move || {
            for _ in 0..100 {
                copy.update_raw_stats(StoreStats {
                    available: 123,
                    ..Default::default()
                });
            }
        });
        for _ in 0..100 {
            let snapshot = store.store_stats();
            assert!(snapshot.available == 0 || snapshot.available == 123);
        }
    });
    assert_eq!(store.available(), 123);
    assert_eq!(store.avg_available(), 123);
}
