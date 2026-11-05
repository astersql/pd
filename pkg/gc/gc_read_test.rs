// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
use astersql_pd_keyspace::KeyspaceManager;
use astersql_pd_storage_domain::{Keyspace, KeyspaceIdentity, MetaStore, StorageEndpoint};
use std::sync::Arc;
#[tokio::test]
async fn mcs_043_behavior() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let p = ClusterPaths::new(42);
    let keyspaces = Arc::new(KeyspaceManager::new(
        astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone())
            .keyspace_storage(p.clone()),
    ));
    keyspaces
        .save_new(&Keyspace {
            identity: Some(KeyspaceIdentity::Id(2)),
            name: "unified".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let storage = StorageEndpoint::new(store);
    storage
        .run_in_gc_state_transaction(&p, async |wb| {
            wb.set_gc_safe_point(NULL_KEYSPACE_ID, 41);
            wb.set_txn_safe_point(NULL_KEYSPACE_ID, 50);
            Ok::<_, String>(())
        })
        .await
        .unwrap();
    let m = GCStateManager::new(storage, p, keyspaces, tokio::runtime::Handle::current());
    let state = m.get_gc_state(2, true).await.unwrap();
    assert_eq!(
        state.keyspace_id, NULL_KEYSPACE_ID,
        "unified metadata must redirect to the actual shared scope"
    );
    assert_eq!(state.gc_safe_point, 41);
    assert_eq!(state.txn_safe_point, 50);
}

use astersql_pd_storage_domain::{BarrierTime, Base, GCBarrier};
use std::{collections::HashMap, time::Duration};
use tokio_util::sync::CancellationToken;
async fn setup(store: Arc<dyn MetaStore>, reader: Arc<dyn MetaStore>) -> GCStateManager {
    let p = ClusterPaths::new(42);
    let k = Arc::new(KeyspaceManager::new(
        astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone())
            .keyspace_storage(p.clone()),
    ));
    for (id, level, state) in [(0, true, 0), (2, true, 0), (3, false, 0), (4, true, 2)] {
        k.save_new(&Keyspace {
            identity: Some(KeyspaceIdentity::Id(id)),
            name: format!("ks{id}"),
            state,
            config: if level {
                HashMap::from([("gc_management_type".into(), "keyspace_level".into())])
            } else {
                HashMap::new()
            },
            ..Default::default()
        })
        .await
        .unwrap();
    }
    let ep = StorageEndpoint::new(store);
    for id in [NULL_KEYSPACE_ID, 0, 2, 3, 4] {
        ep.run_in_gc_state_transaction(&p, async |b| {
            b.set_txn_safe_point(id, 100 + u64::from(id != NULL_KEYSPACE_ID) * id as u64);
            b.set_gc_safe_point(id, 50);
            Ok::<_, String>(())
        })
        .await
        .unwrap();
    }
    GCStateManager::new(
        StorageEndpoint::new(reader),
        p,
        k,
        tokio::runtime::Handle::current(),
    )
}
#[tokio::test]
async fn mcs_043_redirect_matrix_and_internal_legacy_guards() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = setup(store.clone(), store).await;
    for id in [NULL_KEYSPACE_ID, 0x0100_0000, 0x7fff_ffff, 3, 0, 2, 4] {
        for exclude in [true, false] {
            let state = m.get_gc_state(id, exclude).await.unwrap();
            let actual = if id == 3 || id > 0xffffff {
                NULL_KEYSPACE_ID
            } else {
                id
            };
            assert_eq!(state.keyspace_id, actual);
            assert_eq!(state.is_keyspace_level, actual != NULL_KEYSPACE_ID);
            assert_eq!(state.gc_safe_point, 50);
            assert_eq!(
                state.txn_safe_point,
                100 + if actual == NULL_KEYSPACE_ID {
                    0
                } else {
                    actual as u64
                }
            );
        }
    }
    assert!(matches!(
        m.get_gc_state(65535, true).await,
        Err(GCReadError::Keyspace(_))
    ));
    assert!(matches!(
        m.compatible_load_gc_safe_point(3).await,
        Err(GCReadError::InvalidKeyspace { id: 3, .. })
    ));
    assert_eq!(
        m.compatible_load_gc_safe_point(0x0100_0000).await.unwrap(),
        50
    );
}
#[tokio::test]
async fn mcs_043_passive_reads_active_cache_and_delayed_old_cleanup() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = setup(store.clone(), store).await;
    assert_eq!(m.get_gc_state(2, true).await.unwrap().txn_safe_point, 102);
    let set = async |value| {
        m.storage
            .run_in_gc_state_transaction(&m.paths, async |b| {
                b.set_txn_safe_point(2, value);
                Ok::<_, String>(())
            })
            .await
            .unwrap()
    };
    set(200).await;
    assert_eq!(
        m.get_gc_state(2, true).await.unwrap().txn_safe_point,
        200,
        "passive cache is never served without a generation"
    );
    let old = m.on_node_becomes_leader().await;
    assert_eq!(m.get_gc_state(2, true).await.unwrap().txn_safe_point, 200);
    set(300).await;
    assert_eq!(m.get_gc_state(2, true).await.unwrap().txn_safe_point, 200);
    assert_eq!(m.get_gc_state(2, false).await.unwrap().txn_safe_point, 300);
    assert_eq!(
        m.get_gc_state(2, true).await.unwrap().txn_safe_point,
        200,
        "full read must not replace the point cache"
    );
    let new = m.on_node_becomes_leader().await;
    old.shutdown().await;
    assert_eq!(m.get_gc_state(2, true).await.unwrap().txn_safe_point, 300);
    new.shutdown().await;
    set(400).await;
    assert_eq!(m.get_gc_state(2, true).await.unwrap().txn_safe_point, 400);
}
#[tokio::test]
async fn mcs_043_decode_failure_no_partial_cache_and_barrier_filtering() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = setup(store.clone(), store.clone()).await;
    let role = m.on_node_becomes_leader().await;
    let base = Base::new(store.as_ref());
    base.save(m.paths.txn_safe_point_path(2).as_bytes(), b"bad-txn")
        .await
        .unwrap();
    base.save(m.paths.gc_safe_point_path(2).as_bytes(), b"bad-gc")
        .await
        .unwrap();
    let error = m.get_gc_state(2, true).await.unwrap_err();
    assert!(
        matches!(error,GCReadError::SafePoint(ref e) if matches!(e.as_ref(),astersql_pd_storage_domain::SafePointReadError::ParseUint(_)))
    );
    base.save(m.paths.txn_safe_point_path(2).as_bytes(), b"999")
        .await
        .unwrap();
    assert!(m.get_gc_state(2, true).await.is_err());
    base.save(
        m.paths.gc_safe_point_path(2).as_bytes(),
        br#"{"safe_point":80}"#,
    )
    .await
    .unwrap();
    let state = m.get_gc_state(2, true).await.unwrap();
    assert_eq!((state.txn_safe_point, state.gc_safe_point), (999, 80));
    let mut txn = m
        .storage
        .begin_gc_state_transaction(&m.paths)
        .await
        .unwrap();
    let mut expiration = BarrierTime::from_unix(1, 0);
    txn.batch
        .set_gc_barrier(2, &GCBarrier::new("expired", 1000, Some(&mut expiration)))
        .unwrap();
    txn.batch
        .set_gc_barrier(2, &GCBarrier::new("gc_worker", 999, None))
        .unwrap();
    txn.commit().await.unwrap();
    let state = m.get_gc_state(2, false).await.unwrap();
    assert_eq!(state.gc_barriers.len(), 1);
    assert_eq!(
        state.gc_barriers[0].barrier_id, "expired",
        "reads preserve expired entries but exclude reserved gc_worker"
    );
    base.save(m.paths.gc_barrier_path(2, "broken").as_bytes(), b"invalid")
        .await
        .unwrap();
    assert!(m.get_gc_state(2, false).await.is_err());
    assert!(
        m.get_gc_state(2, true)
            .await
            .unwrap()
            .gc_barriers
            .is_empty()
    );
    role.shutdown().await;
}
#[tokio::test]
async fn mcs_043_real_io_failure_and_same_wal_retry() {
    let mut f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = setup(store.clone(), store).await;
    let role = m.on_node_becomes_leader().await;
    let expected = m.get_gc_state(2, true).await.unwrap();
    let unified = m.get_gc_state(NULL_KEYSPACE_ID, true).await.unwrap();
    f.command("stop", "stopped");
    assert!(
        m.get_gc_state(2, true).await.is_err(),
        "metadata redirection still reads authoritative storage before the point cache"
    );
    assert_eq!(
        m.get_gc_state(NULL_KEYSPACE_ID, true).await.unwrap(),
        unified
    );
    assert!(m.get_gc_state(2, false).await.is_err());
    assert!(m.get_gc_state(4, true).await.is_err());
    role.shutdown().await;
    f.command("start", "restarted");
    let fresh: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let keys = Arc::new(KeyspaceManager::new(
        astersql_pd_storage_endpoint::StorageEndpoint::new(fresh.clone())
            .keyspace_storage(m.paths.clone()),
    ));
    let recovered = GCStateManager::new(
        StorageEndpoint::new(fresh),
        m.paths.clone(),
        keys,
        tokio::runtime::Handle::current(),
    );
    let role = recovered.on_node_becomes_leader().await;
    assert_eq!(recovered.get_gc_state(2, true).await.unwrap(), expected);
    assert_eq!(
        recovered
            .get_gc_state(4, true)
            .await
            .unwrap()
            .txn_safe_point,
        104
    );
    role.shutdown().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_043_cancelled_waiter_does_not_cancel_shared_real_read() {
    use std::sync::atomic::Ordering;
    let f = etcd_fixture::Fixture::start();
    let direct: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let proxy = crate::response_gate_test::ResponseGate::start(&f.endpoint).await;
    let reader: Arc<dyn MetaStore> = Arc::new(
        astersql_pd_storage_kv::EtcdMetaStore::connect(
            &[proxy.endpoint.clone()],
            Duration::from_secs(3),
        )
        .await
        .unwrap(),
    );
    let m = Arc::new(setup(direct, reader).await);
    m.get_gc_state(NULL_KEYSPACE_ID, true).await.unwrap();
    let role = m.on_node_becomes_leader().await;
    proxy.armed.store(true, Ordering::SeqCst);
    let cancel = CancellationToken::new();
    let a = {
        let m = m.clone();
        let c = cancel.clone();
        tokio::spawn(async move { m.get_gc_state_with_cancel(2, true, &c).await })
    };
    tokio::time::timeout(Duration::from_secs(2), proxy.held.notified())
        .await
        .unwrap();
    let b = {
        let m = m.clone();
        tokio::spawn(async move { m.get_gc_state(2, true).await })
    };
    cancel.cancel();
    assert!(matches!(a.await.unwrap(), Err(GCReadError::Cancelled)));
    assert!(!b.is_finished());
    proxy.release();
    let state = tokio::time::timeout(Duration::from_secs(2), b)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!((state.txn_safe_point, state.gc_safe_point), (102, 50));
    role.shutdown().await;
    proxy.close().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_043_generation_retirement_fences_pending_backend_response_and_joins() {
    use std::sync::atomic::Ordering;
    let f = etcd_fixture::Fixture::start();
    let direct: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let proxy = crate::response_gate_test::ResponseGate::start(&f.endpoint).await;
    let reader: Arc<dyn MetaStore> = Arc::new(
        astersql_pd_storage_kv::EtcdMetaStore::connect(
            &[proxy.endpoint.clone()],
            Duration::from_secs(3),
        )
        .await
        .unwrap(),
    );
    let m = Arc::new(setup(direct, reader).await);
    m.get_gc_state(NULL_KEYSPACE_ID, true).await.unwrap();
    let old = m.on_node_becomes_leader().await;
    proxy.armed.store(true, Ordering::SeqCst);
    let pending = {
        let m = m.clone();
        tokio::spawn(async move { m.get_gc_state(2, true).await })
    };
    tokio::time::timeout(Duration::from_secs(2), proxy.held.notified())
        .await
        .unwrap();
    let fresh = tokio::time::timeout(Duration::from_secs(2), m.on_node_becomes_leader())
        .await
        .unwrap();
    assert!(matches!(
        pending.await.unwrap(),
        Err(GCReadError::NotLeader)
    ));
    old.shutdown().await;
    proxy.release();
    assert_eq!(m.get_gc_state(2, true).await.unwrap().txn_safe_point, 102);
    fresh.shutdown().await;
    proxy.close().await;
}

#[tokio::test]
async fn mcs_043_global_read_atomic_revision_and_no_partial_result() {
    use astersql_pd_storage_domain::GlobalGCBarrier;
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = setup(store.clone(), store.clone()).await;
    let mut txn = m
        .storage
        .begin_gc_state_transaction(&m.paths)
        .await
        .unwrap();
    txn.batch
        .set_global_gc_barrier(&GlobalGCBarrier::new("g", 200, None))
        .unwrap();
    txn.commit().await.unwrap();
    let (state, globals) = m.get_gc_state_with_global_barriers(2, true).await.unwrap();
    assert_eq!(state.txn_safe_point, 102);
    assert_eq!(globals.len(), 1);
    assert_eq!(globals[0].barrier_id, "g");
    let prepared = m.prepare_read(2, true, true).await.unwrap();
    let mut txn = m
        .storage
        .begin_gc_state_transaction(&m.paths)
        .await
        .unwrap();
    txn.batch.set_txn_safe_point(2, 1000);
    txn.commit().await.unwrap();
    assert!(
        matches!(prepared.commit().await,Err(GCReadError::Transaction(ref error))if matches!(error.as_ref(),astersql_pd_storage_domain::GCStateTxnError::Conflict))
    );
    assert_eq!(
        m.get_gc_state_with_global_barriers(2, true)
            .await
            .unwrap()
            .0
            .txn_safe_point,
        1000
    );
    Base::new(store.as_ref())
        .save(m.paths.global_gc_barrier_path("corrupt").as_bytes(), b"{")
        .await
        .unwrap();
    assert!(m.get_gc_state_with_global_barriers(2, true).await.is_err());
    assert_eq!(
        m.get_gc_state(2, false).await.unwrap().txn_safe_point,
        1000,
        "ordinary state read never loads global rows"
    );
}
