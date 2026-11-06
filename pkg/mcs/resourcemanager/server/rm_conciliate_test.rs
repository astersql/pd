// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_storage_domain::MemoryStore;
use std::sync::Arc;
fn configured(name: &str, priority: u32, fill: u64) -> Group {
    let mut group = Group::reserved_default();
    group.name = name.into();
    group.priority = priority;
    let bucket = group.ru.as_mut().unwrap();
    bucket.tokens = 0.0;
    bucket.settings.as_mut().unwrap().fill_rate = fill;
    bucket.settings.as_mut().unwrap().burst_limit = fill as i64;
    group
}
async fn burst(manager: &mut Manager, name: &str, now: i64) -> i64 {
    manager
        .request_ru(u32::MAX, name, now, 1000.0, 5000, 7)
        .await
        .unwrap()
        .0
        .settings
        .unwrap()
        .burst_limit
}
#[tokio::test]
async fn mcs_097_priorities_reverse_without_mutating_configuration() {
    let mut manager = Manager::open(Arc::new(MemoryStore::default()), WriteRole::LegacyAll)
        .await
        .unwrap();
    manager
        .add(u32::MAX, configured("high", 16, 100))
        .await
        .unwrap();
    manager
        .add(u32::MAX, configured("low", 1, 100))
        .await
        .unwrap();
    manager.set_limit_from_storage(u32::MAX, 150.0, 0);
    for now in [0, 1_000_000_000] {
        for name in ["high", "low"] {
            manager
                .request_ru(u32::MAX, name, now, 1000.0, 5000, 7)
                .await
                .unwrap();
        }
    }
    manager.conciliate();
    assert_eq!(burst(&mut manager, "high", 1_000_000_001).await, 100);
    assert_eq!(burst(&mut manager, "low", 1_000_000_001).await, 50);
    manager
        .modify(u32::MAX, configured("high", 1, 100))
        .await
        .unwrap();
    manager
        .modify(u32::MAX, configured("low", 16, 100))
        .await
        .unwrap();
    manager.conciliate();
    assert_eq!(burst(&mut manager, "high", 1_000_000_002).await, 50);
    assert_eq!(burst(&mut manager, "low", 1_000_000_002).await, 100);
    for name in ["high", "low"] {
        let group = manager.get(u32::MAX, name, false).await.unwrap();
        assert_eq!(group.ru.unwrap().settings.unwrap().burst_limit, 100);
    }
}
#[tokio::test]
async fn mcs_097_same_priority_shares_cleanup_and_zero_limit() {
    let mut manager = Manager::open(Arc::new(MemoryStore::default()), WriteRole::LegacyAll)
        .await
        .unwrap();
    manager
        .add(u32::MAX, configured("small", 6, 100))
        .await
        .unwrap();
    manager
        .add(u32::MAX, configured("large", 6, 300))
        .await
        .unwrap();
    manager.set_limit_from_storage(u32::MAX, 200.0, 0);
    for now in [0, 1_000_000_000] {
        for (name, demand) in [("small", 50.0), ("large", 300.0)] {
            manager
                .request_ru(u32::MAX, name, now, demand, 5000, 7)
                .await
                .unwrap();
        }
    }
    manager.conciliate();
    assert_eq!(burst(&mut manager, "small", 1_000_000_001).await, 50);
    assert_eq!(burst(&mut manager, "large", 1_000_000_001).await, 150);
    manager.cleanup_demand_trackers(602_000_000_000);
    assert!(
        manager
            .buckets
            .values()
            .all(|bucket| bucket.demands.is_empty())
    );
    manager.set_limit_from_storage(u32::MAX, 0.0, 602_000_000_000);
    manager.conciliate();
    assert_eq!(burst(&mut manager, "small", 602_000_000_001).await, 100);
    assert_eq!(burst(&mut manager, "large", 602_000_000_001).await, 300);
}
