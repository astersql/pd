// Copyright 2026 AsterSQL.
use super::grpc_metadata::{Target, target};
use crate::{Bucket, BucketState, Group, GroupState, Manager, TokenLimits, WriteRole};
use astersql_pd_storage_domain::{Entry, Event, MetaStore};
use astersql_pd_storage_kv::MemoryStore;
use prost::Message;
use std::sync::Arc;
#[test]
fn mcs_098_watch_path_identity_boundaries() {
    assert_eq!(
        target("resource_group/settings/a/b"),
        Some(Target::Setting(u32::MAX, "a/b"))
    );
    assert_eq!(
        target("resource_group/keyspace/settings/7/a"),
        Some(Target::Setting(7, "a"))
    );
    assert_eq!(
        target("resource_group/keyspace/states/0/default"),
        Some(Target::State(0, "default"))
    );
    assert_eq!(
        target("resource_group/keyspace/service_limits/4294967295"),
        Some(Target::Limit(u32::MAX))
    );
    for key in [
        "resource_group/settings/",
        "resource_group/keyspace/settings/7/",
        "resource_group/keyspace/settings/-1/a",
        "resource_group/keyspace/service_limits/7/",
        "resource_group/keyspace/service_limits/+7",
        "resource_group/keyspace/service_limits/4294967296",
    ] {
        assert_eq!(target(key), None, "{key}");
    }
}
#[tokio::test]
async fn mcs_098_watched_settings_preserve_balance_and_state_keeps_client_slots() {
    let store: Arc<dyn MetaStore> = Arc::new(MemoryStore::default());
    let mut manager = Manager::open(store, WriteRole::LegacyAll).await.unwrap();
    let group = Group {
        name: "a".into(),
        mode: 1,
        ru_settings_present: true,
        priority: 2,
        ru: Some(Bucket {
            tokens: 75.0,
            settings: Some(TokenLimits {
                fill_rate: 100,
                burst_limit: 100,
                ..Default::default()
            }),
        }),
        ..Default::default()
    };
    manager.add(7, group.clone()).await.unwrap();
    manager
        .request_ru(7, "a", 1_000_000_000, 10.0, 1000, 23)
        .await
        .unwrap();
    let balance = manager.get(7, "a", false).await.unwrap().ru.unwrap().tokens;
    let mut update = group;
    update.priority = 12;
    update.ru.as_mut().unwrap().tokens = 500.0;
    let key = b"resource_group/keyspace/settings/7/a";
    let value = super::grpc_codec::encode(&update, 7).encode_to_vec();
    manager
        .metadata_event(Event::Put(Entry {
            key: key.to_vec(),
            value,
            create_revision: 1,
            mod_revision: 2,
            version: 2,
            lease: None,
        }))
        .unwrap();
    let current = manager.get(7, "a", false).await.unwrap();
    assert_eq!(current.priority, 12);
    assert_eq!(current.ru.unwrap().tokens, balance);
    assert!(manager.buckets[&(7, "a".into())].slots.contains_key(&23));
    let state = GroupState {
        ru: Some(BucketState {
            tokens: 19.0,
            initialized: true,
            last_update: Some("2026-11-06T00:00:00Z".into()),
        }),
        ..Default::default()
    };
    manager
        .metadata_event(Event::Put(Entry {
            key: b"resource_group/keyspace/states/7/a".to_vec(),
            value: serde_json::to_vec(&state).unwrap(),
            create_revision: 1,
            mod_revision: 3,
            version: 2,
            lease: None,
        }))
        .unwrap();
    assert_eq!(
        manager.get(7, "a", false).await.unwrap().ru.unwrap().tokens,
        19.0
    );
    assert!(manager.buckets[&(7, "a".into())].slots.contains_key(&23));
    manager
        .metadata_event(Event::Delete {
            key: key.to_vec(),
            revision: 4,
        })
        .unwrap();
    assert!(manager.get(7, "a", false).await.is_err());
    assert!(!manager.buckets.contains_key(&(7, "a".into())));
}

#[tokio::test]
async fn mcs_097_overwriting_group_resets_bucket_but_keeps_keyspace_demand_tracker() {
    let store: Arc<dyn MetaStore> = Arc::new(MemoryStore::default());
    let mut manager = Manager::open(store, WriteRole::LegacyAll).await.unwrap();
    let mut group = Group::reserved_default();
    group.name = "tracked".into();
    manager.add(7, group.clone()).await.unwrap();
    manager
        .request_ru(7, "tracked", 1_000_000_000, 10.0, 1000, 23)
        .await
        .unwrap();
    manager
        .request_ru(7, "tracked", 2_000_000_000, 10.0, 1000, 23)
        .await
        .unwrap();
    let rate = manager.buckets[&(7, "tracked".into())].demands[&23].rate;
    assert!(rate > 0.0);
    manager.add(7, group).await.unwrap();
    let bucket = &manager.buckets[&(7, "tracked".into())];
    assert_eq!(bucket.demands[&23].rate, rate);
    assert!(bucket.has_tracker);
    assert!(bucket.slots.is_empty());
    assert!(!bucket.initialized);
}
