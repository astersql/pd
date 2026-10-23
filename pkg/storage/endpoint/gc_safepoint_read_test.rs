// Copyright 2026 AsterSQL.

use super::*;
use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
use astersql_pd_storage_kv::{Base, Consistency, MemoryStore, MetaStore, Read};
use std::sync::Arc;

async fn contract(store: Arc<dyn MetaStore>) {
    let paths = ClusterPaths::new(0);
    let ep = StorageEndpoint::new(store.clone());
    let base = Base::new(store.as_ref());
    // Go TestDataPhysicalRepresentation writes and asserts these actual persisted bytes.
    for (id, raw, expected) in [
        (NULL_KEYSPACE_ID, "654882009e40000", 456139133457530880),
        (
            1111,
            r#"{"keyspace_id":1111,"safe_point":456139133457530881}"#,
            456139133457530881,
        ),
    ] {
        base.save(paths.gc_safe_point_path(id).as_bytes(), raw.as_bytes())
            .await
            .unwrap();
        assert_eq!(ep.load_gc_safe_point(&paths, id).await.unwrap(), expected);
    }
    for (id, raw, expected) in [
        (NULL_KEYSPACE_ID, "456139133457530882", 456139133457530882),
        (2222, "456139133457530883", 456139133457530883),
    ] {
        base.save(paths.txn_safe_point_path(id).as_bytes(), raw.as_bytes())
            .await
            .unwrap();
        assert_eq!(ep.load_txn_safe_point(&paths, id).await.unwrap(), expected);
    }
    for id in [NULL_KEYSPACE_ID, 0, 1000] {
        base.remove(paths.gc_safe_point_path(id).as_bytes())
            .await
            .unwrap();
        base.remove(paths.txn_safe_point_path(id).as_bytes())
            .await
            .unwrap();
        assert_eq!(ep.load_gc_safe_point(&paths, id).await.unwrap(), 0);
        assert_eq!(ep.load_txn_safe_point(&paths, id).await.unwrap(), 0);
        for value in [0, 1, 233, 23333333333, u64::MAX] {
            let raw = if id == NULL_KEYSPACE_ID {
                format!("{value:x}")
            } else {
                format!(r#"{{"keyspace_id":{id},"safe_point":{value}}}"#)
            };
            base.save(paths.gc_safe_point_path(id).as_bytes(), raw.as_bytes())
                .await
                .unwrap();
            base.save(
                paths.txn_safe_point_path(id).as_bytes(),
                value.to_string().as_bytes(),
            )
            .await
            .unwrap();
            assert_eq!(ep.load_gc_safe_point(&paths, id).await.unwrap(), value);
            assert_eq!(ep.load_txn_safe_point(&paths, id).await.unwrap(), value);
        }
        base.save(paths.gc_safe_point_path(id).as_bytes(), b"")
            .await
            .unwrap();
        base.save(paths.txn_safe_point_path(id).as_bytes(), b"")
            .await
            .unwrap();
        assert_eq!(ep.load_gc_safe_point(&paths, id).await.unwrap(), 0);
        assert_eq!(ep.load_txn_safe_point(&paths, id).await.unwrap(), 0);
    }
    for (raw, value) in [
        ("null", 0),
        ("{}", 0),
        (
            r#"{"SAFE_POINT":12,"safe_point":null,"keyspace_id":999,"unknown":[]}"#,
            12,
        ),
        (r#"{"safe_point":1,"safe_point":2}"#, 2),
    ] {
        base.save(paths.gc_safe_point_path(0).as_bytes(), raw.as_bytes())
            .await
            .unwrap();
        assert_eq!(ep.load_gc_safe_point(&paths, 0).await.unwrap(), value);
    }
    for raw in [
        "[]",
        "true",
        "1",
        "bad",
        r#"{"safe_point":-1}"#,
        r#"{"safe_point":1.0}"#,
        r#"{"safe_point":"1"}"#,
        r#"{"safe_point":18446744073709551616}"#,
        r#"{"keyspace_id":4294967296}"#,
        r#"{"safe_point":1} garbage"#,
    ] {
        base.save(paths.gc_safe_point_path(0).as_bytes(), raw.as_bytes())
            .await
            .unwrap();
        assert!(
            matches!(
                ep.load_gc_safe_point(&paths, 0).await,
                Err(SafePointReadError::JsonUnmarshal(_))
            ),
            "{raw}"
        );
    }
    for raw in [
        b"+1".as_slice(),
        b"-1",
        b" 1",
        b"1\n",
        b"0x1",
        b"1_0",
        b"g",
        b"18446744073709551616",
        b"fffffffffffffffff",
        b"\xff",
    ] {
        base.save(paths.gc_safe_point_path(NULL_KEYSPACE_ID).as_bytes(), raw)
            .await
            .unwrap();
        base.save(paths.txn_safe_point_path(0).as_bytes(), raw)
            .await
            .unwrap();
        assert!(
            matches!(
                ep.load_gc_safe_point(&paths, NULL_KEYSPACE_ID).await,
                Err(SafePointReadError::ParseUint)
            ),
            "{raw:?}"
        );
        assert!(
            matches!(
                ep.load_txn_safe_point(&paths, 0).await,
                Err(SafePointReadError::ParseUint)
            ),
            "{raw:?}"
        );
    }
    base.save(
        paths.gc_safe_point_path(NULL_KEYSPACE_ID).as_bytes(),
        b"000FF",
    )
    .await
    .unwrap();
    assert_eq!(
        ep.load_gc_safe_point(&paths, NULL_KEYSPACE_ID)
            .await
            .unwrap(),
        255
    );
    base.save(paths.txn_safe_point_path(0).as_bytes(), b"00010")
        .await
        .unwrap();
    assert_eq!(ep.load_txn_safe_point(&paths, 0).await.unwrap(), 10);
    // Every load is read-only, including failures; revisions and values stay intact.
    let before = store
        .read(Read::prefix(b"/", Consistency::Linearizable))
        .await
        .unwrap();
    let _ = ep.load_gc_safe_point(&paths, 0).await;
    ep.load_gc_safe_point(&paths, NULL_KEYSPACE_ID)
        .await
        .unwrap();
    ep.load_txn_safe_point(&paths, 0).await.unwrap();
    let after = store
        .read(Read::prefix(b"/", Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(before.entries, after.entries);
}
#[tokio::test]
async fn gc_safepoint_read_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn gc_safepoint_read_etcd() {
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
    let paths = ClusterPaths::new(29);
    let store = Arc::new(fixture.store().await);
    Base::new(store.as_ref())
        .save(
            paths.gc_safe_point_path(0).as_bytes(),
            br#"{"safe_point":99}"#,
        )
        .await
        .unwrap();
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let ep = StorageEndpoint::new(Arc::new(fixture.store().await));
    assert_eq!(ep.load_gc_safe_point(&paths, 0).await.unwrap(), 99);
    let began = std::time::Instant::now();
    drop(fixture);
    assert!(matches!(
        ep.load_gc_safe_point(&paths, 0).await,
        Err(SafePointReadError::Store(_))
    ));
    assert!(matches!(
        ep.load_txn_safe_point(&paths, 0).await,
        Err(SafePointReadError::Store(_))
    ));
    eprintln!(
        "gc_safepoint_read: backend stopped before reads; both return Store error after {:?}",
        began.elapsed()
    );
}
