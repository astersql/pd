// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Base, MemoryStore, MetaStore};
use std::sync::Arc;

async fn contract(store: Arc<dyn MetaStore>) {
    let endpoint = StorageEndpoint::new(store.clone());
    let base = Base::new(store.as_ref());
    let paths = ClusterPaths::new(42);
    let other = ClusterPaths::new(43);
    for external in [false, true] {
        let key = if external {
            paths.external_timestamp_path()
        } else {
            paths.min_resolved_ts_path()
        };
        macro_rules! load {
            () => {
                if external {
                    endpoint.load_external_ts(&paths).await
                } else {
                    endpoint.load_min_resolved_ts(&paths).await
                }
            };
        }
        macro_rules! save {
            ($value:expr) => {
                if external {
                    endpoint.save_external_ts(&paths, $value).await
                } else {
                    endpoint.save_min_resolved_ts(&paths, $value).await
                }
            };
        }
        assert_eq!(load!().unwrap(), 0);
        base.save(key.as_bytes(), b"").await.unwrap();
        assert_eq!(load!().unwrap(), 0);
        // 42 -> 2a is actual Go ExampleFormatUint stdout, checked by its runner.
        // 16/MAX_I64 are Go TestItoa assertions. MAX_U64 also matches Go
        // TestSprintf after removing its explicit 0x prefix; other boundaries
        // are source-derived rather than claimed as runtime Go differentials.
        for (value, bytes) in [
            (0, b"0".as_slice()),
            (16, b"10"),
            (42, b"2a"),
            (i64::MAX as u64, b"7fffffffffffffff"),
            ((1u64 << 53) + 1, b"20000000000001"),
            (1u64 << 63, b"8000000000000000"),
            (u64::MAX, b"ffffffffffffffff"),
        ] {
            save!(value).unwrap();
            assert_eq!(base.load(key.as_bytes()).await.unwrap(), bytes);
            assert_eq!(load!().unwrap(), value);
            base.save(key.as_bytes(), bytes).await.unwrap();
            assert_eq!(load!().unwrap(), value);
        }
        for (bytes, value) in [
            (b"000a".as_slice(), 10),
            (b"ABCDEF", 0xabcdef),
            (b"000FFFFFFFFFFFFFFFF", u64::MAX),
            (b"10", 16),
        ] {
            base.save(key.as_bytes(), bytes).await.unwrap();
            assert_eq!(load!().unwrap(), value);
        }
        // These are raw hex strings, not JSON numbers or HTTP wrapper objects.
        for bad in [
            b"{".as_slice(),
            b"null",
            b"{}",
            b"[]",
            b"\"2a\"",
            b"+1",
            b"-1",
            b"0x10",
            b"1_0",
            b" 1",
            b"1\n",
            b"10000000000000000",
            b"g",
            b"\xff",
            b"1\0",
        ] {
            base.save(key.as_bytes(), bad).await.unwrap();
            assert!(
                matches!(load!(), Err(TimestampValueError::ParseUint)),
                "{bad:?}"
            );
            assert_eq!(base.load(key.as_bytes()).await.unwrap(), bad);
        }
        save!(u64::MAX).unwrap();
    }
    endpoint.save_min_resolved_ts(&paths, 42).await.unwrap();
    assert_eq!(endpoint.load_external_ts(&paths).await.unwrap(), u64::MAX);
    assert_eq!(endpoint.load_min_resolved_ts(&other).await.unwrap(), 0);
    assert_eq!(endpoint.load_external_ts(&other).await.unwrap(), 0);
    assert_eq!(
        base.load(b"/pd/42/raft/min_resolved_ts").await.unwrap(),
        b"2a"
    );
    assert_eq!(
        base.load(b"/pd/42/raft/external_timestamp").await.unwrap(),
        b"ffffffffffffffff"
    );
}
#[tokio::test]
async fn timestamp_values_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn timestamp_values_etcd() {
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let endpoint = StorageEndpoint::new(Arc::new(fixture.store().await));
    let paths = ClusterPaths::new(42);
    assert_eq!(endpoint.load_min_resolved_ts(&paths).await.unwrap(), 42);
    assert_eq!(endpoint.load_external_ts(&paths).await.unwrap(), u64::MAX);
    drop(fixture);
    assert!(matches!(
        endpoint.load_min_resolved_ts(&paths).await,
        Err(TimestampValueError::Store(_))
    ));
    assert!(matches!(
        endpoint.load_external_ts(&paths).await,
        Err(TimestampValueError::Store(_))
    ));
    assert!(matches!(
        endpoint.save_min_resolved_ts(&paths, 1).await,
        Err(TimestampValueError::Store(_))
    ));
    assert!(matches!(
        endpoint.save_external_ts(&paths, 1).await,
        Err(TimestampValueError::Store(_))
    ));
}
#[test]
fn timestamp_values_go_bytes() {
    assert_eq!(format!("{:x}", 42), "2a");
    assert_eq!(format!("{:x}", 16), "10");
    assert_eq!(format!("{:x}", i64::MAX), "7fffffffffffffff");
}
