// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{KeyRange, RegionMeta};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_endpoint::{LoadState, RegionMetaError, RegionScanError, StorageEndpoint};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use super::etcd_fixture as fixture;

fn sample() -> RegionMeta {
    RegionMeta {
        id: 99,
        key_range: KeyRange {
            start_key: vec![0, 1],
            end_key: vec![255],
        },
        ..Default::default()
    }
}

fn go_region_bytes() -> Vec<u8> {
    // Existing Go output, also used by endpoint/region_meta_test.rs.
    let line = include_str!("../proto/wire-fixtures.json")
        .lines()
        .find(|line| line.contains("\"region_heartbeat\""))
        .unwrap();
    let hex = line.split('"').nth(3).unwrap();
    let bytes: Vec<_> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    assert_eq!(&bytes[18..20], &[0x12, 9]);
    bytes[20..29].to_vec()
}

async fn contract(default: Arc<dyn MetaStore>, local: Arc<dyn MetaStore>) {
    let paths = ClusterPaths::new(42);
    let region = sample();
    let mut local_region = region.clone();
    local_region.key_range.start_key = vec![2];
    let default_view = StorageEndpoint::new(default.clone());
    let local_view = StorageEndpoint::new(local.clone());
    let mut storage = Storage::new_core(Storage::new(default), StorageEndpoint::new(local.clone()));
    storage.save_region(&paths, &region).await.unwrap();
    assert_eq!(
        default_view.load_region(&paths, 99).await.unwrap(),
        Some(region.clone())
    );
    assert_eq!(local_view.load_region(&paths, 99).await.unwrap(), None);
    storage.try_switch_region_storage(true).unwrap();
    storage.save_region(&paths, &local_region).await.unwrap();
    assert_eq!(
        local_view.load_region(&paths, 99).await.unwrap(),
        Some(local_region.clone())
    );
    assert_eq!(
        default_view.load_region(&paths, 99).await.unwrap(),
        Some(region.clone())
    );
    assert!(std::ptr::eq(
        storage.retrieve_region_storage(),
        storage.try_switch_region_storage(true).unwrap()
    ));
    assert!(std::ptr::eq(
        storage.try_switch_region_storage(false).unwrap(),
        storage.endpoint()
    ));
    storage.save_region(&paths, &region).await.unwrap();
    assert_eq!(
        storage.load_region(&paths, 99).await.unwrap(),
        Some(region.clone())
    );
    assert_eq!(
        storage
            .base()
            .load(paths.region_path(99).as_bytes())
            .await
            .unwrap(),
        go_region_bytes()
    );
    assert_eq!(
        local_view.load_region(&paths, 99).await.unwrap(),
        Some(local_region.clone())
    );

    // Both endpoints keep independent scan cursors, as Go StorageEndpoint does.
    let mut scanned = vec![];
    storage
        .load_regions(&paths, &CancellationToken::new(), |r| {
            scanned.push(r.meta().clone());
            vec![]
        })
        .await
        .unwrap();
    assert_eq!(scanned, vec![region.clone()]);
    storage.try_switch_region_storage(true).unwrap();
    assert_eq!(
        storage.load_region(&paths, 99).await.unwrap(),
        Some(local_region.clone())
    );
    scanned.clear();
    storage
        .load_regions(&paths, &CancellationToken::new(), |r| {
            scanned.push(r.meta().clone());
            vec![]
        })
        .await
        .unwrap();
    assert_eq!(scanned, vec![local_region.clone()]);

    // All non-region adapters and callback transactions retain the default KV.
    storage
        .endpoint()
        .save_json(b"/switch/config", &1)
        .await
        .unwrap();
    storage
        .base()
        .run_in_txn(async |txn| txn.save(b"/switch/txn", b"default"))
        .await
        .unwrap();
    assert_eq!(
        default_view
            .load_json::<u64>(b"/switch/config")
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        local_view.load(b"/switch/config").await.unwrap(),
        LoadState::Missing
    );
    assert_eq!(
        local_view.load(b"/switch/txn").await.unwrap(),
        LoadState::Missing
    );
    assert_eq!(
        storage.base().load(b"/switch/txn").await.unwrap(),
        b"default"
    );

    storage.delete_region(&paths, &region).await.unwrap();
    assert_eq!(storage.load_region(&paths, 99).await.unwrap(), None);
    assert_eq!(
        default_view.load_region(&paths, 99).await.unwrap(),
        Some(region.clone())
    );
    // Decoder error and found flag must originate from the selected source.
    Base::new(local.as_ref())
        .save(paths.region_path(99).as_bytes(), &[255])
        .await
        .unwrap();
    let error = storage.load_region(&paths, 99).await.unwrap_err();
    assert!(error.found);
    assert!(matches!(error.source, RegionMetaError::Endpoint(_)));
    storage.try_switch_region_storage(false).unwrap();
    assert_eq!(
        storage.load_region(&paths, 99).await.unwrap(),
        Some(region.clone())
    );
    storage.delete_region(&paths, &region).await.unwrap();
    assert_eq!(default_view.load_region(&paths, 99).await.unwrap(), None);
    assert!(local_view.load_region(&paths, 99).await.unwrap_err().found);

    // Plain facades remain usable, return None for both switch requests.
    let mut plain = Storage::new(local);
    assert!(plain.try_switch_region_storage(true).is_none());
    assert!(plain.try_switch_region_storage(false).is_none());
    assert!(std::ptr::eq(
        plain.retrieve_region_storage(),
        plain.endpoint()
    ));
    plain.save_region(&paths, &region).await.unwrap();
    assert_eq!(
        plain.load_region(&paths, 99).await.unwrap(),
        Some(region.clone())
    );
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(matches!(
        plain.load_regions(&paths, &cancelled, |_| vec![]).await,
        Err(RegionScanError::Cancelled)
    ));
    plain.delete_region(&paths, &region).await.unwrap();
    assert_eq!(plain.load_region(&paths, 99).await.unwrap(), None);
}

#[tokio::test]
async fn region_switch_memory() {
    contract(
        Arc::new(MemoryStore::default()),
        Arc::new(MemoryStore::default()),
    )
    .await;
}

#[tokio::test]
async fn region_switch_etcd() {
    let fixture = fixture::Fixture::start();
    contract(
        Arc::new(fixture.store().await),
        Arc::new(MemoryStore::default()),
    )
    .await;
}
