// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
use astersql_pd_keyspace::KeyspaceManager;
use astersql_pd_storage_domain::{BarrierTime, MetaStore, StorageEndpoint};
use astersql_pd_typeutil::Duration;
use std::sync::Arc;
#[tokio::test]
async fn mcs_046_behavior() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let p = ClusterPaths::new(42);
    let m = GCStateManager::new(
        StorageEndpoint::new(store.clone()),
        p.clone(),
        Arc::new(KeyspaceManager::new(
            astersql_pd_storage_endpoint::StorageEndpoint::new(store).keyspace_storage(p),
        )),
        tokio::runtime::Handle::current(),
    );
    m.advance_txn_safe_point(NULL_KEYSPACE_ID, 20, BarrierTime::from_unix(100, 0))
        .await
        .unwrap();
    assert!(
        m.set_global_gc_barrier(
            "global",
            19,
            Duration::from_nanos(i64::MAX),
            BarrierTime::from_unix(100, 0)
        )
        .await
        .is_err(),
        "global barriers must not precede any scope's transaction safe point"
    );
}

use astersql_pd_storage_domain::{
    Base, Consistency, GlobalGCBarrier, Keyspace, KeyspaceIdentity, Read,
};
use tokio_util::sync::CancellationToken;
async fn manager(store: Arc<dyn MetaStore>) -> GCStateManager {
    let p = ClusterPaths::new(42);
    GCStateManager::new(
        StorageEndpoint::new(store.clone()),
        p.clone(),
        Arc::new(KeyspaceManager::new(
            astersql_pd_storage_endpoint::StorageEndpoint::new(store).keyspace_storage(p),
        )),
        tokio::runtime::Handle::current(),
    )
}
async fn point(m: &GCStateManager, id: u32, value: u64) {
    let mut tx = m
        .storage
        .begin_gc_state_transaction(&m.paths)
        .await
        .unwrap();
    tx.batch.set_txn_safe_point(id, value);
    tx.commit().await.unwrap();
}
#[tokio::test]
async fn mcs_046_pagination_all_enabled_unified_and_ties_match_iterator_order() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store).await;
    assert_eq!(
        m.get_max_txn_safe_point().await.unwrap(),
        MaxTxnSafePoint::default()
    );
    for id in 1..=109 {
        m.keyspaces
            .save_new(&Keyspace {
                identity: Some(KeyspaceIdentity::Id(id)),
                name: if id == 105 {
                    "终点\"\0".into()
                } else {
                    format!("space{id}")
                },
                state: match id {
                    106 => 1,
                    107 => 2,
                    108 => 3,
                    109 => 99,
                    _ => 0,
                },
                ..Default::default()
            })
            .await
            .unwrap();
        point(
            &m,
            id,
            if id <= 105 {
                id as u64
            } else {
                1000 * id as u64
            },
        )
        .await;
    }
    point(&m, NULL_KEYSPACE_ID, 105).await;
    let maximum = m.get_max_txn_safe_point().await.unwrap();
    assert_eq!((maximum.safe_point, maximum.keyspace_id), (105, 105));
    assert!(
        m.set_global_gc_barrier(
            "a",
            104,
            Duration::from_nanos(i64::MAX),
            BarrierTime::from_unix(100, 0)
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("终点\\\"\\x00")
    );
    point(&m, NULL_KEYSPACE_ID, 106).await;
    assert_eq!(
        m.get_max_txn_safe_point().await.unwrap().keyspace_id,
        NULL_KEYSPACE_ID
    );
    point(&m, 1, 106).await;
    assert_eq!(
        m.get_max_txn_safe_point().await.unwrap().keyspace_id,
        1,
        "first metadata ID wins equal values; null is considered last"
    );
    m.set_global_gc_barrier(
        "a",
        106,
        Duration::from_nanos(i64::MAX),
        BarrierTime::from_unix(100, 0),
    )
    .await
    .unwrap();
}
#[tokio::test]
async fn mcs_046_validation_ttl_reserved_global_name_and_shrinking_ts() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store).await;
    let now = BarrierTime::from_unix(100, 900_000_000);
    assert!(matches!(
        m.set_global_gc_barrier("", 10, Duration::from_nanos(0), now)
            .await,
        Err(GCReadError::InvalidArgument {
            argument: "ttl",
            ..
        })
    ));
    assert!(matches!(
        m.delete_global_gc_barrier("").await,
        Err(GCReadError::InvalidArgument {
            argument: "barrierID",
            ..
        })
    ));
    point(&m, NULL_KEYSPACE_ID, 10).await;
    for ts in [30, 40, 20, 10] {
        let value = m
            .set_global_gc_barrier("gc_worker", ts, Duration::from_nanos(200_000_000), now)
            .await
            .unwrap();
        assert_eq!(value.expiration_time, Some(BarrierTime::from_unix(102, 0)));
        assert_eq!(
            m.storage
                .load_global_gc_barrier(&m.paths, "gc_worker")
                .await
                .unwrap(),
            Some(value)
        );
    }
    assert!(matches!(
        m.set_global_gc_barrier("gc_worker", 9, Duration::from_nanos(i64::MAX), now)
            .await,
        Err(GCReadError::GlobalBarrierBehind { .. })
    ));
    let value = m
        .set_global_gc_barrier("gc_worker", 10, Duration::from_nanos(i64::MAX), now)
        .await
        .unwrap();
    assert!(value.expiration_time.is_none());
    assert_eq!(
        m.delete_global_gc_barrier("gc_worker").await.unwrap(),
        Some(value)
    );
    assert!(
        m.delete_global_gc_barrier("gc_worker")
            .await
            .unwrap()
            .is_none()
    );
}
#[tokio::test]
async fn mcs_046_corrupt_metadata_point_and_max_valid_id_do_not_partially_set() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store.clone()).await;
    let now = BarrierTime::from_unix(100, 0);
    let ttl = Duration::from_nanos(i64::MAX);
    m.keyspaces
        .save_new(&Keyspace {
            identity: Some(KeyspaceIdentity::Id(1)),
            name: "enabled".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let key = m.paths.keyspace_meta_path(100);
    Base::new(store.as_ref())
        .save(key.as_bytes(), b"\xff")
        .await
        .unwrap();
    let before = store
        .read(Read::prefix(b"/", Consistency::Linearizable))
        .await
        .unwrap();
    assert!(m.set_global_gc_barrier("a", 100, ttl, now).await.is_err());
    assert_eq!(
        before.entries,
        store
            .read(Read::prefix(b"/", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
    );
    Base::new(store.as_ref())
        .remove(key.as_bytes())
        .await
        .unwrap();
    Base::new(store.as_ref())
        .save(m.paths.txn_safe_point_path(1).as_bytes(), b"bad")
        .await
        .unwrap();
    assert!(m.set_global_gc_barrier("a", 100, ttl, now).await.is_err());
    point(&m, 1, 90).await;
    assert!(m.set_global_gc_barrier("a", 100, ttl, now).await.is_ok());
    m.keyspaces
        .save_new(&Keyspace {
            identity: Some(KeyspaceIdentity::Id(0xffffff)),
            name: "maximum".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let error = m.get_max_txn_safe_point().await.unwrap_err();
    assert!(
        error.to_string().contains("16777216"),
        "Go iterator performs its final load at last metadata ID plus one; it does not stop merely because the last page was short"
    );
}
#[tokio::test]
async fn mcs_046_cas_conflicts_keep_global_and_point_cache_unchanged() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store).await;
    point(&m, NULL_KEYSPACE_ID, 10).await;
    let role = m.on_node_becomes_leader().await;
    m.get_gc_state(NULL_KEYSPACE_ID, true).await.unwrap();
    let now = BarrierTime::from_unix(100, 0);
    let ttl = Duration::from_nanos(i64::MAX);
    let cancel = CancellationToken::new();
    let prepared = m
        .prepare_set_global_gc_barrier("a", 10, ttl, now, &cancel)
        .await
        .unwrap();
    point(&m, NULL_KEYSPACE_ID, 20).await;
    assert!(prepared.commit().await.is_err());
    assert!(
        m.storage
            .load_global_gc_barrier(&m.paths, "a")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        m.get_gc_state(NULL_KEYSPACE_ID, true)
            .await
            .unwrap()
            .txn_safe_point,
        10,
        "global barrier failures preserve the safe-point cache"
    );
    m.set_global_gc_barrier("a", 20, ttl, now).await.unwrap();
    let prepared = m
        .prepare_delete_global_gc_barrier("a", &cancel)
        .await
        .unwrap();
    let mut tx = m
        .storage
        .begin_gc_state_transaction(&m.paths)
        .await
        .unwrap();
    tx.batch
        .set_global_gc_barrier(&GlobalGCBarrier::new("a", 30, None))
        .unwrap();
    tx.commit().await.unwrap();
    assert!(prepared.commit().await.is_err());
    assert_eq!(
        m.storage
            .load_global_gc_barrier(&m.paths, "a")
            .await
            .unwrap()
            .unwrap()
            .barrier_ts,
        30
    );
    role.shutdown().await;
}
#[tokio::test]
async fn mcs_046_stopped_set_delete_commit_and_same_wal_retry() {
    let mut f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store).await;
    let ttl = Duration::from_nanos(i64::MAX);
    let now = BarrierTime::from_unix(100, 0);
    let cancel = CancellationToken::new();
    let prepared = m
        .prepare_set_global_gc_barrier("a", 20, ttl, now, &cancel)
        .await
        .unwrap();
    f.command("stop", "stopped");
    assert!(prepared.commit().await.is_err());
    f.command("start", "restarted");
    let fresh = manager(Arc::new(f.store().await)).await;
    assert!(
        fresh
            .storage
            .load_global_gc_barrier(&fresh.paths, "a")
            .await
            .unwrap()
            .is_none()
    );
    let original = fresh
        .set_global_gc_barrier("a", 20, ttl, now)
        .await
        .unwrap();
    let prepared = fresh
        .prepare_delete_global_gc_barrier("a", &cancel)
        .await
        .unwrap();
    f.command("stop", "stopped");
    assert!(prepared.commit().await.is_err());
    f.command("start", "restarted");
    let recovered = manager(Arc::new(f.store().await)).await;
    assert_eq!(
        recovered
            .storage
            .load_global_gc_barrier(&recovered.paths, "a")
            .await
            .unwrap(),
        Some(original.clone())
    );
    assert_eq!(
        recovered.delete_global_gc_barrier("a").await.unwrap(),
        Some(original)
    );
}
