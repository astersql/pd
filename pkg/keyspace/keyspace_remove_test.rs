// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{Keyspace, KeyspaceIdentity};
use std::sync::Arc;
#[tokio::test]
async fn mcs_040_behavior() {
    let f = crate::etcd_fixture::Fixture::start();
    let m = KeyspaceManager::new(
        astersql_pd_storage_endpoint::StorageEndpoint::new(Arc::new(f.store().await))
            .keyspace_storage(ClusterPaths::new(42)),
    );
    m.save_new(&Keyspace {
        identity: Some(KeyspaceIdentity::Id(40)),
        name: "removed40".into(),
        state: 2,
        ..Default::default()
    })
    .await
    .unwrap();
    m.remove(40).await.unwrap();
    assert!(
        matches!(
            m.load_by_name("removed40").await,
            Err(KeyspaceError::NotFound)
        ),
        "successful deletion must remove both metadata indexes"
    );
    assert!(m.cache.get(40).is_none());
}
use astersql_pd_storage_domain::{
    Base, Consistency, MetaServiceGroupStatus, MetaStore, Read, StorageEndpoint,
};
use std::{collections::BTreeMap, time::Duration};
use tokio_util::sync::CancellationToken;
fn item(id: u32, state: i32) -> Keyspace {
    Keyspace {
        identity: Some(KeyspaceIdentity::Id(id)),
        name: format!("removed{id}"),
        state,
        ..Default::default()
    }
}
async fn manager(store: Arc<dyn MetaStore>) -> KeyspaceManager {
    KeyspaceManager::with_resources(
        astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone())
            .keyspace_storage(ClusterPaths::new(42)),
        KeyspaceCreateResources {
            meta_groups: Some(Arc::new(MetaServiceGroupManager::new(
                store,
                ClusterPaths::new(42),
                BTreeMap::from([("g".into(), "127.0.0.1:10001".into())]),
            ))),
            ..Default::default()
        },
    )
}
async fn set_count(store: Arc<dyn MetaStore>, count: i64) {
    let mut txn = Base::new(store.as_ref()).begin_txn();
    StorageEndpoint::new(store.clone())
        .save_meta_service_group_status(
            &mut txn,
            &ClusterPaths::new(42),
            "g",
            Some(&MetaServiceGroupStatus {
                enabled: true,
                assignment_count: count,
            }),
        )
        .unwrap();
    txn.commit::<String>().await.unwrap();
}
async fn count(store: Arc<dyn MetaStore>) -> i64 {
    let mut txn = Base::new(store.as_ref()).begin_txn();
    StorageEndpoint::new(store.clone())
        .load_meta_service_group_status(
            &mut txn,
            &ClusterPaths::new(42),
            &BTreeMap::from([("g".into(), String::new())]),
        )
        .await
        .unwrap()["g"]
        .assignment_count
}
#[tokio::test]
async fn mcs_040_strict_state_protection_and_unknown_enum() {
    let f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store.clone()).await;
    for state in [0, 1, 2, 3, 99] {
        let value = item(100 + state as u32, state);
        m.save_new(&value).await.unwrap();
        let result = m.remove(value.id()).await;
        if matches!(state, 0 | 1) {
            assert_eq!(
                result.unwrap_err().to_string(),
                format!(
                    "cannot remove keyspace in state {}",
                    crate::keyspace_config::state_name(state)
                )
            );
            assert_eq!(m.load_by_id(value.id()).await.unwrap(), value);
            assert!(m.cache.get(value.id()).is_some());
        } else {
            result.unwrap();
            assert!(matches!(
                m.load_by_id(value.id()).await,
                Err(KeyspaceError::NotFound)
            ));
            assert!(m.cache.get(value.id()).is_none());
            assert!(matches!(
                m.remove(value.id()).await,
                Err(KeyspaceError::NotFound)
            ));
        }
    }
    for (kernel, id) in [
        (Kernel::Classic, 0),
        (
            Kernel::NextGeneration,
            astersql_pd_keyspace_constant::SYSTEM_KEYSPACE_ID,
        ),
    ] {
        let m = KeyspaceManager::with_resources(
            m.storage.clone(),
            KeyspaceCreateResources {
                kernel,
                ..Default::default()
            },
        );
        let value = item(id, 3);
        m.save_new(&value).await.unwrap();
        assert_eq!(
            m.remove(id).await.unwrap_err().to_string(),
            crate::keyspace_state::protected_state_error(kernel).to_string()
        );
        assert_eq!(m.load_by_id(id).await.unwrap(), value);
    }
}
#[tokio::test]
async fn mcs_040_eligibility_filters_and_deferred_predicate_stage_no_side_effects() {
    use crate::keyspace_remove::RemovalEligibility;
    let f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store.clone()).await;
    for state in [0, 1, 2, 3, 99] {
        let mut value = item(100 + state as u32, state);
        value
            .config
            .insert("meta_service_group_id".into(), "g".into());
        m.save_new(&value).await.unwrap();
    }
    let before = store
        .read(Read::prefix(b"/pd/42", Consistency::Linearizable))
        .await
        .unwrap();
    let _guard = m.mutation.lock().await;
    for id in [0, 500, 100, 101, 199] {
        let mut txn = Base::new(store.as_ref()).begin_txn();
        assert_eq!(
            m.stage_remove_if_eligible(&mut txn, id, true, None)
                .await
                .unwrap(),
            RemovalEligibility::Skipped
        );
    }
    for id in [102, 103] {
        let mut txn = Base::new(store.as_ref()).begin_txn();
        assert_eq!(
            m.stage_remove_if_eligible(&mut txn, id, true, Some(&|group| group != "g"))
                .await
                .unwrap(),
            RemovalEligibility::Deferred {
                meta_service_group: "g".into()
            }
        );
    }
    assert_eq!(
        before.entries,
        store
            .read(Read::prefix(b"/pd/42", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
    );
}
#[tokio::test]
async fn mcs_040_names_meta_and_assignment_share_actual_delete_revision() {
    let f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store.clone()).await;
    let mut value = item(40, 2);
    value
        .config
        .insert("meta_service_group_id".into(), "g".into());
    m.save_new(&value).await.unwrap();
    set_count(store.clone(), 1).await;
    let before = store
        .read(Read::prefix(b"/pd/42", Consistency::Linearizable))
        .await
        .unwrap();
    let mut watch = store
        .watch(b"/pd/42".to_vec(), before.revision + 1)
        .await
        .unwrap();
    m.remove(40).await.unwrap();
    let batch = tokio::time::timeout(
        Duration::from_secs(3),
        watch.next(&CancellationToken::new()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(batch.events.len(), 3);
    assert!(
        batch
            .events
            .iter()
            .all(|event| event.revision() == batch.revision)
    );
    assert!(batch.events.iter().any(|e| {
        e.key()
            == ClusterPaths::new(42)
                .keyspace_id_path("removed40")
                .as_bytes()
    }));
    assert!(
        batch
            .events
            .iter()
            .any(|e| e.key() == ClusterPaths::new(42).keyspace_meta_path(40).as_bytes())
    );
    assert_eq!(count(store).await, 0);
}
#[tokio::test]
async fn mcs_040_prepared_abort_and_real_cas_conflict_keep_cache_and_indexes() {
    let f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store.clone()).await;
    let original = item(40, 2);
    m.save_new(&original).await.unwrap();
    let prepared = m.prepare_remove(40).await.unwrap();
    assert_eq!(prepared.id(), 40);
    drop(prepared);
    assert_eq!(m.load_by_id(40).await.unwrap(), original);
    assert!(m.cache.get(40).is_some());
    let prepared = m.prepare_remove(40).await.unwrap();
    let mut updated = original.clone();
    updated.state = 0;
    let mut txn = Base::new(store.as_ref()).begin_txn();
    m.storage
        .save_keyspace_meta(&mut txn, &m.storage.paths, &updated)
        .unwrap();
    txn.commit::<String>().await.unwrap();
    assert!(matches!(
        prepared.commit().await,
        Err(KeyspaceError::Conflict)
    ));
    assert_eq!(m.load_by_name("removed40").await.unwrap(), updated);
    assert!(m.cache.get(40).is_some());
    assert!(m.remove(40).await.is_err());
}
#[tokio::test]
async fn mcs_040_commit_fault_and_retry_publish_real_observation_cache_after_commit() {
    use astersql_pd_cache::{TtlCache, TtlDuration};
    let mut f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = Arc::new(manager(store.clone()).await);
    let mut original = item(40, 2);
    original
        .config
        .insert("meta_service_group_id".into(), "g".into());
    m.save_new(&original).await.unwrap();
    set_count(store.clone(), 1).await;
    let barrier_key = ClusterPaths::new(42).gc_barrier_path(40, "worker");
    let barrier = astersql_pd_storage_endpoint::GCBarrier::new("worker", 10, None);
    let bytes = serde_json::to_vec(&barrier.to_service_safe_point(40)).unwrap();
    Base::new(store.as_ref())
        .save(barrier_key.as_bytes(), &bytes)
        .await
        .unwrap();
    let observation = Arc::new(TtlCache::new(
        tokio::runtime::Handle::current(),
        CancellationToken::new(),
        Duration::from_secs(60),
        TtlDuration::from_nanos(3_600_000_000_000),
    ));
    let raw = store
        .read(Read::range(
            barrier_key.as_bytes(),
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    observation.put(40, raw.entries[0].value.clone());
    let cache = observation.clone();
    let weak = Arc::downgrade(&m);
    m.set_gc_barrier_invalidator(Some(Arc::new(move |id| {
        assert!(
            weak.upgrade().unwrap().mutation.try_lock().is_ok(),
            "observer must run outside metadata lock"
        );
        cache.remove(&id);
    })));
    let prepared = m.prepare_remove(40).await.unwrap();
    assert!(observation.get(&40).is_some());
    f.command("stop", "stopped");
    assert!(prepared.commit().await.is_err());
    assert!(observation.get(&40).is_some());
    assert!(m.cache.get(40).is_some());
    f.command("start", "restarted");
    let fresh: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let mut retry = manager(fresh.clone()).await;
    retry.cache = m.cache.clone();
    let retry = Arc::new(retry);
    let weak = Arc::downgrade(&retry);
    let cache = observation.clone();
    retry.set_gc_barrier_invalidator(Some(Arc::new(move |id| {
        let owner = weak.upgrade().unwrap();
        assert!(
            owner.mutation.try_lock().is_ok(),
            "observer must run outside metadata lock"
        );
        assert!(
            owner.cache.get(id).is_none(),
            "observer sees committed cache removal"
        );
        cache.remove(&id);
    })));
    assert_eq!(retry.load_by_id(40).await.unwrap().state, 2);
    assert_eq!(count(fresh.clone()).await, 1);
    retry.remove(40).await.unwrap();
    assert!(retry.cache.get(40).is_none());
    assert!(observation.get(&40).is_none());
    assert!(matches!(
        retry.load_by_name("removed40").await,
        Err(KeyspaceError::NotFound)
    ));
    assert_eq!(count(fresh.clone()).await, 0);
    assert_eq!(
        fresh
            .read(Read::range(
                barrier_key.as_bytes(),
                b"",
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries[0]
            .value,
        bytes
    );
    observation.shutdown().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_040_concurrent_removals_have_one_winner_and_one_counter_decrement() {
    let f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = Arc::new(manager(store.clone()).await);
    let mut original = item(40, 2);
    original
        .config
        .insert("meta_service_group_id".into(), "g".into());
    m.save_new(&original).await.unwrap();
    set_count(store.clone(), 1).await;
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let a = m.clone();
    let b = m.clone();
    let start = barrier.clone();
    let first = tokio::spawn(async move {
        start.wait().await;
        a.remove(40).await
    });
    let second = tokio::spawn(async move {
        barrier.wait().await;
        b.remove(40).await
    });
    let first = first.await.unwrap();
    let second = second.await.unwrap();
    assert_ne!(first.is_ok(), second.is_ok());
    let rejected = if let Err(e) = first {
        e
    } else {
        second.unwrap_err()
    };
    assert!(matches!(rejected, KeyspaceError::NotFound));
    assert_eq!(count(store).await, 0);
    assert!(m.cache.get(40).is_none());
}
