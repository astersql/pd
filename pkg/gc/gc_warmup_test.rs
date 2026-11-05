// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
use astersql_pd_storage_domain::{Base, Keyspace, KeyspaceIdentity, MetaStore, StorageEndpoint};
use std::{collections::HashMap, sync::Arc};
use tokio_util::sync::CancellationToken;
pub(crate) async fn setup(store: Arc<dyn MetaStore>, reader: Arc<dyn MetaStore>) -> GCStateManager {
    let paths = ClusterPaths::new(42);
    let keys = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone())
        .keyspace_storage(paths.clone());
    for id in 100..356 {
        let mut tx = Base::new(store.as_ref()).begin_txn();
        keys.save_keyspace_meta(
            &mut tx,
            &paths,
            &Keyspace {
                identity: Some(KeyspaceIdentity::Id(id)),
                name: format!("ks{id}"),
                config: HashMap::from([("gc_management_type".into(), "keyspace_level".into())]),
                ..Default::default()
            },
        )
        .unwrap();
        tx.commit::<String>().await.unwrap();
    }
    let ep = StorageEndpoint::new(store);
    let mut tx = ep.begin_gc_state_transaction(&paths).await.unwrap();
    tx.batch.set_txn_safe_point(NULL_KEYSPACE_ID, 50);
    tx.batch.set_gc_safe_point(NULL_KEYSPACE_ID, 40);
    tx.commit().await.unwrap();
    GCStateManager::new(
        StorageEndpoint::new(reader),
        paths,
        Arc::new(astersql_pd_keyspace::KeyspaceManager::new(keys)),
        tokio::runtime::Handle::current(),
    )
}
#[tokio::test]
async fn mcs_050_behavior() {
    let mut f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = setup(store.clone(), store.clone()).await;
    let role = m.on_node_becomes_leader().await;
    let result = m
        .warmup_batch(
            [NULL_KEYSPACE_ID, 100].into_iter(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(
        result.completed.len(),
        2,
        "warmup must publish actual batch completions"
    );
    f.command("stop", "stopped");
    assert_eq!(
        m.get_gc_state(NULL_KEYSPACE_ID, true)
            .await
            .unwrap()
            .gc_safe_point,
        40
    );
    f.command("start", "restarted");
    role.shutdown().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_050_campaign_reconciles_dropped_pages() {
    let mut f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = Arc::new(setup(store.clone(), store.clone()).await);
    let role = m.on_node_becomes_leader().await;
    let warmup = Arc::new(GCWarmup::new(m.clone(), None, 4).unwrap());
    let entries: std::collections::BTreeMap<_, _> = (100..356)
        .map(|id| {
            (
                id,
                EnabledKeyspace {
                    id,
                    gc_management_type: "keyspace_level".into(),
                },
            )
        })
        .collect();
    for entry in entries.values() {
        warmup.on_page(vec![entry.clone()]);
    }
    warmup.on_initial_snapshot(EnabledSnapshot {
        entries,
        revision: 1,
    });
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        warmup.run(&CancellationToken::new()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(m.shared.cache.read().unwrap().len(), 257);
    // A completed campaign must already have warmed Null; it must not perform this read after stop.
    f.command("stop", "stopped");
    assert_eq!(
        m.get_gc_state(NULL_KEYSPACE_ID, true)
            .await
            .expect("completed campaign must publish the null singleton")
            .gc_safe_point,
        40
    );
    f.command("start", "restarted");
    role.shutdown().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_050_batch_limits_partial_errors_and_cached_completions() {
    let f = etcd_fixture::Fixture::start_with_txn_limit(120);
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = setup(store.clone(), store.clone()).await;
    let role = m.on_node_becomes_leader().await;
    let cancel = CancellationToken::new();
    let mut ids = 100..356;
    for count in [60, 60, 60, 60, 16] {
        let result = m.warmup_batch(&mut ids, &cancel).await.unwrap();
        assert_eq!(result.completed.len(), count);
        assert!(result.failed.is_empty());
    }
    assert_eq!(m.shared.cache.read().unwrap().len(), 256);
    // All 256 cached completions fit in one assembly; they consume no read slots.
    let cached = m.warmup_batch(100..356, &cancel).await.unwrap();
    assert_eq!(cached.completed.len(), 256);
    m.invalidate_points(107);
    Base::new(store.as_ref())
        .save(
            ClusterPaths::new(42).txn_safe_point_path(107).as_bytes(),
            b"bad",
        )
        .await
        .unwrap();
    let result = m
        .warmup_batch([107, 108].into_iter(), &cancel)
        .await
        .unwrap();
    assert!(result.failed.contains_key(&107));
    assert_eq!(result.completed, vec![108]);
    assert!(!m.shared.cache.read().unwrap().contains_key(&107));
    Base::new(store.as_ref())
        .save(
            ClusterPaths::new(42).txn_safe_point_path(107).as_bytes(),
            b"55",
        )
        .await
        .unwrap();
    assert_eq!(
        m.warmup_batch([107, 107].into_iter(), &cancel)
            .await
            .unwrap()
            .completed,
        vec![107]
    );
    assert_eq!(m.shared.cache.read().unwrap()[&107], (55, 0));
    role.shutdown().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_050_batch_gate_excludes_writer_and_preserves_new_point() {
    use std::time::Duration;
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
    let m = Arc::new(setup(store.clone(), reader).await);
    let role = m.on_node_becomes_leader().await;
    gate.arm_on_key(
        ClusterPaths::new(42)
            .txn_safe_point_path(NULL_KEYSPACE_ID)
            .as_bytes(),
    );
    let batch = {
        let m = m.clone();
        tokio::spawn(async move {
            m.warmup_batch(
                [NULL_KEYSPACE_ID, 100].into_iter(),
                &CancellationToken::new(),
            )
            .await
        })
    };
    tokio::time::timeout(Duration::from_secs(3), gate.held.notified())
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(
            Duration::from_millis(100),
            m.get_gc_state(NULL_KEYSPACE_ID, true)
        )
        .await
        .is_err()
    );
    let writer = {
        let m = m.clone();
        tokio::spawn(async move {
            m.advance_txn_safe_point(
                NULL_KEYSPACE_ID,
                60,
                astersql_pd_storage_domain::BarrierTime::from_unix(100, 0),
            )
            .await
        })
    };
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!writer.is_finished());
    assert_eq!(
        StorageEndpoint::new(store)
            .load_txn_safe_point(&ClusterPaths::new(42), NULL_KEYSPACE_ID)
            .await
            .unwrap(),
        50
    );
    gate.release();
    assert_eq!(batch.await.unwrap().unwrap().completed.len(), 2);
    writer.await.unwrap().unwrap();
    assert_eq!(
        m.get_gc_state(NULL_KEYSPACE_ID, true)
            .await
            .unwrap()
            .txn_safe_point,
        60
    );
    gate.close().await;
    role.shutdown().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_050_owner_drop_wakes_join_and_releases_claims() {
    use std::time::Duration;
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
    let m = Arc::new(setup(store, reader).await);
    let role = m.on_node_becomes_leader().await;
    gate.arm_on_key(
        ClusterPaths::new(42)
            .txn_safe_point_path(NULL_KEYSPACE_ID)
            .as_bytes(),
    );
    let batch = {
        let m = m.clone();
        tokio::spawn(async move {
            m.warmup_batch([NULL_KEYSPACE_ID].into_iter(), &CancellationToken::new())
                .await
        })
    };
    tokio::time::timeout(Duration::from_secs(3), gate.held.notified())
        .await
        .unwrap();
    let result = m
        .warmup_batch([NULL_KEYSPACE_ID].into_iter(), &CancellationToken::new())
        .await
        .unwrap();
    let mut waiter = result.joined.get(&NULL_KEYSPACE_ID).unwrap().clone();
    assert!(waiter.try_result().is_none());
    batch.abort();
    assert!(batch.await.unwrap_err().is_cancelled());
    assert!(
        tokio::time::timeout(
            Duration::from_secs(1),
            waiter.wait(&CancellationToken::new())
        )
        .await
        .unwrap()
        .is_err()
    );
    assert!(m.generation().unwrap().flights.lock().unwrap().is_empty());
    gate.release();
    assert_eq!(
        m.get_gc_state(NULL_KEYSPACE_ID, true)
            .await
            .unwrap()
            .gc_safe_point,
        40
    );
    gate.close().await;
    role.shutdown().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_050_pool_cancel_joins_workers_and_closes_full_input() {
    use std::time::Duration;
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
    let m = Arc::new(setup(store, reader).await);
    let role = m.on_node_becomes_leader().await;
    let warmup = Arc::new(GCWarmup::new(m.clone(), None, 4).unwrap());
    let weak = Arc::downgrade(&warmup);
    let cancel = CancellationToken::new();
    gate.arm_on_key(
        ClusterPaths::new(42)
            .txn_safe_point_path(NULL_KEYSPACE_ID)
            .as_bytes(),
    );
    let work = {
        let w = warmup.clone();
        let c = cancel.clone();
        tokio::spawn(async move { w.run(&c).await })
    };
    tokio::time::timeout(Duration::from_secs(3), gate.held.notified())
        .await
        .unwrap();
    for id in 100..356 {
        warmup.on_page(vec![EnabledKeyspace {
            id,
            gc_management_type: "keyspace_level".into(),
        }]);
    }
    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(3), work)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(m.generation().unwrap().flights.lock().unwrap().is_empty());
    assert!(
        warmup
            .wait_finished(&CancellationToken::new())
            .await
            .is_err()
    );
    warmup.on_page(vec![EnabledKeyspace {
        id: 100,
        gc_management_type: "keyspace_level".into(),
    }]);
    drop(warmup);
    assert!(weak.upgrade().is_none());
    gate.release();
    assert_eq!(
        m.get_gc_state(NULL_KEYSPACE_ID, true)
            .await
            .unwrap()
            .gc_safe_point,
        40
    );
    gate.close().await;
    role.shutdown().await;
}
