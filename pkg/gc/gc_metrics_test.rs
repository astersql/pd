// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_storage_domain::MetaStore;
use std::sync::Arc;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_053_behavior() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = Arc::new(crate::gc_warmup_test::setup(store.clone(), store).await);
    let role = m.on_node_becomes_leader().await;
    let states = m.get_all_keyspaces_gc_states(true).await.unwrap();
    role.shutdown().await;
    assert_eq!(
        states.len(),
        257,
        "GC enumeration must contain all enabled metadata scopes plus Null"
    );
    assert_eq!(
        states[&astersql_pd_keypath::NULL_KEYSPACE_ID].gc_safe_point,
        40
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_053_metrics_producer_behavior() {
    use astersql_pd_storage_domain::BarrierTime;
    use astersql_pd_typeutil::Duration;
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = Arc::new(crate::gc_warmup_test::setup(store.clone(), store).await);
    let role = m.on_node_becomes_leader().await;
    let now = BarrierTime::from_unix(2_000_000_000, 0);
    let registry = prometheus::Registry::new();
    let metrics = GCMetrics::new_with_clock(Arc::new(move || now)).unwrap();
    registry.register(Box::new(metrics.clone())).unwrap();
    m.attach_gc_metrics(metrics).unwrap();
    let ts = ((2_000_000_000_u64 - 48 * 3600) * 1000) << 18;
    m.set_gc_barrier(100, "ticdc", ts, Duration::from_nanos(i64::MAX), now)
        .await
        .unwrap();
    m.advance_txn_safe_point(100, ts + 100, now).await.unwrap();
    let count = registry
        .gather()
        .iter()
        .filter(|f| f.name() == "pd_gc_barrier_timestamp_seconds")
        .map(|f| f.get_metric().len())
        .sum::<usize>();
    role.shutdown().await;
    assert_eq!(
        count, 1,
        "committed advancement must discover the old persisted barrier for collection"
    );
}
#[test]
fn mcs_053_age_warning_expiry_and_identity() {
    use crate::gc_metrics::{BarrierMetricScope as Scope, BarrierMetrics};
    use astersql_pd_storage_domain::{BarrierTime, GCBarrier, GlobalGCBarrier};
    let now = BarrierTime::from_unix(2_000_000_000, 0);
    let ts = |age: i64| ((2_000_000_000_000_i64 - age) as u64) << 18;
    let barrier = |id: &str, age| GCBarrier {
        barrier_id: id.into(),
        barrier_ts: ts(age),
        expiration_time: None,
    };
    for (age, expected) in [
        (86_399_999, false),
        (86_400_000, false),
        (86_400_001, true),
        (172_800_000, true),
        (-3_600_000, false),
    ] {
        assert_eq!(
            BarrierMetrics::candidate(Scope::Keyspace(10), &barrier("x", age), now),
            expected
        );
    }
    let metrics = BarrierMetrics::default();
    let epoch = metrics.epoch();
    let local = barrier("x", 172_800_000);
    let worker = barrier("gc_worker", 172_800_000);
    let global = GlobalGCBarrier {
        barrier_id: worker.barrier_id.clone(),
        barrier_ts: worker.barrier_ts,
        expiration_time: None,
    };
    assert_eq!(
        metrics
            .observe_metrics(
                epoch,
                10,
                "old",
                &[local.clone(), worker],
                &[global.clone()],
                now
            )
            .len(),
        2
    );
    assert_eq!(metrics.snapshot_metrics(now).len(), 2);
    assert!(
        metrics
            .observe_metrics(
                epoch,
                10,
                "new",
                &[local.clone()],
                &[global],
                now.add_nanos(599_000_000_000)
            )
            .is_empty()
    );
    let sample = metrics
        .snapshot_metrics(now)
        .into_iter()
        .find(|m| {
            m.get_label()
                .iter()
                .any(|l| l.name() == "scope" && l.value() == "keyspace")
        })
        .unwrap();
    assert!(
        sample
            .get_label()
            .iter()
            .any(|l| l.name() == "keyspace_name" && l.value() == "new")
    );
    assert_eq!(
        metrics
            .observe_metrics(
                epoch,
                10,
                "new",
                &[local.clone()],
                &[],
                now.add_nanos(600_000_000_000)
            )
            .len(),
        1
    );
    let finite = GCBarrier {
        expiration_time: Some(now.add_nanos(1_000_000_000)),
        ..local.clone()
    };
    metrics.update_metrics(Scope::Keyspace(10), &finite, now);
    assert!(
        metrics
            .snapshot_metrics(now.add_nanos(2_000_000_000))
            .is_empty()
    );
    metrics.update_metrics(Scope::Keyspace(10), &local, now.add_nanos(2_000_000_000));
    assert!(
        metrics.snapshot_metrics(now).is_empty(),
        "a write cannot rediscover a sample"
    );
    metrics.invalidate_keyspace_metrics(10);
    assert!(
        metrics
            .observe_metrics(epoch, 10, "stale", &[local.clone()], &[], now)
            .is_empty()
    );
    assert_eq!(
        metrics
            .observe_metrics(metrics.epoch(), 10, "new", &[local], &[], now)
            .len(),
        1
    );
    metrics.clear_metrics();
    assert!(metrics.snapshot_metrics(now).is_empty());
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_053_list_cache_metadata_errors_and_cleanup() {
    use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
    use astersql_pd_storage_domain::{Base, StorageEndpoint};
    use tokio_util::sync::CancellationToken;
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = Arc::new(crate::gc_warmup_test::setup(store.clone(), store.clone()).await);
    let role = m.on_node_becomes_leader().await;
    assert_eq!(
        m.get_gc_state(NULL_KEYSPACE_ID, true)
            .await
            .unwrap()
            .gc_safe_point,
        40
    );
    let ep = StorageEndpoint::new(store.clone());
    let p = ClusterPaths::new(42);
    let mut tx = ep.begin_gc_state_transaction(&p).await.unwrap();
    tx.batch.set_gc_safe_point(NULL_KEYSPACE_ID, 45);
    tx.commit().await.unwrap();
    assert_eq!(
        m.get_all_keyspaces_gc_states(true).await.unwrap()[&NULL_KEYSPACE_ID].gc_safe_point,
        40
    );
    assert_eq!(
        m.get_all_keyspaces_gc_states(false).await.unwrap()[&NULL_KEYSPACE_ID].gc_safe_point,
        45
    );
    m.get_gc_state(100, true).await.unwrap();
    m.keyspaces.update_state_by_id(100, 1, 1).await.unwrap();
    assert!(
        !m.get_all_keyspaces_gc_states(true)
            .await
            .unwrap()
            .contains_key(&100)
    );
    assert!(!m.shared.cache.read().unwrap().contains_key(&100));
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(matches!(
        m.get_all_keyspaces_gc_states_with_cancel(true, &cancel)
            .await,
        Err(GCReadError::Cancelled)
    ));
    // Malformed persisted transaction safe point must fail the complete cold enumeration.
    let key = p.txn_safe_point_path(101);
    let mut tx = Base::new(store.as_ref()).begin_txn();
    tx.save(key.as_bytes(), b"bad").unwrap();
    tx.commit::<String>().await.unwrap();
    assert!(m.get_all_keyspaces_gc_states(false).await.is_err());
    role.shutdown().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_053_metrics_writes_deletion_and_generations() {
    use astersql_pd_storage_domain::BarrierTime;
    use astersql_pd_typeutil::Duration;
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = Arc::new(crate::gc_warmup_test::setup(store.clone(), store.clone()).await);
    let role = m.on_node_becomes_leader().await;
    let now = BarrierTime::from_unix(2_000_000_000, 0);
    let ts = ((2_000_000_000_u64 - 172800) * 1000) << 18;
    let registry = prometheus::Registry::new();
    let metrics = GCMetrics::new_with_clock(Arc::new(move || now)).unwrap();
    registry.register(Box::new(metrics.clone())).unwrap();
    m.attach_gc_metrics(metrics.clone()).unwrap();
    let count = || {
        registry
            .gather()
            .iter()
            .filter(|f| f.name() == "pd_gc_barrier_timestamp_seconds")
            .map(|f| f.get_metric().len())
            .sum::<usize>()
    };
    m.set_gc_barrier(100, "x", ts, Duration::from_nanos(i64::MAX), now)
        .await
        .unwrap();
    assert_eq!(count(), 0);
    m.advance_txn_safe_point(100, ts + 100, now).await.unwrap();
    assert_eq!(count(), 1);
    m.delete_gc_barrier(100, "x").await.unwrap();
    assert_eq!(count(), 0);
    m.set_gc_barrier(100, "x", ts + 100, Duration::from_nanos(i64::MAX), now)
        .await
        .unwrap();
    assert_eq!(count(), 0);
    m.advance_txn_safe_point(100, ts + 100, now).await.unwrap();
    assert_eq!(count(), 1);
    m.keyspaces.update_state_by_id(100, 1, 1).await.unwrap();
    m.keyspaces.update_state_by_id(100, 2, 2).await.unwrap();
    m.keyspaces.remove(100).await.unwrap();
    assert_eq!(count(), 0);
    m.set_gc_barrier(101, "x", ts, Duration::from_nanos(i64::MAX), now)
        .await
        .unwrap();
    m.advance_txn_safe_point(101, ts + 100, now).await.unwrap();
    assert_eq!(count(), 1);
    let next = m.on_node_becomes_leader().await;
    assert_eq!(count(), 0);
    m.advance_txn_safe_point(101, ts + 100, now).await.unwrap();
    assert_eq!(count(), 1);
    role.shutdown().await;
    assert_eq!(
        count(),
        1,
        "late prior-generation cleanup must preserve current samples"
    );
    next.shutdown().await;
    assert_eq!(count(), 0);
    drop(m);
    assert_eq!(count(), 0);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_053_fresh_invocation_and_cancellation() {
    use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
    use astersql_pd_storage_domain::StorageEndpoint;
    use tokio_util::sync::CancellationToken;
    let f = etcd_fixture::Fixture::start();
    let direct: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let gate = crate::response_gate_test::ResponseGate::start(&f.endpoint).await;
    let reader: Arc<dyn MetaStore> = Arc::new(
        astersql_pd_storage_kv::EtcdMetaStore::connect(
            &[gate.endpoint.clone()],
            std::time::Duration::from_secs(10),
        )
        .await
        .unwrap(),
    );
    let manager = Arc::new(crate::gc_warmup_test::setup(direct.clone(), reader).await);
    let role = manager.on_node_becomes_leader().await;
    let p = ClusterPaths::new(42);
    gate.arm_on_key(p.txn_safe_point_path(NULL_KEYSPACE_ID).as_bytes());
    let first = {
        let m = manager.clone();
        tokio::spawn(async move { m.get_all_keyspaces_gc_states(false).await })
    };
    tokio::time::timeout(std::time::Duration::from_secs(3), gate.held.notified())
        .await
        .unwrap();
    let ep = StorageEndpoint::new(direct);
    let mut tx = ep.begin_gc_state_transaction(&p).await.unwrap();
    tx.batch.set_txn_safe_point(NULL_KEYSPACE_ID, 60);
    tx.commit().await.unwrap();
    let second = {
        let m = manager.clone();
        tokio::spawn(async move { m.get_all_keyspaces_gc_states(false).await })
    };
    tokio::task::yield_now().await;
    gate.release();
    assert_eq!(
        first.await.unwrap().unwrap_err().to_string(),
        "[PD:etcd:ErrEtcdTxnConflict]etcd transaction failed, conflicted and rolled back"
    );
    assert_eq!(
        second.await.unwrap().unwrap()[&NULL_KEYSPACE_ID].txn_safe_point,
        60
    );
    gate.arm_on_key(p.txn_safe_point_path(NULL_KEYSPACE_ID).as_bytes());
    let cancel = CancellationToken::new();
    let waiter = {
        let m = manager.clone();
        let c = cancel.clone();
        tokio::spawn(async move { m.get_all_keyspaces_gc_states_with_cancel(false, &c).await })
    };
    tokio::time::timeout(std::time::Duration::from_secs(3), gate.held.notified())
        .await
        .unwrap();
    cancel.cancel();
    assert!(matches!(waiter.await.unwrap(), Err(GCReadError::Cancelled)));
    tokio::time::timeout(std::time::Duration::from_secs(3), role.shutdown())
        .await
        .unwrap();
    gate.close().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_053_cache_counters_and_compatible_gauge() {
    use astersql_pd_keypath::NULL_KEYSPACE_ID;
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = Arc::new(crate::gc_warmup_test::setup(store.clone(), store).await);
    let role = m.on_node_becomes_leader().await;
    let registry = prometheus::Registry::new();
    let metrics = GCMetrics::register(&registry).unwrap();
    m.attach_gc_metrics(metrics).unwrap();
    m.get_gc_state(NULL_KEYSPACE_ID, true).await.unwrap();
    m.get_gc_state(NULL_KEYSPACE_ID, true).await.unwrap();
    m.warmup_batch(
        [NULL_KEYSPACE_ID].into_iter(),
        &tokio_util::sync::CancellationToken::new(),
    )
    .await
    .unwrap();
    let families = registry.gather();
    let samples = families
        .iter()
        .find(|f| f.name() == "pd_gc_gc_state_cache_access_total")
        .unwrap();
    for name in ["hit", "slow_hit", "miss"] {
        let value = samples
            .get_metric()
            .iter()
            .find(|m| m.get_label()[0].value() == name)
            .unwrap()
            .get_counter()
            .get_value();
        assert_eq!(value, 1.0, "{name}");
    }
    assert!(!families.iter().any(|f| f.name() == "pd_gc_gc_safepoint"));
    assert_eq!(
        m.compatible_update_gc_safe_point(NULL_KEYSPACE_ID, 20)
            .await
            .unwrap()
            .new_gc_safe_point,
        40
    );
    assert!(
        m.compatible_update_gc_safe_point(NULL_KEYSPACE_ID, 51)
            .await
            .is_err()
    );
    let families = registry.gather();
    assert_eq!(
        families
            .iter()
            .find(|f| f.name() == "pd_gc_gc_safepoint")
            .unwrap()
            .get_metric()[0]
            .get_gauge()
            .get_value(),
        20.0
    );
    role.shutdown().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_053_failed_transaction_and_scrape_without_storage() {
    use astersql_pd_keypath::ClusterPaths;
    use astersql_pd_storage_domain::{BarrierTime, Base};
    use astersql_pd_typeutil::Duration;
    let mut f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = Arc::new(crate::gc_warmup_test::setup(store.clone(), store.clone()).await);
    let role = m.on_node_becomes_leader().await;
    let now = BarrierTime::from_unix(2_000_000_000, 0);
    let ts = ((2_000_000_000_u64 - 172800) * 1000) << 18;
    let registry = prometheus::Registry::new();
    let metrics = GCMetrics::new_with_clock(Arc::new(move || now)).unwrap();
    registry.register(Box::new(metrics.clone())).unwrap();
    m.attach_gc_metrics(metrics).unwrap();
    m.set_gc_barrier(100, "x", ts, Duration::from_nanos(i64::MAX), now)
        .await
        .unwrap();
    m.advance_txn_safe_point(100, ts + 100, now).await.unwrap();
    let prepared = m
        .prepare_delete_gc_barrier(100, "x", &tokio_util::sync::CancellationToken::new())
        .await
        .unwrap();
    let p = ClusterPaths::new(42);
    let key = p.gc_state_revision_path();
    let mut tx = Base::new(store.as_ref()).begin_txn();
    tx.save(key.as_bytes(), b"9999").unwrap();
    tx.commit::<String>().await.unwrap();
    assert!(prepared.commit().await.is_err());
    f.command("stop", "stopped");
    assert_eq!(
        registry
            .gather()
            .iter()
            .filter(|f| f.name() == "pd_gc_barrier_timestamp_seconds")
            .map(|f| f.get_metric().len())
            .sum::<usize>(),
        1,
        "failed storage transaction cannot remove observed metrics; scrape has no storage dependency"
    );
    role.shutdown().await;
}
