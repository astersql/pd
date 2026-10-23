// Copyright 2026 AsterSQL.
use super::*;
use crate::endpoint_codec_test::fixture;
use astersql_pd_keypath::CLUSTER_ID_PATH;
use astersql_pd_storage_kv::{Base, Consistency, MemoryStore, MetaStore, Read, StoreError};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
const KEY: &[u8] = CLUSTER_ID_PATH.as_bytes();
// Existing Go TestUint64ToBytes / TestBytesToUint64, run read-only this task.
const GO_1000: &[u8] = b"\x00\x00\x00\x00\x00\x00\x03\xe8";
async fn contract(store: Arc<dyn MetaStore>) {
    let endpoint = StorageEndpoint::new(store.clone());
    let base = Base::new(store.as_ref());
    assert_eq!(endpoint.get_cluster_id().await.unwrap(), 0);
    assert_eq!(endpoint.init_cluster_id(0, 1000).await.unwrap(), 1000);
    assert_eq!(base.load(KEY).await.unwrap(), GO_1000);
    assert_eq!(endpoint.get_cluster_id().await.unwrap(), 1000);
    let before = store
        .read(Read::range(KEY, b"", Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(endpoint.init_cluster_id(123, 456).await.unwrap(), 1000);
    assert_eq!(endpoint.init_or_get_cluster_id(999).await.unwrap(), 1000);
    let after = store
        .read(Read::range(KEY, b"", Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(before.entries, after.entries);
    // Fresh endpoint instances cannot rely on a process-global cache.
    assert_eq!(
        StorageEndpoint::new(store.clone())
            .get_cluster_id()
            .await
            .unwrap(),
        1000
    );
    for length in [0, 1, 7, 9] {
        let bytes = vec![0xff; length];
        base.save(KEY, &bytes).await.unwrap();
        assert!(
            matches!(endpoint.get_cluster_id().await, Err(ClusterIdError::InvalidLength(n)) if n == length)
        );
        assert!(
            matches!(endpoint.init_cluster_id(1, 2).await, Err(ClusterIdError::InvalidLength(n)) if n == length)
        );
        assert!(
            matches!(endpoint.init_or_get_cluster_id(999).await, Err(ClusterIdError::InvalidLength(n)) if n == length)
        );
        assert_eq!(base.load(KEY).await.unwrap(), bytes);
    }
    // Existing zero still occupies the key: CAS must read it, never overwrite it.
    base.save(KEY, &[0; 8]).await.unwrap();
    assert_eq!(endpoint.init_cluster_id(2, 3).await.unwrap(), 0);
    assert_eq!(base.load(KEY).await.unwrap(), [0; 8]);
    base.remove(KEY).await.unwrap();
    assert_eq!(endpoint.init_cluster_id(0, 0).await.unwrap(), 0);
    assert_eq!(base.load(KEY).await.unwrap(), [0; 8]);
    base.remove(KEY).await.unwrap();
    let barrier = Arc::new(tokio::sync::Barrier::new(16));
    let mut tasks = tokio::task::JoinSet::new();
    for random in 1..=16 {
        let store = store.clone();
        let barrier = barrier.clone();
        tasks.spawn(async move {
            barrier.wait().await;
            StorageEndpoint::new(store)
                .init_or_get_cluster_id(cluster_id_candidate(123, random))
                .await
                .unwrap()
        });
    }
    let winner = tasks.join_next().await.unwrap().unwrap();
    assert!((1..=16).any(|random| cluster_id_candidate(123, random) == winner));
    while let Some(result) = tasks.join_next().await {
        assert_eq!(result.unwrap(), winner);
    }
    assert_eq!(base.load(KEY).await.unwrap(), winner.to_be_bytes());
    assert_eq!(endpoint.get_cluster_id().await.unwrap(), winner);
    base.remove(KEY).await.unwrap();
    let cancel = CancellationToken::new();
    let mut waiting = Box::pin(endpoint.wait_cluster_id(&cancel));
    assert!(
        tokio::time::timeout(Duration::from_millis(30), &mut waiting)
            .await
            .is_err()
    );
    assert!(base.load(KEY).await.unwrap().is_empty());
    // An MCS reader stays pending on corrupt and zero values, then accepts a PD write.
    base.save(KEY, b"bad").await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(550), &mut waiting)
            .await
            .is_err()
    );
    base.save(KEY, &[0; 8]).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(550), &mut waiting)
            .await
            .is_err()
    );
    base.save(KEY, GO_1000).await.unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), &mut waiting)
            .await
            .unwrap()
            .unwrap(),
        1000
    );
    assert_eq!(base.load(KEY).await.unwrap(), GO_1000);
    base.remove(KEY).await.unwrap();
    let mut waiting = Box::pin(endpoint.wait_cluster_id(&cancel));
    assert!(
        tokio::time::timeout(Duration::from_millis(30), &mut waiting)
            .await
            .is_err()
    );
    cancel.cancel();
    assert!(matches!(
        tokio::time::timeout(Duration::from_millis(200), waiting)
            .await
            .unwrap(),
        Err(ClusterIdError::Store(StoreError::Cancelled))
    ));
    assert!(base.load(KEY).await.unwrap().is_empty());
    // Pre-cancellation wins even when an ID is already present.
    base.save(KEY, GO_1000).await.unwrap();
    assert!(matches!(
        endpoint.wait_cluster_id(&cancel).await,
        Err(ClusterIdError::Store(StoreError::Cancelled))
    ));
}
#[tokio::test]
async fn cluster_id_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn cluster_id_etcd() {
    let fixture = fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
}
#[test]
fn cluster_id_candidate_values() {
    assert_eq!(cluster_id_candidate(0, 1000).to_be_bytes(), GO_1000);
    assert_eq!(cluster_id_candidate(1, u32::MAX), 0x1_ffff_ffff);
    assert_eq!(cluster_id_candidate(u64::MAX, u32::MAX), u64::MAX);
    assert_eq!(cluster_id_candidate(1 << 32, 0), 0);
}

#[tokio::test]
async fn cluster_id_etcd_cancel_inflight_read() {
    let mut fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let endpoint = StorageEndpoint::new(store.clone());
    endpoint.init_cluster_id(0, 1000).await.unwrap();
    fixture.command("stop", "stopped");
    let cancel = CancellationToken::new();
    let mut waiting = Box::pin(endpoint.wait_cluster_id(&cancel));
    // Read starts only after the real server has acknowledged shutdown.
    assert!(
        tokio::time::timeout(Duration::from_millis(30), &mut waiting)
            .await
            .is_err()
    );
    cancel.cancel();
    assert!(matches!(
        tokio::time::timeout(Duration::from_millis(200), waiting)
            .await
            .unwrap(),
        Err(ClusterIdError::Store(StoreError::Cancelled))
    ));
    fixture.command("start", "restarted");
    assert_eq!(endpoint.get_cluster_id().await.unwrap(), 1000);
    assert_eq!(Base::new(store.as_ref()).load(KEY).await.unwrap(), GO_1000);
}
