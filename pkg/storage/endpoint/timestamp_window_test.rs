// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Base, MemoryStore, MetaStore, StoreError};
use std::{
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;

// Real Go TestUint64ToBytes output preserved in task 3's evidence.
const GO_1000: &[u8] = b"\x00\x00\x00\x00\x00\x00\x03\xe8";
async fn contract(store: Arc<dyn MetaStore>) {
    let endpoint = StorageEndpoint::new(store.clone());
    let base = Base::new(store.as_ref());
    let paths = ClusterPaths::new(42);
    let other = ClusterPaths::new(43);
    let cancel = CancellationToken::new();
    for id in [0, 1, 10, 100_000, u32::MAX] {
        let key = paths.timestamp_path(id);
        assert_eq!(endpoint.load_timestamp(&paths, id).await.unwrap(), None);
        endpoint
            .delete_timestamp(&paths, id, &cancel)
            .await
            .unwrap();
        base.save(key.as_bytes(), b"").await.unwrap();
        assert_eq!(endpoint.load_timestamp(&paths, id).await.unwrap(), None);
        base.save(key.as_bytes(), GO_1000).await.unwrap();
        assert_eq!(
            endpoint.load_timestamp(&paths, id).await.unwrap(),
            Some(UNIX_EPOCH + Duration::from_nanos(1000))
        );
        assert_eq!(base.load(key.as_bytes()).await.unwrap(), GO_1000);
        // Additional boundaries are source-derived, not extra Go runtime output.
        for nanos in [0, 1, i64::MAX, i64::MIN, -1] {
            let bytes = nanos.to_be_bytes();
            base.save(key.as_bytes(), &bytes).await.unwrap();
            let expected = if nanos >= 0 {
                UNIX_EPOCH + Duration::from_nanos(nanos as u64)
            } else {
                UNIX_EPOCH - Duration::from_nanos(nanos.unsigned_abs())
            };
            assert_eq!(
                endpoint.load_timestamp(&paths, id).await.unwrap(),
                Some(expected)
            );
        }
        for len in [1, 2, 7, 9, 16] {
            let bad = vec![0xff; len];
            base.save(key.as_bytes(), &bad).await.unwrap();
            assert!(
                matches!(endpoint.load_timestamp(&paths, id).await, Err(TimestampWindowError::Decode(error)) if error.actual_len() == len)
            );
            assert_eq!(base.load(key.as_bytes()).await.unwrap(), bad);
        }
        base.save(key.as_bytes(), GO_1000).await.unwrap();
    }
    assert_eq!(base.load(b"/pd/42/timestamp").await.unwrap(), GO_1000);
    assert_eq!(
        base.load(b"/ms/42/tso/00001/gta/timestamp").await.unwrap(),
        GO_1000
    );
    base.save(other.timestamp_path(1).as_bytes(), GO_1000)
        .await
        .unwrap();
    base.save(b"/ms/42/tso/00001/gta/timestamp/neighbor", b"keep")
        .await
        .unwrap();
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(matches!(
        endpoint.delete_timestamp(&paths, 1, &cancelled).await,
        Err(TimestampWindowError::Store(StoreError::Cancelled))
    ));
    assert!(endpoint.load_timestamp(&paths, 1).await.unwrap().is_some());
    endpoint.delete_timestamp(&paths, 1, &cancel).await.unwrap();
    endpoint.delete_timestamp(&paths, 1, &cancel).await.unwrap();
    assert_eq!(endpoint.load_timestamp(&paths, 1).await.unwrap(), None);
    for id in [0, 10, 100_000, u32::MAX] {
        assert!(endpoint.load_timestamp(&paths, id).await.unwrap().is_some());
    }
    assert!(endpoint.load_timestamp(&other, 1).await.unwrap().is_some());
    assert_eq!(
        base.load(b"/ms/42/tso/00001/gta/timestamp/neighbor")
            .await
            .unwrap(),
        b"keep"
    );
}
#[tokio::test]
async fn timestamp_window_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn timestamp_window_etcd() {
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let endpoint = StorageEndpoint::new(Arc::new(fixture.store().await));
    let paths = ClusterPaths::new(42);
    assert_eq!(endpoint.load_timestamp(&paths, 1).await.unwrap(), None);
    assert_eq!(
        endpoint.load_timestamp(&paths, 0).await.unwrap(),
        Some(UNIX_EPOCH + Duration::from_nanos(1000))
    );
    drop(fixture);
    assert!(matches!(
        endpoint.load_timestamp(&paths, 0).await,
        Err(TimestampWindowError::Store(_))
    ));
    assert!(matches!(
        endpoint
            .delete_timestamp(&paths, 0, &CancellationToken::new())
            .await,
        Err(TimestampWindowError::Store(_))
    ));
}
#[test]
fn timestamp_window_go_bytes() {
    assert_eq!(astersql_pd_typeutil::u64_to_bytes(1000), GO_1000);
    assert_eq!(
        astersql_pd_typeutil::parse_timestamp(GO_1000).unwrap(),
        UNIX_EPOCH + Duration::from_nanos(1000)
    );
}
