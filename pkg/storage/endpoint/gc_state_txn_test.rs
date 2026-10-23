// Copyright 2026 AsterSQL.

use super::*;
use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
use astersql_pd_storage_kv::{Base, Consistency, MemoryStore, MetaStore, Read};
use std::sync::Arc;

async fn contract(store: Arc<dyn MetaStore>) {
    let paths = ClusterPaths::new(0);
    let ep = StorageEndpoint::new(store.clone());
    let base = Base::new(store.as_ref());
    let revision = paths.gc_state_revision_path();
    ep.run_in_gc_state_transaction(&paths, async |_| Ok::<_, &'static str>(()))
        .await
        .unwrap();
    assert_eq!(
        ep.load(revision.as_bytes()).await.unwrap(),
        LoadState::Missing
    );
    ep.run_in_gc_state_transaction(&paths, async |wb| {
        wb.set_gc_safe_point(NULL_KEYSPACE_ID, 456139133457530880);
        wb.set_gc_safe_point(1111, 456139133457530881);
        wb.set_txn_safe_point(NULL_KEYSPACE_ID, 456139133457530882);
        wb.set_txn_safe_point(2222, 456139133457530883);
        // Buffered writes are invisible to normal reads during the callback.
        assert_eq!(ep.load_gc_safe_point(&paths, 1111).await.unwrap(), 0);
        Ok::<_, &'static str>(())
    })
    .await
    .unwrap();
    // Go TestDataPhysicalRepresentation actually stores and checks these bytes.
    for (key, value) in [
        (
            paths.gc_safe_point_path(NULL_KEYSPACE_ID),
            "654882009e40000",
        ),
        (
            paths.gc_safe_point_path(1111),
            r#"{"keyspace_id":1111,"safe_point":456139133457530881}"#,
        ),
        (
            paths.txn_safe_point_path(NULL_KEYSPACE_ID),
            "456139133457530882",
        ),
        (paths.txn_safe_point_path(2222), "456139133457530883"),
        (revision.clone(), "1"),
    ] {
        assert_eq!(
            base.load(key.as_bytes()).await.unwrap(),
            value.as_bytes(),
            "{key}"
        );
    }
    let before = store
        .read(Read::prefix(b"/", Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(before.entries.len(), 5);
    assert!(
        before
            .entries
            .iter()
            .all(|e| e.mod_revision == before.entries[0].mod_revision)
    );
    ep.run_in_gc_state_transaction(&paths, async |_| Ok::<_, &'static str>(()))
        .await
        .unwrap();
    let err = ep
        .run_in_gc_state_transaction(&paths, async |wb| {
            wb.set_gc_safe_point(1111, 0);
            wb.set_txn_safe_point(2222, 0);
            Err("callback sentinel")
        })
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        GCStateTxnError::Callback("callback sentinel")
    ));
    assert_eq!(
        before.entries,
        store
            .read(Read::prefix(b"/", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
    );

    // An actual intervening transaction wins; outer writes cannot partially commit.
    for writing in [true, false] {
        let err = ep
            .run_in_gc_state_transaction(&paths, async |wb| {
                if writing {
                    wb.set_gc_safe_point(1111, 99);
                    wb.set_txn_safe_point(2222, 99);
                }
                ep.run_in_gc_state_transaction(&paths, async |inner| {
                    inner.set_gc_safe_point(0, 7);
                    Ok::<_, &'static str>(())
                })
                .await
                .unwrap();
                Ok::<_, &'static str>(())
            })
            .await
            .unwrap_err();
        assert!(matches!(err, GCStateTxnError::Conflict));
        assert_eq!(
            ep.load_gc_safe_point(&paths, 1111).await.unwrap(),
            456139133457530881
        );
        assert_eq!(
            ep.load_txn_safe_point(&paths, 2222).await.unwrap(),
            456139133457530883
        );
    }
    assert_eq!(base.load(revision.as_bytes()).await.unwrap(), b"3");
    for raw in ["x", "+1", "-1", " 1", "1_0", "18446744073709551616"] {
        base.save(revision.as_bytes(), raw.as_bytes())
            .await
            .unwrap();
        let mut called = false;
        let result = ep
            .run_in_gc_state_transaction(&paths, async |_| {
                called = true;
                Ok::<_, &'static str>(())
            })
            .await;
        assert!(
            matches!(result, Err(GCStateTxnError::ParseRevision)),
            "{raw}"
        );
        assert!(!called);
        assert_eq!(
            base.load(revision.as_bytes()).await.unwrap(),
            raw.as_bytes()
        );
    }
    // Empty persisted revision means NotExists, so even read-only transactions conflict.
    base.save(revision.as_bytes(), b"").await.unwrap();
    assert!(matches!(
        ep.run_in_gc_state_transaction(&paths, async |wb| {
            wb.set_gc_safe_point(0, 123);
            Ok::<_, &'static str>(())
        })
        .await,
        Err(GCStateTxnError::Conflict)
    ));
    assert_eq!(ep.load_gc_safe_point(&paths, 0).await.unwrap(), 7);
    // Decimal leading zeroes compare raw bytes; uint64 increment wraps as Go does.
    for (raw, next) in [("00041", "42"), ("18446744073709551615", "0")] {
        base.save(revision.as_bytes(), raw.as_bytes())
            .await
            .unwrap();
        ep.run_in_gc_state_transaction(&paths, async |wb| {
            for id in [0, NULL_KEYSPACE_ID] {
                wb.set_gc_safe_point(id, u64::MAX);
                wb.set_txn_safe_point(id, u64::MAX);
            }
            Ok::<_, &'static str>(())
        })
        .await
        .unwrap();
        assert_eq!(
            base.load(revision.as_bytes()).await.unwrap(),
            next.as_bytes()
        );
        for id in [0, NULL_KEYSPACE_ID] {
            assert_eq!(ep.load_gc_safe_point(&paths, id).await.unwrap(), u64::MAX);
            assert_eq!(ep.load_txn_safe_point(&paths, id).await.unwrap(), u64::MAX);
        }
    }
    // No monotonicity policy belongs to the persistence adapter.
    ep.run_in_gc_state_transaction(&paths, async |wb| {
        for id in [0, NULL_KEYSPACE_ID] {
            wb.set_gc_safe_point(id, 0);
            wb.set_txn_safe_point(id, 0);
        }
        Ok::<_, &'static str>(())
    })
    .await
    .unwrap();
    for id in [0, NULL_KEYSPACE_ID] {
        assert_eq!(ep.load_gc_safe_point(&paths, id).await.unwrap(), 0);
        assert_eq!(ep.load_txn_safe_point(&paths, id).await.unwrap(), 0);
    }
}

#[tokio::test]
async fn gc_state_txn_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}

#[tokio::test]
async fn gc_state_txn_etcd() {
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
    let paths = ClusterPaths::new(0);
    let ep = StorageEndpoint::new(Arc::new(fixture.store().await));
    let began = std::time::Instant::now();
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    assert_eq!(ep.load_gc_safe_point(&paths, 0).await.unwrap(), 0);
    assert_eq!(
        Base::new(ep.store.as_ref())
            .load(paths.gc_state_revision_path().as_bytes())
            .await
            .unwrap(),
        b"1"
    );
    drop(fixture);
    // Each fixture acknowledgement is single-use, so use a fresh backend for the
    // stopped-between-read-and-commit case rather than reusing old ready files.
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    let ep = StorageEndpoint::new(Arc::new(fixture.store().await));
    ep.run_in_gc_state_transaction(&paths, async |wb| {
        wb.set_gc_safe_point(0, 0);
        Ok::<_, &'static str>(())
    })
    .await
    .unwrap();
    // Stop between revision read and commit: real IO failure, no fake txn result.
    let mut called = false;
    let result = ep
        .run_in_gc_state_transaction(&paths, async |wb| {
            called = true;
            wb.set_gc_safe_point(0, 123);
            fixture.command("stop", "stopped");
            Ok::<_, &'static str>(())
        })
        .await;
    assert!(called);
    assert!(matches!(result, Err(GCStateTxnError::Store(_))));
    fixture.command("start", "restarted");
    assert_eq!(ep.load_gc_safe_point(&paths, 0).await.unwrap(), 0);
    assert_eq!(
        Base::new(ep.store.as_ref())
            .load(paths.gc_state_revision_path().as_bytes())
            .await
            .unwrap(),
        b"1"
    );
    drop(fixture);
    called = false;
    let result = ep
        .run_in_gc_state_transaction(&paths, async |_| {
            called = true;
            Ok::<_, &'static str>(())
        })
        .await;
    assert!(matches!(result, Err(GCStateTxnError::Store(_))));
    assert!(!called);
    eprintln!(
        "gc_state_txn: same-WAL restart; stopped after revision read before commit, IO error, restarted state unchanged; final stopped read fails before callback; elapsed {:?}",
        began.elapsed()
    );
}

#[test]
fn gc_state_txn_go_encoding() {
    use super::gc_state_txn::encode_gc_safe_point;
    // Actual bytes asserted by the existing Go TestDataPhysicalRepresentation.
    assert_eq!(
        encode_gc_safe_point(NULL_KEYSPACE_ID, 456139133457530880),
        "654882009e40000"
    );
    assert_eq!(
        encode_gc_safe_point(1111, 456139133457530881),
        r#"{"keyspace_id":1111,"safe_point":456139133457530881}"#
    );
    assert_eq!(
        encode_gc_safe_point(0, 0),
        r#"{"keyspace_id":0,"safe_point":0}"#
    );
}
