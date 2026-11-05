// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_storage_domain::{Consistency, MetaStore, Read};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_052_behavior() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = Arc::new(crate::gc_warmup_test::setup(store.clone(), store.clone()).await);
    let host = CancellationToken::new();
    let role = m.on_node_becomes_leader_with_cancel(&host).await;
    assert!(m.start_metadata_discovery(0, 2).await.is_err());
    assert!(m.enabled_keyspace_index().is_none());
    let index = m.start_metadata_discovery(2, 2).await.unwrap();
    assert!(m.start_metadata_discovery(2, 2).await.is_err());
    assert!(Arc::ptr_eq(&index, &m.enabled_keyspace_index().unwrap()));
    assert!(index.set_load_concurrency(0).is_err());
    index.set_load_concurrency(4).unwrap();
    assert_eq!(index.load_concurrency(), 4);
    let revision = store
        .read(Read::prefix(b"", Consistency::Linearizable))
        .await
        .unwrap()
        .revision;
    let ready = tokio::time::timeout(
        Duration::from_secs(2),
        index.snapshot_at_least(revision, &CancellationToken::new()),
    )
    .await;
    let warmed = tokio::time::timeout(Duration::from_secs(5), async {
        while m.shared.cache.read().unwrap().len() < 257 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    role.shutdown().await;
    assert!(
        warmed.is_ok(),
        "generation must own and complete its initial warmup"
    );
    assert!(
        ready.is_ok(),
        "the serving generation must actually own and run its enabled metadata index"
    );
    assert_eq!(ready.unwrap().unwrap().entries.len(), 256);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_052_late_response_reset_and_delayed_old_cleanup() {
    use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
    use astersql_pd_storage_domain::StorageEndpoint;
    let mut f = etcd_fixture::Fixture::start();
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
    let host = CancellationToken::new();
    let old_role = m.on_node_becomes_leader_with_cancel(&host).await;
    let old_generation = m.generation().unwrap();
    let paths = ClusterPaths::new(42);
    gate.arm_on_key(paths.txn_safe_point_path(NULL_KEYSPACE_ID).as_bytes());
    let old_index = m.start_metadata_discovery(2, 2).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), gate.held.notified())
        .await
        .unwrap();
    let ep = StorageEndpoint::new(store.clone());
    let mut tx = ep.begin_gc_state_transaction(&paths).await.unwrap();
    tx.batch.set_txn_safe_point(NULL_KEYSPACE_ID, 80);
    tx.batch.set_gc_safe_point(NULL_KEYSPACE_ID, 70);
    tx.commit().await.unwrap();
    let new_role = tokio::time::timeout(
        Duration::from_secs(3),
        m.on_node_becomes_leader_with_cancel(&host),
    )
    .await
    .expect("retirement must cancel and join blocked old-generation I/O");
    assert!(old_generation.flights.lock().unwrap().is_empty());
    assert!(old_generation.work.is_empty());
    assert!(
        old_index
            .snapshot_at_least(0, &CancellationToken::new())
            .await
            .is_err()
    );
    assert!(!Arc::ptr_eq(&old_generation, &m.generation().unwrap()));
    assert!(m.shared.cache.read().unwrap().is_empty());
    let new_index = m.start_metadata_discovery(3, 3).await.unwrap();
    gate.release();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if m.shared.cache.read().unwrap().get(&NULL_KEYSPACE_ID) == Some(&(80, 70)) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    old_role.shutdown().await;
    assert!(Arc::ptr_eq(
        &new_index,
        &m.enabled_keyspace_index().unwrap()
    ));
    f.command("stop", "stopped");
    assert_eq!(
        m.get_gc_state(NULL_KEYSPACE_ID, true)
            .await
            .unwrap()
            .gc_safe_point,
        70
    );
    f.command("start", "restarted");
    let fresh_store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let keys = astersql_pd_storage_endpoint::StorageEndpoint::new(fresh_store.clone())
        .keyspace_storage(paths.clone());
    let fresh = GCStateManager::new(
        StorageEndpoint::new(fresh_store),
        paths,
        Arc::new(astersql_pd_keyspace::KeyspaceManager::new(keys)),
        tokio::runtime::Handle::current(),
    );
    assert_eq!(
        fresh
            .get_gc_state(NULL_KEYSPACE_ID, true)
            .await
            .unwrap()
            .gc_safe_point,
        70
    );
    host.cancel();
    new_role.shutdown().await;
    assert!(
        new_index
            .snapshot_at_least(0, &CancellationToken::new())
            .await
            .is_err()
    );
    assert!(m.start_metadata_discovery(2, 2).await.is_err());
    gate.close().await;
}
