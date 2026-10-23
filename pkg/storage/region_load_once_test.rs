// Copyright 2026 AsterSQL.
use std::sync::Arc;

use astersql_pd_core::RegionMeta;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_endpoint::{RegionScanError, StorageEndpoint};
use tokio_util::sync::CancellationToken;

use super::{Base, MemoryStore, MetaStore, Storage, etcd_fixture};

fn region(id: u64) -> RegionMeta {
    RegionMeta {
        id,
        ..Default::default()
    }
}

async fn load(storage: &mut Storage, paths: &ClusterPaths) -> Vec<u64> {
    let mut ids = vec![];
    storage
        .try_load_regions_once(paths, &CancellationToken::new(), |r| {
            ids.push(r.meta().id);
            vec![]
        })
        .await
        .unwrap();
    ids
}

async fn contract(default: Arc<dyn MetaStore>, local: Arc<dyn MetaStore>) {
    // Unique cluster paths isolate each scenario without replacing real backends.
    let paths = ClusterPaths::new(5601);
    let default_view = StorageEndpoint::new(default.clone());
    let local_view = StorageEndpoint::new(local.clone());
    let core = || {
        Storage::new_core(
            Storage::new(default.clone()),
            StorageEndpoint::new(local.clone()),
        )
    };
    default_view.save_region(&paths, &region(1)).await.unwrap();
    local_view.save_region(&paths, &region(10)).await.unwrap();
    let mut storage = core();
    assert!(!storage.are_regions_loaded());
    storage.try_switch_region_storage(true).unwrap();
    assert!(!storage.are_regions_loaded());
    assert_eq!(load(&mut storage, &paths).await, vec![10]);
    assert!(storage.are_regions_loaded());
    local_view.save_region(&paths, &region(11)).await.unwrap();
    assert!(load(&mut storage, &paths).await.is_empty());
    // Even a cancelled call must be skipped after successful local loading.
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    storage
        .try_load_regions_once(&paths, &cancelled, |_| panic!("local loaded twice"))
        .await
        .unwrap();
    storage.try_switch_region_storage(false).unwrap();
    assert!(!storage.are_regions_loaded());
    assert!(matches!(
        storage
            .try_load_regions_once(&paths, &cancelled, |_| panic!("cancelled callback"))
            .await,
        Err(RegionScanError::Cancelled)
    ));
    assert!(!storage.are_regions_loaded());
    storage.try_switch_region_storage(true).unwrap();
    assert!(
        storage.are_regions_loaded(),
        "default failure must preserve local success"
    );
    storage.try_switch_region_storage(false).unwrap();
    assert_eq!(load(&mut storage, &paths).await, vec![1]);
    assert!(storage.are_regions_loaded());
    default_view.save_region(&paths, &region(2)).await.unwrap();
    assert_eq!(
        load(&mut storage, &paths).await,
        vec![2],
        "default must scan again with its retained cursor"
    );
    // A failed subsequent default scan must retain its earlier successful state.
    assert!(
        storage
            .try_load_regions_once(&paths, &cancelled, |_| vec![])
            .await
            .is_err()
    );
    assert!(storage.are_regions_loaded());
    storage.try_switch_region_storage(true).unwrap();
    assert!(!storage.are_regions_loaded());
    assert!(load(&mut storage, &paths).await.is_empty());
    assert!(
        !storage.are_regions_loaded(),
        "skipping cannot overwrite default provenance"
    );

    // Default-first success suppresses even the first local scan.
    let paths = ClusterPaths::new(5602);
    default_view.save_region(&paths, &region(1)).await.unwrap();
    local_view.save_region(&paths, &region(10)).await.unwrap();
    let mut storage = core();
    assert_eq!(load(&mut storage, &paths).await, vec![1]);
    storage.try_switch_region_storage(true).unwrap();
    assert!(!storage.are_regions_loaded());
    assert!(load(&mut storage, &paths).await.is_empty());
    assert_eq!(storage.retrieve_region_storage().next_region_id(), 0);
    storage.try_switch_region_storage(false).unwrap();
    assert!(storage.are_regions_loaded());

    // Failed local scan retains progress but never marks loading successful.
    let paths = ClusterPaths::new(5603);
    local_view.save_region(&paths, &region(1)).await.unwrap();
    Base::new(local.as_ref())
        .save(paths.region_path(2).as_bytes(), &[255])
        .await
        .unwrap();
    let mut storage = core();
    storage.try_switch_region_storage(true).unwrap();
    let mut ids = vec![];
    let error = storage
        .try_load_regions_once(&paths, &CancellationToken::new(), |r| {
            ids.push(r.meta().id);
            vec![]
        })
        .await
        .unwrap_err();
    assert!(matches!(error, RegionScanError::Region(_)));
    assert_eq!(ids, vec![1]);
    assert!(!storage.are_regions_loaded());
    assert_eq!(storage.retrieve_region_storage().next_region_id(), 2);
    local_view.save_region(&paths, &region(2)).await.unwrap();
    assert_eq!(load(&mut storage, &paths).await, vec![2]);
    assert!(storage.are_regions_loaded());
    local_view.save_region(&paths, &region(3)).await.unwrap();
    assert!(load(&mut storage, &paths).await.is_empty());

    // Cancellation of initial default and local loads leaves both retryable.
    let paths = ClusterPaths::new(5604);
    local_view.save_region(&paths, &region(1)).await.unwrap();
    let mut storage = core();
    assert!(matches!(
        storage
            .try_load_regions_once(&paths, &cancelled, |_| vec![])
            .await,
        Err(RegionScanError::Cancelled)
    ));
    assert!(!storage.are_regions_loaded());
    storage.try_switch_region_storage(true).unwrap();
    assert!(matches!(
        storage
            .try_load_regions_once(&paths, &cancelled, |_| vec![])
            .await,
        Err(RegionScanError::Cancelled)
    ));
    assert!(!storage.are_regions_loaded());
    assert_eq!(load(&mut storage, &paths).await, vec![1]);
    assert!(storage.are_regions_loaded());

    // Plain facades always delegate to their endpoint and retain its cursor.
    let paths = ClusterPaths::new(5605);
    let mut plain = Storage::new(default.clone());
    assert!(load(&mut plain, &paths).await.is_empty());
    default_view.save_region(&paths, &region(1)).await.unwrap();
    assert_eq!(load(&mut plain, &paths).await, vec![1]);
    default_view.save_region(&paths, &region(2)).await.unwrap();
    assert_eq!(load(&mut plain, &paths).await, vec![2]);
    assert!(matches!(
        plain
            .try_load_regions_once(&paths, &cancelled, |_| vec![])
            .await,
        Err(RegionScanError::Cancelled)
    ));

    // Empty local success is still a successful load, and direct scans do not set facade state.
    let paths = ClusterPaths::new(5606);
    let mut storage = core();
    storage.try_switch_region_storage(true).unwrap();
    storage
        .load_regions(&paths, &CancellationToken::new(), |_| vec![])
        .await
        .unwrap();
    assert!(!storage.are_regions_loaded());
    assert!(load(&mut storage, &paths).await.is_empty());
    assert!(storage.are_regions_loaded());
    local_view.save_region(&paths, &region(1)).await.unwrap();
    assert!(load(&mut storage, &paths).await.is_empty());
}

#[tokio::test]
async fn region_load_once_memory() {
    contract(
        Arc::new(MemoryStore::default()),
        Arc::new(MemoryStore::default()),
    )
    .await;
}

#[tokio::test]
async fn region_load_once_etcd() {
    let fixture = etcd_fixture::Fixture::start();
    // Both sources use real etcd; scenario cluster paths also keep cursors independent.
    // Separate fixture processes ensure the default and injected sources have distinct data.
    let local_fixture = etcd_fixture::Fixture::start();
    contract(
        Arc::new(fixture.store().await),
        Arc::new(local_fixture.store().await),
    )
    .await;
}
