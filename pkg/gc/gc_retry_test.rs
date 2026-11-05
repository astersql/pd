// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{Base, MetaStore};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_051_behavior() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let gate = response_gate_test::ResponseGate::start(&f.endpoint).await;
    let reader: Arc<dyn MetaStore> = Arc::new(
        astersql_pd_storage_kv::EtcdMetaStore::connect(
            &[gate.endpoint.clone()],
            Duration::from_secs(2),
        )
        .await
        .unwrap(),
    );
    let m = Arc::new(crate::gc_warmup_test::setup(store.clone(), reader).await);
    let role = m.on_node_becomes_leader().await;
    let key = ClusterPaths::new(42).txn_safe_point_path(100);
    Base::new(store.as_ref())
        .save(key.as_bytes(), b"malformed")
        .await
        .unwrap();
    gate.arm_on_key(key.as_bytes());
    let warmup = Arc::new(GCWarmup::new(m.clone(), None, 4).unwrap());
    let entry = EnabledKeyspace {
        id: 100,
        gc_management_type: "keyspace_level".into(),
    };
    warmup.on_page(vec![entry.clone()]);
    warmup.on_initial_snapshot(EnabledSnapshot {
        entries: std::collections::BTreeMap::from([(100, entry)]),
        revision: 1,
    });
    let cancel = CancellationToken::new();
    let mut work = {
        let w = warmup.clone();
        let c = cancel.clone();
        tokio::spawn(async move { w.run(&c).await })
    };
    tokio::time::timeout(Duration::from_secs(3), gate.held.notified())
        .await
        .unwrap();
    // The held response contains the actual old malformed bytes; repair only affects the retry.
    Base::new(store.as_ref())
        .save(key.as_bytes(), b"55")
        .await
        .unwrap();
    gate.release();
    for _ in 0..20 {
        warmup.on_page(vec![EnabledKeyspace {
            id: 100,
            gc_management_type: "keyspace_level".into(),
        }]);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        !work.is_finished(),
        "duplicate metadata hints must not bypass the retry deadline"
    );
    assert!(!m.shared.cache.read().unwrap().contains_key(&100));
    let result = tokio::time::timeout(Duration::from_secs(5), &mut work).await;
    let timed_out = result.is_err();
    let succeeded = matches!(result, Ok(Ok(Ok(()))));
    cancel.cancel();
    if timed_out {
        work.await.unwrap().unwrap();
    }
    let point = m.shared.cache.read().unwrap().get(&100).copied();
    gate.close().await;
    role.shutdown().await;
    assert!(
        succeeded,
        "a repaired physical GC scope must be retried to campaign completion"
    );
    assert_eq!(point, Some((55, 0)));
}
#[test]
fn mcs_051_deadline_order_backoff_and_bounded_promotion() {
    use crate::gc_retry::{GCRetryQueue, next_retry_delay};
    use std::time::Instant;
    let now = Instant::now();
    let mut queue = GCRetryQueue::default();
    for (id, delay) in [(100, 30), (101, 2), (102, 1)] {
        queue.schedule(id, now + Duration::from_secs(delay));
    }
    for (delay, id) in [(1, 102), (2, 101), (30, 100)] {
        let deadline = now + Duration::from_secs(delay);
        assert_eq!(queue.deadline(), Some(deadline));
        assert!(
            queue
                .pop_due(deadline - Duration::from_nanos(1), 60)
                .is_empty()
        );
        assert_eq!(queue.pop_due(deadline, 60), vec![id]);
    }
    assert!(queue.deadline().is_none());
    let mut delay = Duration::ZERO;
    for seconds in [1, 2, 4, 8, 16, 30, 30] {
        delay = next_retry_delay(delay);
        assert_eq!(delay, Duration::from_secs(seconds));
    }
    for id in 0..20000 {
        queue.schedule(id, now);
    }
    queue.schedule(1, now);
    assert_eq!(queue.pop_due(now, 60).len(), 60);
    assert_eq!(queue.pop_due(now, 60).len(), 60);
    queue.retain(|id| id >= 19900);
    assert_eq!(queue.pop_due(now, usize::MAX).len(), 100);
    assert!(queue.deadline().is_none());
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_051_live_index_retires_obsolete_corrupt_target() {
    use astersql_pd_storage_domain::{Consistency, Keyspace, KeyspaceIdentity, Read};
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = Arc::new(crate::gc_warmup_test::setup(store.clone(), store.clone()).await);
    let role = m.on_node_becomes_leader().await;
    let paths = ClusterPaths::new(42);
    let keys = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone())
        .keyspace_storage(paths.clone());
    let bad = paths.txn_safe_point_path(100);
    Base::new(store.as_ref())
        .save(bad.as_bytes(), b"malformed")
        .await
        .unwrap();
    let index = Arc::new(EnabledKeyspaceIndex::new(EnabledSnapshotLoader {
        storage: keys.clone(),
        concurrency: 2,
    }));
    let stop = CancellationToken::new();
    let index_task = {
        let i = index.clone();
        let c = stop.clone();
        tokio::spawn(async move { i.run(&c).await })
    };
    let snapshot = tokio::time::timeout(Duration::from_secs(4), index.snapshot_at_least(1, &stop))
        .await
        .unwrap()
        .unwrap();
    let warmup = Arc::new(GCWarmup::new(m.clone(), Some(index.clone()), 4).unwrap());
    warmup.on_initial_snapshot(snapshot);
    let work = {
        let w = warmup.clone();
        let c = stop.clone();
        tokio::spawn(async move { w.run(&c).await })
    };
    let mut tx = Base::new(store.as_ref()).begin_txn();
    keys.save_keyspace_meta(
        &mut tx,
        &paths,
        &Keyspace {
            identity: Some(KeyspaceIdentity::Id(100)),
            name: "ks100".into(),
            state: 1,
            config: std::collections::HashMap::from([(
                "gc_management_type".into(),
                "keyspace_level".into(),
            )]),
            ..Default::default()
        },
    )
    .unwrap();
    tx.commit::<String>().await.unwrap();
    let revision = store
        .read(Read::prefix(b"", Consistency::Linearizable))
        .await
        .unwrap()
        .revision;
    tokio::time::timeout(
        Duration::from_secs(4),
        index.snapshot_at_least(revision, &stop),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(index.can_skip_gc_state_warmup(100));
    tokio::time::timeout(Duration::from_secs(8), work)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(!m.shared.cache.read().unwrap().contains_key(&100));
    assert_eq!(
        Base::new(store.as_ref())
            .load(bad.as_bytes())
            .await
            .unwrap(),
        b"malformed"
    );
    stop.cancel();
    index_task.await.unwrap();
    role.shutdown().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_051_cancelled_campaign_does_not_retry_repaired_scope() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = Arc::new(crate::gc_warmup_test::setup(store.clone(), store.clone()).await);
    let role = m.on_node_becomes_leader().await;
    let key = ClusterPaths::new(42).txn_safe_point_path(100);
    Base::new(store.as_ref())
        .save(key.as_bytes(), b"malformed")
        .await
        .unwrap();
    let warmup = Arc::new(GCWarmup::new(m.clone(), None, 4).unwrap());
    warmup.on_initial_snapshot(EnabledSnapshot {
        entries: std::collections::BTreeMap::from([(
            100,
            EnabledKeyspace {
                id: 100,
                gc_management_type: "keyspace_level".into(),
            },
        )]),
        revision: 1,
    });
    let stop = CancellationToken::new();
    let work = {
        let w = warmup.clone();
        let c = stop.clone();
        tokio::spawn(async move { w.run(&c).await })
    };
    tokio::time::sleep(Duration::from_millis(200)).await;
    stop.cancel();
    work.await.unwrap().unwrap();
    assert!(
        warmup
            .wait_finished(&CancellationToken::new())
            .await
            .is_err()
    );
    Base::new(store.as_ref())
        .save(key.as_bytes(), b"55")
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert!(!m.shared.cache.read().unwrap().contains_key(&100));
    assert!(m.generation().unwrap().flights.lock().unwrap().is_empty());
    assert_eq!(m.get_gc_state(100, true).await.unwrap().txn_safe_point, 55);
    role.shutdown().await;
}
