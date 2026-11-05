// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
use astersql_pd_keyspace::KeyspaceManager;
use astersql_pd_storage_domain::{BarrierTime, MetaStore, StorageEndpoint};
use astersql_pd_typeutil::Duration;
use std::sync::Arc;
#[tokio::test]
async fn mcs_045_behavior() {
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
    m.advance_txn_safe_point(NULL_KEYSPACE_ID, 10, BarrierTime::from_unix(100, 0))
        .await
        .unwrap();
    assert!(
        m.set_gc_barrier(
            NULL_KEYSPACE_ID,
            "old",
            9,
            Duration::from_nanos(1_000_000_000),
            BarrierTime::from_unix(100, 0)
        )
        .await
        .is_err(),
        "a barrier must not protect an already discarded snapshot"
    );
}

use astersql_pd_storage_domain::{Base, Consistency, GCBarrier, Keyspace, KeyspaceIdentity, Read};
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
#[tokio::test]
async fn mcs_045_ttl_subseconds_sentinel_and_negative_epoch_round_trip() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store.clone()).await;
    for (seconds, nanos, ttl, expected) in [
        (100, 0, 1, 101),
        (100, 999_999_999, 1, 101),
        (100, 500_000_000, 500_000_000, 101),
        (100, 0, 1_000_000_000, 101),
        (100, 1, 1_000_000_000, 102),
        (100, 0, i64::MAX - 1, 9_223_372_137),
    ] {
        let result = m
            .set_gc_barrier(
                NULL_KEYSPACE_ID,
                "ttl",
                10,
                Duration::from_nanos(ttl),
                BarrierTime::from_unix(seconds, nanos),
            )
            .await
            .unwrap();
        assert_eq!(
            result.expiration_time,
            Some(BarrierTime::from_unix(expected, 0))
        );
        assert_eq!(
            m.storage
                .load_gc_barrier(&m.paths, NULL_KEYSPACE_ID, "ttl")
                .await
                .unwrap(),
            Some(result)
        );
    }
    let result = m
        .set_gc_barrier(
            NULL_KEYSPACE_ID,
            "ttl",
            10,
            Duration::from_nanos(i64::MAX),
            BarrierTime::from_unix(100, 1),
        )
        .await
        .unwrap();
    assert!(result.expiration_time.is_none());
    let row = store
        .read(Read::range(
            m.paths.gc_barrier_path(NULL_KEYSPACE_ID, "ttl").as_bytes(),
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert_eq!(row.entries[0].lease, None);
    assert!(String::from_utf8_lossy(&row.entries[0].value).contains("9223372036854775807"));
    let result = m
        .set_gc_barrier(
            NULL_KEYSPACE_ID,
            "negative",
            10,
            Duration::from_nanos(1),
            BarrierTime::from_unix(-2, 0),
        )
        .await
        .unwrap();
    assert_eq!(result.expiration_time, Some(BarrierTime::from_unix(-1, 0)));
    assert!(
        m.storage
            .load_gc_barrier(&m.paths, NULL_KEYSPACE_ID, "negative")
            .await
            .unwrap()
            .unwrap()
            .expiration_time
            .is_none(),
        "legacy nonpositive expiration decoding matches Go, even though the immediate new value retains its time"
    );
}
#[tokio::test]
async fn mcs_045_validation_order_scope_redirect_and_shrinking_barrier() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store.clone()).await;
    m.keyspaces
        .save_new(&Keyspace {
            identity: Some(KeyspaceIdentity::Id(3)),
            name: "unified45".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let now = BarrierTime::from_unix(100, 0);
    let ttl = Duration::from_nanos(i64::MAX);
    assert!(matches!(
        m.set_gc_barrier(999, "gc_worker", 10, Duration::from_nanos(0), now)
            .await,
        Err(GCReadError::InvalidArgument {
            argument: "ttl",
            ..
        })
    ));
    assert!(matches!(
        m.set_gc_barrier(999, "gc_worker", 10, ttl, now).await,
        Err(GCReadError::Keyspace(_))
    ));
    assert!(matches!(
        m.delete_gc_barrier(999, "gc_worker").await,
        Err(GCReadError::Keyspace(_))
    ));
    for ttl in [0, -1, i64::MIN] {
        let error = m
            .set_gc_barrier(NULL_KEYSPACE_ID, "a", 10, Duration::from_nanos(ttl), now)
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            format!(
                "[PD:common:ErrInvalidArgument]invalid argument for ttl: {}",
                Duration::from_nanos(ttl)
            )
        );
    }
    assert!(matches!(
        m.set_gc_barrier(3, "gc_worker", 10, ttl, now).await,
        Err(GCReadError::ReservedBarrierId(_))
    ));
    assert!(matches!(
        m.delete_gc_barrier(3, "").await,
        Err(GCReadError::InvalidArgument {
            argument: "barrierID",
            ..
        })
    ));
    m.advance_txn_safe_point(NULL_KEYSPACE_ID, 20, now)
        .await
        .unwrap();
    for ts in [30, 40, 25, 20] {
        let value = m
            .set_gc_barrier(3, "native_br/a", ts, ttl, now)
            .await
            .unwrap();
        assert_eq!(value.barrier_ts, ts);
        assert_eq!(
            m.storage
                .load_gc_barrier(&m.paths, NULL_KEYSPACE_ID, "native_br/a")
                .await
                .unwrap(),
            Some(value)
        );
    }
    assert!(matches!(
        m.set_gc_barrier(3, "a", 19, ttl, now).await,
        Err(GCReadError::BarrierBehind {
            barrier: 19,
            txn: 20
        })
    ));
    let removed = m
        .delete_gc_barrier(0x0100_0000, "native_br/a")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(removed.barrier_ts, 20);
    assert!(
        m.delete_gc_barrier(3, "native_br/a")
            .await
            .unwrap()
            .is_none()
    );
}
#[tokio::test]
async fn mcs_045_set_delete_revisions_absence_and_expiration_do_not_mutate_point_cache() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store.clone()).await;
    let now = BarrierTime::from_unix(100, 0);
    m.advance_txn_safe_point(NULL_KEYSPACE_ID, 10, now)
        .await
        .unwrap();
    let role = m.on_node_becomes_leader().await;
    m.get_gc_state(NULL_KEYSPACE_ID, true).await.unwrap();
    let mut raw = m
        .storage
        .begin_gc_state_transaction(&m.paths)
        .await
        .unwrap();
    raw.batch.set_txn_safe_point(NULL_KEYSPACE_ID, 20);
    raw.commit().await.unwrap();
    assert!(
        m.set_gc_barrier(
            NULL_KEYSPACE_ID,
            "a",
            19,
            Duration::from_nanos(i64::MAX),
            now
        )
        .await
        .is_err()
    );
    assert_eq!(
        m.get_gc_state(NULL_KEYSPACE_ID, true)
            .await
            .unwrap()
            .txn_safe_point,
        10,
        "barrier errors retain existing point cache, unlike advancement errors"
    );
    let before = store
        .read(Read::prefix(b"/", Consistency::Linearizable))
        .await
        .unwrap();
    let mut watch = store
        .watch(b"/".to_vec(), before.revision + 1)
        .await
        .unwrap();
    let barrier = m
        .set_gc_barrier(NULL_KEYSPACE_ID, "a", 25, Duration::from_nanos(1), now)
        .await
        .unwrap();
    let events = watch.next(&CancellationToken::new()).await.unwrap();
    assert_eq!(events.events.len(), 2);
    assert_eq!(events.events[0].revision(), events.events[1].revision());
    assert_eq!(
        m.get_gc_state(NULL_KEYSPACE_ID, true)
            .await
            .unwrap()
            .txn_safe_point,
        10
    );
    assert_eq!(
        m.delete_gc_barrier(NULL_KEYSPACE_ID, "a").await.unwrap(),
        Some(barrier)
    );
    let revision = Base::new(store.as_ref())
        .load(m.paths.gc_state_revision_path().as_bytes())
        .await
        .unwrap();
    let revision = String::from_utf8(revision).unwrap().parse::<u64>().unwrap();
    assert!(
        m.delete_gc_barrier(NULL_KEYSPACE_ID, "a")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        Base::new(store.as_ref())
            .load(m.paths.gc_state_revision_path().as_bytes())
            .await
            .unwrap(),
        (revision + 1).to_string().as_bytes()
    );
    role.shutdown().await;
}
#[tokio::test]
async fn mcs_045_set_and_delete_cas_conflicts_preserve_authoritative_rows() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store).await;
    let now = BarrierTime::from_unix(100, 0);
    let cancel = CancellationToken::new();
    let prepared = m
        .prepare_set_gc_barrier(
            NULL_KEYSPACE_ID,
            "a",
            10,
            Duration::from_nanos(i64::MAX),
            now,
            &cancel,
        )
        .await
        .unwrap();
    let mut raw = m
        .storage
        .begin_gc_state_transaction(&m.paths)
        .await
        .unwrap();
    raw.batch.set_txn_safe_point(NULL_KEYSPACE_ID, 20);
    raw.commit().await.unwrap();
    assert!(matches!(
        prepared.commit().await,
        Err(GCReadError::Transaction(_))
    ));
    assert!(
        m.storage
            .load_gc_barrier(&m.paths, NULL_KEYSPACE_ID, "a")
            .await
            .unwrap()
            .is_none()
    );
    m.set_gc_barrier(
        NULL_KEYSPACE_ID,
        "a",
        30,
        Duration::from_nanos(i64::MAX),
        now,
    )
    .await
    .unwrap();
    let prepared = m
        .prepare_delete_gc_barrier(NULL_KEYSPACE_ID, "a", &cancel)
        .await
        .unwrap();
    assert_eq!(prepared.barrier().unwrap().barrier_ts, 30);
    let mut raw = m
        .storage
        .begin_gc_state_transaction(&m.paths)
        .await
        .unwrap();
    raw.batch
        .set_gc_barrier(NULL_KEYSPACE_ID, &GCBarrier::new("a", 40, None))
        .unwrap();
    raw.commit().await.unwrap();
    assert!(prepared.commit().await.is_err());
    assert_eq!(
        m.storage
            .load_gc_barrier(&m.paths, NULL_KEYSPACE_ID, "a")
            .await
            .unwrap()
            .unwrap()
            .barrier_ts,
        40
    );
}
#[tokio::test]
async fn mcs_045_commit_faults_and_wal_retry_preserve_ttl_and_deletion() {
    let mut f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store).await;
    let now = BarrierTime::from_unix(100, 900_000_000);
    let cancel = CancellationToken::new();
    let prepared = m
        .prepare_set_gc_barrier(
            NULL_KEYSPACE_ID,
            "a",
            10,
            Duration::from_nanos(200_000_000),
            now,
            &cancel,
        )
        .await
        .unwrap();
    f.command("stop", "stopped");
    assert!(prepared.commit().await.is_err());
    f.command("start", "restarted");
    let fresh = manager(Arc::new(f.store().await)).await;
    assert!(
        fresh
            .storage
            .load_gc_barrier(&fresh.paths, NULL_KEYSPACE_ID, "a")
            .await
            .unwrap()
            .is_none()
    );
    let value = fresh
        .set_gc_barrier(
            NULL_KEYSPACE_ID,
            "a",
            10,
            Duration::from_nanos(200_000_000),
            now,
        )
        .await
        .unwrap();
    assert_eq!(value.expiration_time, Some(BarrierTime::from_unix(102, 0)));
    let prepared = fresh
        .prepare_delete_gc_barrier(NULL_KEYSPACE_ID, "a", &cancel)
        .await
        .unwrap();
    f.command("stop", "stopped");
    assert!(prepared.commit().await.is_err());
    f.command("start", "restarted");
    let recovered = manager(Arc::new(f.store().await)).await;
    assert_eq!(
        recovered
            .storage
            .load_gc_barrier(&recovered.paths, NULL_KEYSPACE_ID, "a")
            .await
            .unwrap(),
        Some(value.clone())
    );
    assert_eq!(
        recovered
            .delete_gc_barrier(NULL_KEYSPACE_ID, "a")
            .await
            .unwrap(),
        Some(value)
    );
}
#[tokio::test]
async fn mcs_045_force_delete_bypasses_reserved_empty_and_corrupt_payload() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store.clone()).await;
    for id in ["gc_worker", ""] {
        let key = m.paths.gc_barrier_path(NULL_KEYSPACE_ID, id);
        Base::new(store.as_ref())
            .save(key.as_bytes(), b"not-json")
            .await
            .unwrap();
        assert!(m.delete_gc_barrier(NULL_KEYSPACE_ID, id).await.is_err());
        m.force_delete_service_gc_safe_point(id).await.unwrap();
        assert!(
            Base::new(store.as_ref())
                .load(key.as_bytes())
                .await
                .unwrap()
                .is_empty()
        );
        m.force_delete_service_gc_safe_point(id).await.unwrap();
    }
}

#[tokio::test]
async fn mcs_045_expired_set_barrier_is_removed_only_by_confirmed_advancement() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store).await;
    let start = BarrierTime::from_unix(100, 900_000_000);
    let barrier = m
        .set_gc_barrier(
            NULL_KEYSPACE_ID,
            "lease",
            20,
            Duration::from_nanos(200_000_000),
            start,
        )
        .await
        .unwrap();
    assert_eq!(
        barrier.expiration_time,
        Some(BarrierTime::from_unix(102, 0))
    );
    for now in [start, BarrierTime::from_unix(102, 0)] {
        assert_eq!(
            m.advance_txn_safe_point(NULL_KEYSPACE_ID, 30, now)
                .await
                .unwrap()
                .new_txn_safe_point,
            20
        );
        assert!(
            m.storage
                .load_gc_barrier(&m.paths, NULL_KEYSPACE_ID, "lease")
                .await
                .unwrap()
                .is_some()
        );
    }
    assert_eq!(
        m.advance_txn_safe_point(NULL_KEYSPACE_ID, 30, BarrierTime::from_unix(102, 1))
            .await
            .unwrap()
            .new_txn_safe_point,
        30
    );
    assert!(
        m.storage
            .load_gc_barrier(&m.paths, NULL_KEYSPACE_ID, "lease")
            .await
            .unwrap()
            .is_none()
    );
}
