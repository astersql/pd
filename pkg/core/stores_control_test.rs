// Copyright 2026 AsterSQL.
use crate::{StoreControlError, StoreInfo, StoreMeta, StoresInfo};
use astersql_pd_core_constant::Direction;
use astersql_pd_storelimit::Type;
use std::sync::Arc;
fn registry() -> StoresInfo {
    let r = StoresInfo::new();
    r.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 1,
        ..Default::default()
    })));
    r
}
#[test]
fn core_task_27_controls_and_snapshots() {
    let r = registry();
    let old = r.get_store(1).unwrap();
    for direction in [Direction::IN, Direction::OUT] {
        r.pause_leader_transfer(1, direction).unwrap();
        r.pause_leader_transfer(1, direction).unwrap();
        r.resume_leader_transfer(1, direction);
    }
    assert!(!r.get_store(1).unwrap().allow_leader_transfer_in());
    assert!(!r.get_store(1).unwrap().allow_leader_transfer_out());
    assert!(old.allow_leader_transfer_in());
    r.resume_leader_transfer(1, Direction::IN);
    assert!(r.get_store(1).unwrap().allow_leader_transfer_in());
    r.resume_leader_transfer(1, Direction::IN);
    assert!(!r.get_store(1).unwrap().allow_leader_transfer_in());
    r.slow_store_evicted(1).unwrap();
    r.slow_store_evicted(1).unwrap();
    r.slow_store_recovered(1);
    assert!(r.get_store(1).unwrap().evicted_as_slow_store());
    r.slow_store_recovered(1);
    assert!(!r.get_store(1).unwrap().evicted_as_slow_store());
    r.stopping_store_evicted(1).unwrap();
    assert!(r.get_store(1).unwrap().evicted_as_stopping_store());
    r.stopping_store_recovered(1);
    assert!(!r.get_store(1).unwrap().evicted_as_stopping_store());
    r.slow_trend_evicted(1).unwrap();
    assert_eq!(
        r.slow_trend_evicted(1),
        Err(StoreControlError::SlowTrendEvicted(1))
    );
    r.slow_trend_recovered(1);
    assert!(!r.get_store(1).unwrap().is_evicted_as_slow_trend());
    r.slow_trend_evicted(1).unwrap();
    assert!(!old.is_evicted_as_slow_trend());
}
#[test]
fn core_task_27_missing_and_limit() {
    let r = registry();
    assert_eq!(
        r.pause_leader_transfer(99, Direction::IN),
        Err(StoreControlError::StoreNotFound(99))
    );
    assert_eq!(
        r.slow_store_evicted(99),
        Err(StoreControlError::StoreNotFound(99))
    );
    assert_eq!(
        r.stopping_store_evicted(99),
        Err(StoreControlError::StoreNotFound(99))
    );
    assert_eq!(
        r.slow_trend_evicted(99),
        Err(StoreControlError::StoreNotFound(99))
    );
    r.resume_leader_transfer(99, Direction::OUT);
    r.slow_store_recovered(99);
    r.stopping_store_recovered(99);
    r.slow_trend_recovered(99);
    r.reset_store_limit(99, Type::ADD_PEER, &[]);
    r.trigger_network_slow_evict(99);
    r.reset_trigger_network_slow_evict(99);
    assert_eq!(r.store_count(), 1);
    let old = r.get_store(1).unwrap();
    r.reset_store_limit(1, Type::ADD_PEER, &[3.0, 9.0]);
    assert_eq!(
        old.limiter().unwrap().read().unwrap().rate(Type::ADD_PEER),
        3.0
    );
    r.reset_store_limit(1, Type::ADD_PEER, &[]);
    assert_eq!(
        old.limiter().unwrap().read().unwrap().rate(Type::ADD_PEER),
        0.0
    );
}
#[test]
fn core_task_27_network_current_membership() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_27.json")).unwrap();
    let r = registry();
    assert_eq!(r.avg_network_slow_score(99), 0);
    assert_eq!(r.avg_network_slow_score(1), 0);
    r.patch_store(1, |s| {
        s.stats.write().unwrap().raw.network_slow_scores = [(1, 999), (2, 8), (99, 100)].into();
    });
    for id in [2, 3] {
        r.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        })));
    }
    assert_eq!(
        r.avg_network_slow_score(1),
        f["average_with_missing_peer"].as_u64().unwrap()
    );
    r.delete_store(2);
    assert_eq!(r.avg_network_slow_score(1), 1);
    let old = r.get_store(1).unwrap();
    r.trigger_network_slow_evict(1);
    r.trigger_network_slow_evict(1);
    assert_eq!(r.get_store(1).unwrap().network_slow_triggers(), 2);
    assert_eq!(old.network_slow_triggers(), 0);
    assert!(Arc::ptr_eq(
        old.meta.as_ref().unwrap(),
        r.get_store(1).unwrap().meta.as_ref().unwrap()
    ));
    r.reset_trigger_network_slow_evict(1);
    assert_eq!(r.get_store(1).unwrap().network_slow_triggers(), 0);
}
