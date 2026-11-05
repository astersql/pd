// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
use astersql_pd_keyspace::KeyspaceManager;
use astersql_pd_storage_domain::{BarrierTime, MetaStore, StorageEndpoint};
use std::sync::Arc;
#[tokio::test]
async fn mcs_047_behavior() {
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
        m.advance_gc_safe_point(NULL_KEYSPACE_ID, 11).await.is_err(),
        "GC safe point must never exceed the transaction safe point"
    );
}

use astersql_pd_storage_domain::{Base, Consistency, GCBarrier, Keyspace, KeyspaceIdentity, Read};
use astersql_pd_typeutil::Duration;
use std::collections::HashMap;
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
async fn seed(m: &GCStateManager, id: u32, txn: u64, gc: u64) {
    let mut b = m
        .storage
        .begin_gc_state_transaction(&m.paths)
        .await
        .unwrap();
    b.batch.set_txn_safe_point(id, txn);
    b.batch.set_gc_safe_point(id, gc);
    b.commit().await.unwrap();
}
#[tokio::test]
async fn mcs_047_strict_and_compatible_bounds_writes_and_legacy_inconsistency() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store.clone()).await;
    seed(&m, NULL_KEYSPACE_ID, 50, 10).await;
    let role = m.on_node_becomes_leader().await;
    let before = store
        .read(Read::prefix(b"/", Consistency::Linearizable))
        .await
        .unwrap();
    let result = m
        .compatible_update_gc_safe_point(NULL_KEYSPACE_ID, 0)
        .await
        .unwrap();
    assert_eq!(
        (result.old_gc_safe_point, result.new_gc_safe_point),
        (10, 10)
    );
    assert_eq!(
        before.entries,
        store
            .read(Read::prefix(b"/", Consistency::Linearizable))
            .await
            .unwrap()
            .entries,
        "smaller compatible target is a read-only CAS, not a rewrite"
    );
    assert!(matches!(
        m.advance_gc_safe_point(NULL_KEYSPACE_ID, 9).await,
        Err(GCReadError::DecreasingGCSafePoint {
            current: 10,
            given: 9
        })
    ));
    assert!(matches!(
        m.advance_gc_safe_point(NULL_KEYSPACE_ID, 51).await,
        Err(GCReadError::GCExceedsTxn {
            current: 10,
            given: 51,
            txn: 50
        })
    ));
    assert_eq!(
        m.advance_gc_safe_point(NULL_KEYSPACE_ID, 50)
            .await
            .unwrap()
            .new_gc_safe_point,
        50
    );
    seed(&m, NULL_KEYSPACE_ID, 40, 100).await;
    assert_eq!(
        m.compatible_update_gc_safe_point(NULL_KEYSPACE_ID, 0)
            .await
            .unwrap()
            .new_gc_safe_point,
        100,
        "old inconsistent data still permits compatible lower-target reads"
    );
    assert!(matches!(
        m.compatible_update_gc_safe_point(NULL_KEYSPACE_ID, 100)
            .await,
        Err(GCReadError::GCExceedsTxn { .. })
    ));
    role.shutdown().await;
}
#[tokio::test]
async fn mcs_047_shared_barrier_advancement_and_management_checks() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store).await;
    let now = BarrierTime::from_unix(100, 0);
    let ttl = Duration::from_nanos(i64::MAX);
    m.set_gc_barrier(NULL_KEYSPACE_ID, "local", 20, ttl, now)
        .await
        .unwrap();
    m.set_global_gc_barrier("global", 15, ttl, now)
        .await
        .unwrap();
    assert_eq!(
        m.advance_txn_safe_point(NULL_KEYSPACE_ID, 30, now)
            .await
            .unwrap()
            .new_txn_safe_point,
        15
    );
    assert!(m.advance_gc_safe_point(NULL_KEYSPACE_ID, 16).await.is_err());
    m.advance_gc_safe_point(NULL_KEYSPACE_ID, 15).await.unwrap();
    m.set_global_gc_barrier("global", 25, ttl, now)
        .await
        .unwrap();
    assert_eq!(
        m.advance_txn_safe_point(NULL_KEYSPACE_ID, 30, now)
            .await
            .unwrap()
            .new_txn_safe_point,
        20
    );
    m.advance_gc_safe_point(NULL_KEYSPACE_ID, 20).await.unwrap();
    m.keyspaces
        .save_new(&Keyspace {
            identity: Some(KeyspaceIdentity::Id(3)),
            name: "unified47".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(matches!(
        m.compatible_update_gc_safe_point(3, 0).await,
        Err(GCReadError::InvalidKeyspace { id: 3, .. })
    ));
    assert!(matches!(
        m.advance_gc_safe_point(999, 0).await,
        Err(GCReadError::Keyspace(_))
    ));
    assert_eq!(
        m.compatible_update_gc_safe_point(0x0100_0000, 0)
            .await
            .unwrap()
            .new_gc_safe_point,
        20
    );
}
#[tokio::test]
async fn mcs_047_commit_conflict_invalidation_cancellation_and_same_wal_retry() {
    let mut f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store).await;
    seed(&m, NULL_KEYSPACE_ID, 50, 10).await;
    let role = m.on_node_becomes_leader().await;
    m.get_gc_state(NULL_KEYSPACE_ID, true).await.unwrap();
    let cancel = CancellationToken::new();
    let prepared = m
        .prepare_gc_safe_point(NULL_KEYSPACE_ID, 30, GCSafePointMode::Strict, &cancel)
        .await
        .unwrap();
    seed(&m, NULL_KEYSPACE_ID, 50, 40).await;
    assert!(prepared.commit().await.is_err());
    assert_eq!(
        m.get_gc_state(NULL_KEYSPACE_ID, true)
            .await
            .unwrap()
            .gc_safe_point,
        40
    );
    let token = CancellationToken::new();
    let prepared = m
        .prepare_gc_safe_point(NULL_KEYSPACE_ID, 50, GCSafePointMode::Strict, &token)
        .await
        .unwrap();
    token.cancel();
    assert!(matches!(
        prepared.commit().await,
        Err(GCReadError::Cancelled)
    ));
    let prepared = m
        .prepare_gc_safe_point(NULL_KEYSPACE_ID, 50, GCSafePointMode::Strict, &cancel)
        .await
        .unwrap();
    f.command("stop", "stopped");
    assert!(prepared.commit().await.is_err());
    role.shutdown().await;
    f.command("start", "restarted");
    let recovered = manager(Arc::new(f.store().await)).await;
    assert_eq!(
        recovered
            .compatible_load_gc_safe_point(NULL_KEYSPACE_ID)
            .await
            .unwrap(),
        40
    );
    assert_eq!(
        recovered
            .advance_gc_safe_point(NULL_KEYSPACE_ID, 50)
            .await
            .unwrap()
            .new_gc_safe_point,
        50
    );
}
#[tokio::test]
async fn mcs_047_gc_decode_precedes_txn_decode_and_u64_max_is_supported() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store.clone()).await;
    m.keyspaces
        .save_new(&Keyspace {
            identity: Some(KeyspaceIdentity::Id(2)),
            name: "independent47".into(),
            config: HashMap::from([("gc_management_type".into(), "keyspace_level".into())]),
            ..Default::default()
        })
        .await
        .unwrap();
    Base::new(store.as_ref())
        .save(m.paths.gc_safe_point_path(2).as_bytes(), b"{")
        .await
        .unwrap();
    Base::new(store.as_ref())
        .save(m.paths.txn_safe_point_path(2).as_bytes(), b"bad")
        .await
        .unwrap();
    assert!(
        matches!(m.advance_gc_safe_point(2,1).await,Err(GCReadError::SafePoint(ref error))if matches!(error.as_ref(),astersql_pd_storage_domain::SafePointReadError::JsonUnmarshal(_)))
    );
    seed(&m, 2, u64::MAX, u64::MAX - 1).await;
    assert_eq!(
        m.advance_gc_safe_point(2, u64::MAX)
            .await
            .unwrap()
            .new_gc_safe_point,
        u64::MAX
    );
    assert_eq!(
        m.compatible_update_gc_safe_point(2, 0)
            .await
            .unwrap()
            .new_gc_safe_point,
        u64::MAX
    );
}
#[tokio::test]
async fn mcs_047_legacy_services_share_worker_min_start_and_ttl_state_machine() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store.clone()).await;
    let now = BarrierTime::from_unix(1741584577, 0);
    let result = m
        .compatible_update_service_gc_safe_point(NULL_KEYSPACE_ID, b"svc1", 10, i64::MAX, now)
        .await
        .unwrap();
    assert!(result.updated);
    assert_eq!(result.minimum.safe_point, 0);
    assert_eq!(result.minimum.service_id, b"gc_worker");
    assert_eq!(
        m.compatible_update_service_gc_safe_point(
            NULL_KEYSPACE_ID,
            b"gc_worker",
            15,
            i64::MAX,
            now
        )
        .await
        .unwrap()
        .minimum
        .safe_point,
        10
    );
    m.set_gc_barrier(
        NULL_KEYSPACE_ID,
        "svc1",
        15,
        Duration::from_nanos(3_600_000_000_000),
        now,
    )
    .await
    .unwrap();
    let before = m
        .storage
        .load_gc_barrier(&m.paths, NULL_KEYSPACE_ID, "svc1")
        .await
        .unwrap();
    let denied = m
        .compatible_update_service_gc_safe_point(NULL_KEYSPACE_ID, b"svc1", 8, i64::MAX, now)
        .await
        .unwrap();
    assert!(!denied.updated);
    assert_eq!(denied.minimum.safe_point, 10);
    assert_eq!(
        m.storage
            .load_gc_barrier(&m.paths, NULL_KEYSPACE_ID, "svc1")
            .await
            .unwrap(),
        before
    );
    m.compatible_update_service_gc_safe_point(NULL_KEYSPACE_ID, b"svc1", 12, i64::MAX, now)
        .await
        .unwrap();
    assert!(matches!(
        m.compatible_update_service_gc_safe_point(NULL_KEYSPACE_ID, b"gc_worker", 20, 3600, now)
            .await,
        Err(GCReadError::LegacyWorkerTTL)
    ));
    let result = m
        .compatible_update_service_gc_safe_point(NULL_KEYSPACE_ID, b"gc_worker", 20, i64::MAX, now)
        .await
        .unwrap();
    assert_eq!(result.minimum.service_id, b"svc1");
    assert_eq!(result.minimum.safe_point, 12);
    assert!(result.updated);
    assert!(
        !m.compatible_update_service_gc_safe_point(NULL_KEYSPACE_ID, b"svc1", 12, 0, now)
            .await
            .unwrap()
            .updated
    );
    let key = format!(
        "{}instance1",
        m.paths
            .compatible_tidb_min_start_ts_prefix(NULL_KEYSPACE_ID)
    );
    Base::new(store.as_ref())
        .save(key.as_bytes(), b"14")
        .await
        .unwrap();
    let result = m
        .compatible_update_service_gc_safe_point(NULL_KEYSPACE_ID, b"gc_worker", 20, i64::MAX, now)
        .await
        .unwrap();
    assert_eq!(
        (result.minimum.service_id, result.minimum.safe_point),
        (b"tidb_min_start_ts_instance1".to_vec(), 14)
    );
    Base::new(store.as_ref())
        .remove(key.as_bytes())
        .await
        .unwrap();
    assert_eq!(
        m.compatible_update_service_gc_safe_point(
            NULL_KEYSPACE_ID,
            b"gc_worker",
            20,
            i64::MAX,
            now
        )
        .await
        .unwrap()
        .minimum
        .safe_point,
        20
    );
    let mut tx = m
        .storage
        .begin_gc_state_transaction(&m.paths)
        .await
        .unwrap();
    tx.batch
        .set_gc_barrier(NULL_KEYSPACE_ID, &GCBarrier::new("gc_worker", 1, None))
        .unwrap();
    tx.commit().await.unwrap();
    m.compatible_update_service_gc_safe_point(NULL_KEYSPACE_ID, b"gc_worker", 25, i64::MAX, now)
        .await
        .unwrap();
    assert_eq!(
        m.storage
            .load_gc_barrier(&m.paths, NULL_KEYSPACE_ID, "gc_worker")
            .await
            .unwrap()
            .unwrap()
            .barrier_ts,
        25
    );
}
#[tokio::test]
async fn mcs_047_raw_service_bytes_keep_physical_key_and_go_json_replacement() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store.clone()).await;
    let now = BarrierTime::from_unix(100, 0);
    let id = b"bad\xff\xe2\x82";
    let result = m
        .compatible_update_service_gc_safe_point(NULL_KEYSPACE_ID, id, 20, i64::MAX, now)
        .await
        .unwrap();
    assert_eq!(result.minimum.safe_point, 0);
    let mut key = m.paths.gc_barrier_prefix(NULL_KEYSPACE_ID).into_bytes();
    key.extend(id);
    let bytes = Base::new(store.as_ref()).load(&key).await.unwrap();
    assert_eq!(bytes,br#"{"service_id":"bad\ufffd\ufffd\ufffd","expired_at":9223372036854775807,"safe_point":20}"#);
    let result = m
        .compatible_update_service_gc_safe_point(NULL_KEYSPACE_ID, b"gc_worker", 30, i64::MAX, now)
        .await
        .unwrap();
    assert_eq!(result.minimum.service_id, "bad���".as_bytes());
    assert_eq!(result.minimum.safe_point, 20);
    m.compatible_update_service_gc_safe_point(NULL_KEYSPACE_ID, id, 0, -1, now)
        .await
        .unwrap();
    assert!(
        Base::new(store.as_ref())
            .load(&key)
            .await
            .unwrap()
            .is_empty()
    );
}
#[tokio::test]
async fn mcs_047_native_br_uses_global_max_only_for_effective_null_scope() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store).await;
    let now = BarrierTime::from_unix(100, 0);
    m.keyspaces
        .save_new(&Keyspace {
            identity: Some(KeyspaceIdentity::Id(2)),
            name: "ks2".into(),
            config: HashMap::from([("gc_management_type".into(), "keyspace_level".into())]),
            ..Default::default()
        })
        .await
        .unwrap();
    seed(&m, 2, 40, 10).await;
    seed(&m, NULL_KEYSPACE_ID, 10, 5).await;
    let denied = m
        .compatible_update_service_gc_safe_point(NULL_KEYSPACE_ID, b"native_br", 35, i64::MAX, now)
        .await
        .unwrap();
    assert!(!denied.updated);
    assert_eq!(denied.minimum.safe_point, 40);
    assert!(
        m.storage
            .load_global_gc_barrier(&m.paths, "native_br")
            .await
            .unwrap()
            .is_none()
    );
    let result = m
        .compatible_update_service_gc_safe_point(NULL_KEYSPACE_ID, b"native_br", 40, i64::MAX, now)
        .await
        .unwrap();
    assert!(result.updated);
    assert_eq!(result.minimum.safe_point, 40);
    assert!(
        m.storage
            .load_global_gc_barrier(&m.paths, "native_br")
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        !m.compatible_update_service_gc_safe_point(NULL_KEYSPACE_ID, b"native_br", 0, 0, now)
            .await
            .unwrap()
            .updated
    );
    assert!(
        m.compatible_update_service_gc_safe_point(2, b"native_br", 50, i64::MAX, now)
            .await
            .unwrap()
            .updated
    );
    assert!(
        m.storage
            .load_gc_barrier(&m.paths, 2, "native_br")
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        m.storage
            .load_global_gc_barrier(&m.paths, "native_br")
            .await
            .unwrap()
            .is_none()
    );
}
#[tokio::test]
async fn mcs_047_service_ttl_saturation_and_empty_id_errors_preserve_state() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = manager(store).await;
    let now = BarrierTime::from_unix(100, 1);
    m.compatible_update_service_gc_safe_point(NULL_KEYSPACE_ID, b"svc", 10, 9_223_372_036, now)
        .await
        .unwrap();
    assert!(
        m.storage
            .load_gc_barrier(&m.paths, NULL_KEYSPACE_ID, "svc")
            .await
            .unwrap()
            .unwrap()
            .expiration_time
            .is_some()
    );
    m.compatible_update_service_gc_safe_point(NULL_KEYSPACE_ID, b"svc", 10, 9_223_372_037, now)
        .await
        .unwrap();
    assert!(
        m.storage
            .load_gc_barrier(&m.paths, NULL_KEYSPACE_ID, "svc")
            .await
            .unwrap()
            .unwrap()
            .expiration_time
            .is_none()
    );
    assert!(matches!(
        m.compatible_update_service_gc_safe_point(NULL_KEYSPACE_ID, b"", 10, 1, now)
            .await,
        Err(GCReadError::InvalidArgument {
            argument: "barrierID",
            ..
        })
    ));
    assert!(matches!(
        m.compatible_update_service_gc_safe_point(NULL_KEYSPACE_ID, b"", 10, 0, now)
            .await,
        Err(GCReadError::InvalidArgument {
            argument: "barrierID",
            ..
        })
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_047_service_committed_change_survives_follow_up_io_failure() {
    let mut f = etcd_fixture::Fixture::start();
    let direct: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let proxy = crate::response_gate_test::ResponseGate::start(&f.endpoint).await;
    let through: Arc<dyn MetaStore> = Arc::new(
        astersql_pd_storage_kv::EtcdMetaStore::connect(
            &[proxy.endpoint.clone()],
            std::time::Duration::from_secs(3),
        )
        .await
        .unwrap(),
    );
    let m = manager(through).await;
    seed(&m, NULL_KEYSPACE_ID, 10, 5).await;
    let key = m.paths.gc_barrier_path(NULL_KEYSPACE_ID, "committed");
    let before = direct
        .read(Read::prefix(b"/", Consistency::Linearizable))
        .await
        .unwrap();
    let mut watch = direct
        .watch(key.as_bytes().to_vec(), before.revision + 1)
        .await
        .unwrap();
    proxy.arm_on_key(key.as_bytes());
    let update = m.compatible_update_service_gc_safe_point(
        NULL_KEYSPACE_ID,
        b"committed",
        20,
        i64::MAX,
        BarrierTime::from_unix(100, 0),
    );
    tokio::pin!(update);
    tokio::select! {result=&mut update=>panic!("legacy update escaped real response gate: {result:?}"),held=tokio::time::timeout(std::time::Duration::from_secs(3),proxy.held.notified())=>{held.unwrap();}}
    let event = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        watch.next(&CancellationToken::new()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(event.events.len(), 1);
    f.command("stop", "stopped");
    proxy.release();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), &mut update)
        .await
        .unwrap();
    assert!(result.is_err());
    eprintln!(
        "legacy committed barrier then backend stop before post-write observation completed: {result:?}"
    );
    proxy.close().await;
    f.command("start", "restarted");
    let recovered = manager(Arc::new(f.store().await)).await;
    assert_eq!(
        recovered
            .storage
            .load_gc_barrier(&recovered.paths, NULL_KEYSPACE_ID, "committed")
            .await
            .unwrap()
            .unwrap()
            .barrier_ts,
        20
    );
    assert_eq!(
        recovered
            .get_gc_state(NULL_KEYSPACE_ID, true)
            .await
            .unwrap()
            .txn_safe_point,
        10
    );
}
