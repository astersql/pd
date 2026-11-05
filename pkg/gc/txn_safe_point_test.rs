// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
use astersql_pd_keyspace::KeyspaceManager;
use astersql_pd_storage_domain::{BarrierTime, MetaStore, StorageEndpoint};
use std::sync::Arc;
#[tokio::test]
async fn mcs_044_behavior() {
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
    m.advance_txn_safe_point(NULL_KEYSPACE_ID, 10, BarrierTime::from_unix(1, 0))
        .await
        .unwrap();
    assert!(
        m.advance_txn_safe_point(NULL_KEYSPACE_ID, 9, BarrierTime::from_unix(1, 0))
            .await
            .is_err(),
        "txn safe point must never decrease"
    );
    assert_eq!(
        m.get_gc_state(NULL_KEYSPACE_ID, true)
            .await
            .unwrap()
            .txn_safe_point,
        10
    );
}

use astersql_pd_storage_domain::{Base, Consistency, GCBarrier, GlobalGCBarrier, Read};
use std::time::Duration;
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
async fn seed(m: &GCStateManager, txn: u64, gc: u64) {
    let mut b = m
        .storage
        .begin_gc_state_transaction(&m.paths)
        .await
        .unwrap();
    b.batch.set_txn_safe_point(NULL_KEYSPACE_ID, txn);
    b.batch.set_gc_safe_point(NULL_KEYSPACE_ID, gc);
    b.commit().await.unwrap();
}
#[tokio::test]
async fn mcs_044_blockers_preserve_ties_downgraded_values_and_raw_owner() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store.clone()).await;
    seed(&m, 10, 5).await;
    let mut txn = m
        .storage
        .begin_gc_state_transaction(&m.paths)
        .await
        .unwrap();
    for id in ["a", "b"] {
        txn.batch
            .set_gc_barrier(NULL_KEYSPACE_ID, &GCBarrier::new(id, 20, None))
            .unwrap();
    }
    txn.batch
        .set_global_gc_barrier(&GlobalGCBarrier::new("global", 20, None))
        .unwrap();
    txn.commit().await.unwrap();
    let owner = [
        m.paths
            .compatible_tidb_min_start_ts_prefix(NULL_KEYSPACE_ID)
            .into_bytes(),
        b"server\xff".to_vec(),
    ]
    .concat();
    Base::new(store.as_ref()).save(&owner, b"20").await.unwrap();
    let result = m
        .advance_txn_safe_point(NULL_KEYSPACE_ID, 30, BarrierTime::from_unix(100, 0))
        .await
        .unwrap();
    assert_eq!(result.new_txn_safe_point, 20);
    assert!(
        result
            .blocker_description
            .starts_with("GCBarrier { BarrierID: \"a\"")
    );
    assert_eq!(result.simulated_service_id, b"a");
    Base::new(store.as_ref()).save(&owner, b"15").await.unwrap();
    let result = m
        .advance_txn_safe_point(NULL_KEYSPACE_ID, 30, BarrierTime::from_unix(100, 0))
        .await
        .unwrap();
    assert_eq!(result.new_txn_safe_point, 20);
    assert_eq!(
        result.blocker_description,
        "TiDBMinStartTS { Key: \"server\\xff\", MinStartTS: 20 }"
    );
    assert_eq!(result.simulated_service_id, b"tidb_min_start_ts_server\xff");
    let mut txn = m
        .storage
        .begin_gc_state_transaction(&m.paths)
        .await
        .unwrap();
    txn.batch
        .set_global_gc_barrier(&GlobalGCBarrier::new("最小", 9, None))
        .unwrap();
    txn.commit().await.unwrap();
    let result = m
        .advance_txn_safe_point(NULL_KEYSPACE_ID, 30, BarrierTime::from_unix(100, 0))
        .await
        .unwrap();
    assert_eq!(result.new_txn_safe_point, 20);
    assert_eq!(
        result.blocker_description,
        "GlobalGCBarrier { BarrierID: \"\\u6700\\u5c0f\", BarrierTS: 9, ExpirationTime: \"<nil>\" }"
    );
}
#[tokio::test]
async fn mcs_044_expiry_boundary_worker_rewrite_and_cleanup_are_atomic() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store.clone()).await;
    seed(&m, 10, 5).await;
    let mut time = BarrierTime::from_unix(100, 0);
    let mut txn = m
        .storage
        .begin_gc_state_transaction(&m.paths)
        .await
        .unwrap();
    txn.batch
        .set_gc_barrier(
            NULL_KEYSPACE_ID,
            &GCBarrier::new("local", 20, Some(&mut time)),
        )
        .unwrap();
    let mut time = BarrierTime::from_unix(100, 0);
    txn.batch
        .set_global_gc_barrier(&GlobalGCBarrier::new("global", 25, Some(&mut time)))
        .unwrap();
    txn.batch
        .set_gc_barrier(
            NULL_KEYSPACE_ID,
            &GCBarrier::new("gc_worker", 1, Some(&mut time)),
        )
        .unwrap();
    txn.commit().await.unwrap();
    let r = m
        .advance_txn_safe_point(NULL_KEYSPACE_ID, 40, BarrierTime::from_unix(100, 0))
        .await
        .unwrap();
    assert_eq!(r.new_txn_safe_point, 20);
    assert!(
        r.blocker_description
            .contains("1970-01-01 00:01:40 +0000 UTC")
    );
    assert!(
        m.storage
            .load_gc_barrier(&m.paths, NULL_KEYSPACE_ID, "local")
            .await
            .unwrap()
            .is_some()
    );
    let before = store
        .read(Read::prefix(b"/", Consistency::Linearizable))
        .await
        .unwrap();
    let mut watch = store
        .watch(b"/".to_vec(), before.revision + 1)
        .await
        .unwrap();
    let r = m
        .advance_txn_safe_point(NULL_KEYSPACE_ID, 40, BarrierTime::from_unix(100, 1))
        .await
        .unwrap();
    assert_eq!(r.new_txn_safe_point, 40);
    let wanted = [
        m.paths.gc_barrier_path(NULL_KEYSPACE_ID, "local"),
        m.paths.global_gc_barrier_path("global"),
        m.paths.gc_barrier_path(NULL_KEYSPACE_ID, "gc_worker"),
        m.paths.txn_safe_point_path(NULL_KEYSPACE_ID),
        m.paths.gc_state_revision_path(),
    ];
    let events = tokio::time::timeout(
        Duration::from_secs(2),
        watch.next(&CancellationToken::new()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(events.events.len(), 5);
    assert!(
        events
            .events
            .iter()
            .all(|e| wanted.iter().any(|key| key.as_bytes() == e.key()))
    );
    let rev = events.events[0].revision();
    assert!(events.events.iter().all(|e| e.revision() == rev));
    let worker = m
        .storage
        .load_gc_barrier(&m.paths, NULL_KEYSPACE_ID, "gc_worker")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(worker.barrier_ts, 40);
    assert!(worker.expiration_time.is_none());
}
#[tokio::test]
async fn mcs_044_cas_conflict_keeps_all_staged_effects_and_invalidates_warm_cache() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store.clone()).await;
    seed(&m, 10, 5).await;
    let role = m.on_node_becomes_leader().await;
    assert_eq!(
        m.get_gc_state(NULL_KEYSPACE_ID, true)
            .await
            .unwrap()
            .txn_safe_point,
        10
    );
    let mut t = BarrierTime::from_unix(1, 0);
    let mut tx = m
        .storage
        .begin_gc_state_transaction(&m.paths)
        .await
        .unwrap();
    tx.batch
        .set_gc_barrier(
            NULL_KEYSPACE_ID,
            &GCBarrier::new("expired", 12, Some(&mut t)),
        )
        .unwrap();
    tx.commit().await.unwrap();
    let prepared = m
        .prepare_advance_txn_safe_point(
            NULL_KEYSPACE_ID,
            30,
            BarrierTime::from_unix(100, 0),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    seed(&m, 40, 35).await;
    assert!(
        matches!(prepared.commit().await,Err(GCReadError::Transaction(ref e))if matches!(e.as_ref(),astersql_pd_storage_domain::GCStateTxnError::Conflict))
    );
    assert!(
        m.storage
            .load_gc_barrier(&m.paths, NULL_KEYSPACE_ID, "expired")
            .await
            .unwrap()
            .is_some()
    );
    let state = m.get_gc_state(NULL_KEYSPACE_ID, true).await.unwrap();
    assert_eq!((state.txn_safe_point, state.gc_safe_point), (40, 35));
    role.shutdown().await;
}
#[tokio::test]
async fn mcs_044_stage_error_rolls_back_lazy_deletion_and_repair_reloads() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store.clone()).await;
    seed(&m, 10, 5).await;
    let role = m.on_node_becomes_leader().await;
    m.get_gc_state(NULL_KEYSPACE_ID, true).await.unwrap();
    let mut t = BarrierTime::from_unix(1, 0);
    let mut tx = m
        .storage
        .begin_gc_state_transaction(&m.paths)
        .await
        .unwrap();
    tx.batch
        .set_gc_barrier(
            NULL_KEYSPACE_ID,
            &GCBarrier::new("expired", 12, Some(&mut t)),
        )
        .unwrap();
    tx.commit().await.unwrap();
    let min = format!(
        "{}server",
        m.paths
            .compatible_tidb_min_start_ts_prefix(NULL_KEYSPACE_ID)
    );
    Base::new(store.as_ref())
        .save(min.as_bytes(), b"bad")
        .await
        .unwrap();
    let before = store
        .read(Read::prefix(b"/", Consistency::Linearizable))
        .await
        .unwrap();
    assert!(
        m.advance_txn_safe_point(NULL_KEYSPACE_ID, 30, BarrierTime::from_unix(100, 0))
            .await
            .is_err()
    );
    assert_eq!(
        before.entries,
        store
            .read(Read::prefix(b"/", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
    );
    Base::new(store.as_ref())
        .save(min.as_bytes(), b"0")
        .await
        .unwrap();
    assert_eq!(
        m.advance_txn_safe_point(NULL_KEYSPACE_ID, 30, BarrierTime::from_unix(100, 0))
            .await
            .unwrap()
            .new_txn_safe_point,
        30
    );
    Base::new(store.as_ref())
        .save(
            m.paths.gc_safe_point_path(NULL_KEYSPACE_ID).as_bytes(),
            b"not-hex",
        )
        .await
        .unwrap();
    assert!(
        matches!(
            m.advance_txn_safe_point(NULL_KEYSPACE_ID, 29, BarrierTime::from_unix(100, 0))
                .await,
            Err(GCReadError::SafePoint(_))
        ),
        "GC decode failure precedes decreasing-target validation"
    );
    role.shutdown().await;
}
#[tokio::test]
async fn mcs_044_stopped_commit_and_wal_retry_do_not_publish_partial_advancement() {
    let mut f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store).await;
    seed(&m, 10, 5).await;
    let mut time = BarrierTime::from_unix(1, 0);
    let mut tx = m
        .storage
        .begin_gc_state_transaction(&m.paths)
        .await
        .unwrap();
    tx.batch
        .set_gc_barrier(
            NULL_KEYSPACE_ID,
            &GCBarrier::new("expired", 20, Some(&mut time)),
        )
        .unwrap();
    tx.commit().await.unwrap();
    let prepared = m
        .prepare_advance_txn_safe_point(
            NULL_KEYSPACE_ID,
            30,
            BarrierTime::from_unix(100, 0),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    f.command("stop", "stopped");
    assert!(prepared.commit().await.is_err());
    f.command("start", "restarted");
    let fresh = manager(Arc::new(f.store().await)).await;
    assert_eq!(
        fresh
            .get_gc_state(NULL_KEYSPACE_ID, true)
            .await
            .unwrap()
            .txn_safe_point,
        10
    );
    assert!(
        fresh
            .storage
            .load_gc_barrier(&fresh.paths, NULL_KEYSPACE_ID, "expired")
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        fresh
            .advance_txn_safe_point(NULL_KEYSPACE_ID, 30, BarrierTime::from_unix(100, 0))
            .await
            .unwrap()
            .new_txn_safe_point,
        30
    );
    assert!(
        fresh
            .storage
            .load_gc_barrier(&fresh.paths, NULL_KEYSPACE_ID, "expired")
            .await
            .unwrap()
            .is_none()
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_044_real_committed_response_cancel_invalidates_cache_and_retry_reads_outcome() {
    use std::sync::atomic::Ordering;
    let f = etcd_fixture::Fixture::start();
    let direct: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let proxy = crate::response_gate_test::ResponseGate::start(&f.endpoint).await;
    let proxied: Arc<dyn MetaStore> = Arc::new(
        astersql_pd_storage_kv::EtcdMetaStore::connect(
            &[proxy.endpoint.clone()],
            Duration::from_secs(3),
        )
        .await
        .unwrap(),
    );
    let m = manager(proxied).await;
    seed(&m, 10, 5).await;
    let role = m.on_node_becomes_leader().await;
    m.get_gc_state(NULL_KEYSPACE_ID, true).await.unwrap();
    let cancel = CancellationToken::new();
    let prepared = m
        .prepare_advance_txn_safe_point(
            NULL_KEYSPACE_ID,
            30,
            BarrierTime::from_unix(100, 0),
            &cancel,
        )
        .await
        .unwrap();
    let before = direct
        .read(Read::prefix(b"/", Consistency::Linearizable))
        .await
        .unwrap();
    let mut watch = direct
        .watch(
            m.paths.txn_safe_point_path(NULL_KEYSPACE_ID).into_bytes(),
            before.revision + 1,
        )
        .await
        .unwrap();
    proxy.armed.store(true, Ordering::SeqCst);
    let commit = prepared.commit();
    tokio::pin!(commit);
    tokio::select! {result=&mut commit=>panic!("commit escaped held response: {result:?}"),held=tokio::time::timeout(Duration::from_secs(2),proxy.held.notified())=>{held.expect("real commit response must reach the gate");}};
    let event = tokio::time::timeout(
        Duration::from_secs(2),
        watch.next(&CancellationToken::new()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(event.events.len(), 1);
    assert_eq!(
        Base::new(direct.as_ref())
            .load(m.paths.txn_safe_point_path(NULL_KEYSPACE_ID).as_bytes())
            .await
            .unwrap(),
        b"30"
    );
    cancel.cancel();
    assert!(matches!(commit.await, Err(GCReadError::Cancelled)));
    proxy.release();
    assert_eq!(
        m.get_gc_state(NULL_KEYSPACE_ID, true)
            .await
            .unwrap()
            .txn_safe_point,
        30
    );
    role.shutdown().await;
    proxy.close().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_044_two_failed_writers_force_fresh_direct_read_without_cache_dependency() {
    use std::{
        future::{Future, poll_fn},
        task::Poll,
    };
    let mut f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store).await;
    seed(&m, 10, 5).await;
    let role = m.on_node_becomes_leader().await;
    let mut read = Box::pin(m.get_gc_state(NULL_KEYSPACE_ID, true));
    for _ in 0..2 {
        assert!(
            poll_fn(|cx| Poll::Ready(read.as_mut().poll(cx)))
                .await
                .is_pending()
        );
        assert_eq!(
            m.get_gc_state(NULL_KEYSPACE_ID, true)
                .await
                .unwrap()
                .txn_safe_point,
            10
        );
        assert!(matches!(
            m.advance_txn_safe_point(NULL_KEYSPACE_ID, 9, BarrierTime::from_unix(100, 0))
                .await,
            Err(GCReadError::DecreasingTxnSafePoint { .. })
        ));
    }
    assert_eq!(read.await.unwrap().txn_safe_point, 10);
    f.command("stop", "stopped");
    assert!(
        m.get_gc_state(NULL_KEYSPACE_ID, true).await.is_err(),
        "direct fallback returns its snapshot without requiring or filling cache"
    );
    role.shutdown().await;
}

#[tokio::test]
async fn mcs_044_independent_managers_cas_and_u64_boundary_preserve_monotonicity() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let a = manager(store.clone()).await;
    let b = manager(store).await;
    let cancel = CancellationToken::new();
    let now = BarrierTime::from_unix(100, 0);
    let left = a
        .prepare_advance_txn_safe_point(NULL_KEYSPACE_ID, 30, now, &cancel)
        .await
        .unwrap();
    let right = b
        .prepare_advance_txn_safe_point(NULL_KEYSPACE_ID, 40, now, &cancel)
        .await
        .unwrap();
    let (l, r) = tokio::join!(left.commit(), right.commit());
    assert_ne!(l.is_ok(), r.is_ok());
    let winner = if l.is_ok() { 30 } else { 40 };
    assert_eq!(
        a.get_gc_state(NULL_KEYSPACE_ID, true)
            .await
            .unwrap()
            .txn_safe_point,
        winner
    );
    let result = b
        .advance_txn_safe_point(NULL_KEYSPACE_ID, u64::MAX, now)
        .await
        .unwrap();
    assert_eq!(result.old_txn_safe_point, winner);
    assert_eq!(result.new_txn_safe_point, u64::MAX);
    assert!(matches!(
        a.advance_txn_safe_point(NULL_KEYSPACE_ID, u64::MAX - 1, now)
            .await,
        Err(GCReadError::DecreasingTxnSafePoint {
            current: u64::MAX,
            ..
        })
    ));
    assert_eq!(
        a.get_gc_state(NULL_KEYSPACE_ID, true)
            .await
            .unwrap()
            .txn_safe_point,
        u64::MAX
    );
}
