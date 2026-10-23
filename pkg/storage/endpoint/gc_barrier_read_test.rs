// Copyright 2026 AsterSQL.

use super::*;
use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
use astersql_pd_storage_kv::{Base, Consistency, MemoryStore, MetaStore, Read};
use std::sync::Arc;

// Existing Go TestDataPhysicalRepresentation persists and asserts these exact bytes.
const SSP: &[u8] = br#"{"service_id":"gc_worker","expired_at":9223372036854775807,"safe_point":456139133457530884}"#;
// Actual Go binary output captured by batch 47; independent of Rust serialization.
const GLOBAL: &[u8] = r#"{"barrier_id":"屏障 \u003c\u003e\u0026 \u2028 end3600","barrier_ts":18446744073709551614,"expiration_time":1792742540}"#.as_bytes();

async fn contract(store: Arc<dyn MetaStore>) {
    let ep = StorageEndpoint::new(store.clone());
    let base = Base::new(store.as_ref());
    let paths = ClusterPaths::new(0);
    for scope in [NULL_KEYSPACE_ID, 0, 1000] {
        assert_eq!(
            ep.load_gc_barrier(&paths, scope, "missing").await.unwrap(),
            None
        );
        assert!(
            ep.load_all_gc_barriers(&paths, scope)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            ep.compatible_load_all_service_gc_safe_points(&paths, scope)
                .await
                .unwrap(),
            (vec![], vec![])
        );
        let key = paths.gc_barrier_path(scope, "gc_worker");
        base.save(key.as_bytes(), SSP).await.unwrap();
        let b = ep
            .load_gc_barrier(&paths, scope, "gc_worker")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(b.barrier_ts, 456139133457530884);
        assert_eq!(b.barrier_id, "gc_worker");
        assert_eq!(b.expiration_time, None);
        assert_eq!(
            ep.load_all_gc_barriers(&paths, scope).await.unwrap(),
            vec![Some(b.clone())]
        );
        let (keys, ssps) = ep
            .compatible_load_all_service_gc_safe_points(&paths, scope)
            .await
            .unwrap();
        assert_eq!(keys, vec![key.as_bytes().to_vec()]);
        assert_eq!(ssps[0].as_ref().unwrap().to_json().unwrap(), SSP);
        // Embedded scope is legacy data, not a validation against the path scope.
        assert_eq!(ssps[0].as_ref().unwrap().keyspace_id, NULL_KEYSPACE_ID);
        for (raw, expected) in [
            (b"null".as_slice(), None),
            (b"", None),
            (b"{}", Some(GCBarrier::default())),
        ] {
            base.save(key.as_bytes(), raw).await.unwrap();
            assert_eq!(
                ep.load_gc_barrier(&paths, scope, "gc_worker")
                    .await
                    .unwrap(),
                expected
            );
            if raw.is_empty() {
                assert!(matches!(
                    ep.load_all_gc_barriers(&paths, scope).await,
                    Err(EndpointError::JsonUnmarshal(_))
                ));
            } else {
                assert_eq!(
                    ep.load_all_gc_barriers(&paths, scope).await.unwrap(),
                    vec![expected]
                );
            }
        }
        for raw in [
            b"bad".as_slice(),
            b"[]",
            br#"{"safe_point":-1}"#,
            br#"{"expired_at":"1"}"#,
            br#"{"keyspace_id":4294967296}"#,
        ] {
            base.save(key.as_bytes(), raw).await.unwrap();
            assert!(matches!(
                ep.load_gc_barrier(&paths, scope, "gc_worker").await,
                Err(EndpointError::JsonUnmarshal(_))
            ));
            assert!(matches!(
                ep.load_all_gc_barriers(&paths, scope).await,
                Err(EndpointError::JsonUnmarshal(_))
            ));
            assert!(matches!(
                ep.compatible_load_all_service_gc_safe_points(&paths, scope)
                    .await,
                Err(EndpointError::JsonUnmarshal(_))
            ));
        }
        base.remove(key.as_bytes()).await.unwrap();
        // Prefix reads must be unlimited and keep byte ordering, nested IDs and expiration.
        for i in 0..207 {
            let id = format!("{i:03}/nested");
            let raw = format!(r#"{{"service_id":"value-{i}","expired_at":1,"safe_point":{i}}}"#);
            base.save(paths.gc_barrier_path(scope, &id).as_bytes(), raw.as_bytes())
                .await
                .unwrap();
        }
        let values = ep.load_all_gc_barriers(&paths, scope).await.unwrap();
        assert_eq!(values.len(), 207, "barrier prefix must not truncate at 100");
        for (i, value) in values.iter().enumerate() {
            let b = value.as_ref().unwrap();
            assert_eq!(b.barrier_id, format!("value-{i}"));
            assert_eq!(b.barrier_ts, i as u64);
            assert!(b.is_expired(BarrierTime::from_unix(2, 0)));
            assert_eq!(
                ep.load_gc_barrier(&paths, scope, &format!("{i:03}/nested"))
                    .await
                    .unwrap(),
                *value
            );
        }
        let prefix = paths.compatible_tidb_min_start_ts_prefix(scope);
        assert_eq!(
            ep.compatible_load_tidb_min_start_ts(&paths, scope)
                .await
                .unwrap(),
            (vec![], 0)
        );
        for (id, raw) in [("a", "18446744073709551615"), ("b", "00014"), ("c", "14")] {
            base.save(format!("{prefix}{id}").as_bytes(), raw.as_bytes())
                .await
                .unwrap();
        }
        assert_eq!(
            ep.compatible_load_tidb_min_start_ts(&paths, scope)
                .await
                .unwrap(),
            (b"b".to_vec(), 14)
        );
        for raw in ["", "+1", "-1", " 1", "1.0", "18446744073709551616", "0xff"] {
            base.save(format!("{prefix}a").as_bytes(), raw.as_bytes())
                .await
                .unwrap();
            assert!(matches!(
                ep.compatible_load_tidb_min_start_ts(&paths, scope).await,
                Err(SafePointReadError::ParseUint)
            ));
        }
    }
    assert!(
        ep.load_all_gc_barriers(&ClusterPaths::new(99), 0)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        ep.load_global_gc_barrier(&paths, "missing")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        ep.load_all_global_gc_barriers(&paths)
            .await
            .unwrap()
            .is_empty()
    );
    let key = paths.global_gc_barrier_path("key-does-not-match-value");
    base.save(key.as_bytes(), GLOBAL).await.unwrap();
    let g = ep
        .load_global_gc_barrier(&paths, "key-does-not-match-value")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(g.to_json().unwrap(), GLOBAL);
    assert_eq!(g.barrier_ts, u64::MAX - 1);
    assert_eq!(
        ep.load_all_global_gc_barriers(&paths).await.unwrap(),
        vec![Some(g)]
    );
    for raw in [
        b"null".as_slice(),
        b"",
        b"{}",
        b"bad",
        b"[]",
        br#"{"expiration_time":"1"}"#,
    ] {
        base.save(key.as_bytes(), raw).await.unwrap();
        let point = ep
            .load_global_gc_barrier(&paths, "key-does-not-match-value")
            .await;
        let all = ep.load_all_global_gc_barriers(&paths).await;
        if raw.is_empty() {
            assert_eq!(point.unwrap(), None);
            assert!(matches!(all, Err(EndpointError::JsonUnmarshal(_))));
        } else if raw == b"null" || raw == b"{}" {
            assert_eq!(all.unwrap(), vec![point.unwrap()]);
        } else {
            assert!(matches!(point, Err(EndpointError::JsonUnmarshal(_))));
            assert!(matches!(all, Err(EndpointError::JsonUnmarshal(_))));
        }
    }
    base.remove(key.as_bytes()).await.unwrap();
    for i in 0..207 {
        base.save(
            paths.global_gc_barrier_path(&format!("{i:03}")).as_bytes(),
            GLOBAL,
        )
        .await
        .unwrap();
    }
    assert_eq!(
        ep.load_all_global_gc_barriers(&paths).await.unwrap().len(),
        207
    );
    let before = store
        .read(Read::prefix(b"", Consistency::Linearizable))
        .await
        .unwrap();
    ep.load_all_global_gc_barriers(&paths).await.unwrap();
    let _ = ep.compatible_load_tidb_min_start_ts(&paths, 0).await;
    ep.load_all_gc_barriers(&paths, 0).await.unwrap();
    let after = store
        .read(Read::prefix(b"", Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(before.entries, after.entries);
}

#[tokio::test]
async fn gc_barrier_read_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}

#[tokio::test]
async fn gc_barrier_read_etcd() {
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
    let paths = ClusterPaths::new(30);
    let store = Arc::new(fixture.store().await);
    Base::new(store.as_ref())
        .save(paths.gc_barrier_path(0, "persisted").as_bytes(), SSP)
        .await
        .unwrap();
    Base::new(store.as_ref())
        .save(paths.global_gc_barrier_path("persisted").as_bytes(), GLOBAL)
        .await
        .unwrap();
    let began = std::time::Instant::now();
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let ep = StorageEndpoint::new(Arc::new(fixture.store().await));
    assert!(
        ep.load_gc_barrier(&paths, 0, "persisted")
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        ep.load_global_gc_barrier(&paths, "persisted")
            .await
            .unwrap()
            .is_some()
    );
    eprintln!(
        "gc_barrier_read: same WAL restarted and both values recovered after {:?}",
        began.elapsed()
    );
    drop(fixture);
    assert!(matches!(
        ep.load_gc_barrier(&paths, 0, "persisted").await,
        Err(EndpointError::Store(_))
    ));
    assert!(matches!(
        ep.load_all_gc_barriers(&paths, 0).await,
        Err(EndpointError::Store(_))
    ));
    assert!(matches!(
        ep.load_global_gc_barrier(&paths, "persisted").await,
        Err(EndpointError::Store(_))
    ));
    assert!(matches!(
        ep.load_all_global_gc_barriers(&paths).await,
        Err(EndpointError::Store(_))
    ));
    assert!(matches!(
        ep.compatible_load_all_service_gc_safe_points(&paths, 0)
            .await,
        Err(EndpointError::Store(_))
    ));
    assert!(matches!(
        ep.compatible_load_tidb_min_start_ts(&paths, 0).await,
        Err(SafePointReadError::Store(_))
    ));
    eprintln!(
        "gc_barrier_read: backend closed before six reads; all failed by {:?}",
        began.elapsed()
    );
}

#[test]
fn gc_barrier_read_values() {
    let ssp = ServiceSafePoint::from_json(SSP).unwrap();
    assert_eq!(ssp.to_json().unwrap(), SSP);
    let barrier = GCBarrier::from_service_safe_point(&ssp);
    assert_eq!(barrier.barrier_id, "gc_worker");
    assert_eq!(barrier.barrier_ts, 456139133457530884);
    assert_eq!(barrier.expiration_time, None);
    let global = GlobalGCBarrier::from_json(GLOBAL).unwrap();
    assert_eq!(global.to_json().unwrap(), GLOBAL);
    assert!(!global.is_expired(BarrierTime::from_unix(1792742540, 0)));
    assert!(global.is_expired(BarrierTime::from_unix(1792742540, 1)));
}
