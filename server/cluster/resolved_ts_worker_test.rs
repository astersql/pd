// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{BasicCluster, NodeState, StoreInfo, StoreMeta};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

fn put(cluster: &BasicCluster, id: u64, ts: u64, state: NodeState, leaders: i64) {
    let mut store = StoreInfo::new(StoreMeta {
        id,
        node_state: state,
        ..Default::default()
    });
    store.set_min_resolved_ts(ts);
    store.set_leader_count(leaders);
    cluster.stores.put_store(Arc::new(store));
}
#[tokio::test]
async fn task_016_behavior() {
    let mut cluster = BasicCluster::new();
    assert_eq!(min_resolved_ts_candidate(&cluster, 1), None);
    let peer = astersql_pd_core::Peer {
        id: 1,
        store_id: 1,
        ..Default::default()
    };
    let region = Arc::new(astersql_pd_core::RegionInfo::new(
        astersql_pd_core::RegionMeta {
            id: 1,
            peers: vec![peer.clone()],
            ..Default::default()
        },
        Some(peer),
        vec![],
        vec![],
    ));
    cluster.regions.set_region(region);
    assert_eq!(min_resolved_ts_candidate(&cluster, 3), None);
    put(&cluster, 1, 42, NodeState::Serving, 1);
    put(&cluster, 2, 60, NodeState::Removing, 1);
    put(&cluster, 3, 1, NodeState::Removed, 1);
    put(&cluster, 4, 0, NodeState::Serving, 0);
    assert_eq!(
        min_resolved_ts_candidate(&cluster, 3),
        None,
        "insufficient voters must suppress aggregation even with eligible stores"
    );
    let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let paths = ClusterPaths::new(516);
    let current = AtomicU64::new(0);
    refresh_min_resolved_ts(
        min_resolved_ts_candidate(&cluster, 1),
        &current,
        &storage,
        &paths,
    )
    .await
    .unwrap();
    assert_eq!(
        current.load(Ordering::Relaxed),
        42,
        "eligible stores must advance cluster minimum"
    );
    assert_eq!(storage.load_min_resolved_ts(&paths).await.unwrap(), 42);
    for value in [None, Some(0), Some(41), Some(u64::MAX)] {
        refresh_min_resolved_ts(value, &current, &storage, &paths)
            .await
            .unwrap();
        assert_eq!(storage.load_min_resolved_ts(&paths).await.unwrap(), 42);
    }
    put(&cluster, 1, 80, NodeState::Removed, 1);
    refresh_min_resolved_ts(
        min_resolved_ts_candidate(&cluster, 1),
        &current,
        &storage,
        &paths,
    )
    .await
    .unwrap();
    assert_eq!(storage.load_min_resolved_ts(&paths).await.unwrap(), 60);
}
#[tokio::test(start_paused = true)]
async fn task_016_disabled_and_cancel() {
    let interval = Arc::new(AtomicU64::new(0));
    let current = Arc::new(AtomicU64::new(0));
    let storage = Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default())));
    let paths = ClusterPaths::new(516);
    let i = interval.clone();
    let c = current.clone();
    let s = storage.clone();
    let worker = ResolvedTsWorker::start(
        &tokio::runtime::Handle::current(),
        &CancellationToken::new(),
        move || Duration::from_millis(i.load(Ordering::Relaxed)),
        move || {
            let c = c.clone();
            let s = s.clone();
            async move {
                refresh_min_resolved_ts(Some(42), &c, &s, &paths)
                    .await
                    .unwrap();
            }
        },
    );
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(1)).await;
    tokio::task::yield_now().await;
    assert_eq!(current.load(Ordering::Relaxed), 0);
    interval.store(10, Ordering::Relaxed);
    tokio::time::advance(Duration::from_secs(1)).await;
    tokio::task::yield_now().await;
    assert_eq!(storage.load_min_resolved_ts(&paths).await.unwrap(), 42);
    worker.stop().await.unwrap();
    assert_eq!(Arc::strong_count(&current), 1);
}

#[tokio::test]
async fn task_016_real_storage_failure() {
    let mut fixture = effects_fixture::Fixture::start();
    let storage = StorageEndpoint::new(Arc::new(fixture.store().await));
    let paths = ClusterPaths::new(516);
    let current = AtomicU64::new(0);
    refresh_min_resolved_ts(Some(42), &current, &storage, &paths)
        .await
        .unwrap();
    fixture.command("stop", "stopped");
    assert!(
        refresh_min_resolved_ts(Some(60), &current, &storage, &paths)
            .await
            .is_err()
    );
    assert_eq!(current.load(Ordering::Relaxed), 60);
    fixture.command("start", "restarted");
    let recovered = StorageEndpoint::new(Arc::new(fixture.store().await));
    refresh_min_resolved_ts(Some(60), &current, &recovered, &paths)
        .await
        .unwrap();
    assert_eq!(
        recovered.load_min_resolved_ts(&paths).await.unwrap(),
        42,
        "Go does not retry an unchanged minimum"
    );
    refresh_min_resolved_ts(Some(61), &current, &recovered, &paths)
        .await
        .unwrap();
    assert_eq!(recovered.load_min_resolved_ts(&paths).await.unwrap(), 61);
}

#[tokio::test(start_paused = true)]
async fn task_016_cancel_joins_admitted_persistence() {
    let storage = Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default())));
    let paths = ClusterPaths::new(516);
    let current = Arc::new(AtomicU64::new(0));
    let cancel = CancellationToken::new();
    let (admitted, mut receive) = tokio::sync::mpsc::unbounded_channel();
    let s = storage.clone();
    let c = current.clone();
    let worker = ResolvedTsWorker::start(
        &tokio::runtime::Handle::current(),
        &cancel,
        || Duration::from_millis(10),
        move || {
            let s = s.clone();
            let c = c.clone();
            let admitted = admitted.clone();
            async move {
                admitted.send(()).unwrap();
                tokio::time::sleep(Duration::from_secs(1)).await;
                refresh_min_resolved_ts(Some(42), &c, &s, &paths)
                    .await
                    .unwrap();
            }
        },
    );
    receive.recv().await.unwrap();
    cancel.cancel();
    worker.stop().await.unwrap();
    assert_eq!(storage.load_min_resolved_ts(&paths).await.unwrap(), 42);
    assert_eq!(Arc::strong_count(&current), 1);
}
