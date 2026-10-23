// Copyright 2026 AsterSQL.
use crate::{fixture, *};

async fn snapshot(store: &dyn MetaStore) -> Snapshot {
    store
        .read(Read::prefix(b"", Consistency::Linearizable))
        .await
        .unwrap()
}

async fn callback_txn_contract(store: &dyn MetaStore) {
    let base = Base::new(store);
    base.save(b"old", b"original").await.unwrap();
    let before = snapshot(store).await;
    let aborted = base
        .run_in_txn(async |txn| {
            txn.save(b"new", b"buffered").unwrap();
            txn.remove(b"old").unwrap();
            assert_eq!(txn.load(b"new").await.unwrap(), b"");
            assert_eq!(txn.load(b"old").await.unwrap(), b"original");
            assert_eq!(txn.load_range(b"", b"\0", 0).await.unwrap(), before.entries);
            Err::<(), _>("caller sentinel")
        })
        .await;
    assert_eq!(aborted, Err(CallbackTxnError::Callback("caller sentinel")));
    let after = snapshot(store).await;
    assert_eq!(after.entries, before.entries);
    assert_eq!(after.revision, before.revision, "abort must not commit");

    let result = base
        .run_in_txn(async |txn| {
            txn.save(b"new", b"committed")?;
            txn.save(b"other", b"same revision")?;
            txn.remove(b"old")?;
            assert_eq!(txn.load(b"new").await?, b"");
            assert_eq!(txn.load(b"old").await?, b"original");
            Ok::<_, StoreError>(42)
        })
        .await
        .unwrap();
    assert_eq!(result, 42);
    assert_eq!(base.load(b"new").await.unwrap(), b"committed");
    assert_eq!(base.load(b"old").await.unwrap(), b"");
    let after = snapshot(store).await;
    assert_eq!(after.revision, before.revision + 1);
    assert!(
        after
            .entries
            .iter()
            .all(|e| e.mod_revision == after.revision)
    );

    // Deterministic interleaving: a backend write occurs after the callback's read,
    // before the runner can commit its buffered output.
    let before = snapshot(store).await.revision;
    let conflict = base
        .run_in_txn(async |txn| {
            assert_eq!(txn.load(b"new").await?, b"committed");
            base.save(b"new", b"concurrent").await?;
            txn.save(b"blocked", b"must not exist")?;
            txn.remove(b"other")?;
            Ok::<_, StoreError>(())
        })
        .await;
    assert_eq!(conflict, Err(CallbackTxnError::Conflict));
    assert_eq!(snapshot(store).await.revision, before + 1);
    assert_eq!(base.load(b"blocked").await.unwrap(), b"");
    assert_eq!(base.load(b"other").await.unwrap(), b"same revision");

    // Missing must compare create revision, even if another writer inserts empty bytes.
    let conflict = base
        .run_in_txn(async |txn| {
            assert_eq!(txn.load(b"missing").await?, b"");
            base.save(b"missing", b"").await?;
            txn.save(b"blocked", b"must not exist")?;
            Ok::<_, StoreError>(())
        })
        .await;
    assert_eq!(conflict, Err(CallbackTxnError::Conflict));
    assert_eq!(base.load(b"blocked").await.unwrap(), b"");

    // Value comparisons allow same-value rewrites (revision protection is a later task).
    base.run_in_txn(async |txn| {
        assert_eq!(txn.load(b"new").await?, b"concurrent");
        base.save(b"new", b"concurrent").await?;
        txn.save(b"same-value", b"accepted")?;
        Ok::<_, StoreError>(())
    })
    .await
    .unwrap();
    assert_eq!(base.load(b"same-value").await.unwrap(), b"accepted");

    base.save(b"range/a", b"a").await.unwrap();
    base.save(b"range/b", b"b").await.unwrap();
    let conflict = base
        .run_in_txn(async |txn| {
            let entries = txn.load_range(b"range/", b"\0", 1).await?;
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].key, b"range/a");
            base.save(b"range/a", b"changed").await?;
            txn.save(b"blocked", b"must not exist")?;
            Ok::<_, StoreError>(())
        })
        .await;
    assert_eq!(conflict, Err(CallbackTxnError::Conflict));
    assert_eq!(base.load(b"blocked").await.unwrap(), b"");

    base.run_in_txn(async |txn| {
        assert_eq!(txn.load_range(b"range/", b"\0", 1).await?.len(), 1);
        base.save(b"range/b", b"outside limit").await?;
        base.save(b"range/c", b"phantom").await?;
        assert!(txn.load_range(b"empty/", b"\0", 0).await?.is_empty());
        base.save(b"empty/phantom", b"inserted").await?;
        txn.save(b"phantoms", b"accepted")?;
        Ok::<_, StoreError>(())
    })
    .await
    .unwrap();
    assert_eq!(base.load(b"phantoms").await.unwrap(), b"accepted");

    for (expected, result) in [
        (b"stale".as_slice(), Err(CallbackTxnError::Conflict)),
        (b"concurrent", Ok(())),
    ] {
        let actual = base
            .run_in_txn_with_conditions(
                vec![Compare::Value(b"new".to_vec(), expected.to_vec())],
                async |txn| {
                    txn.save(b"guarded", b"accepted")?;
                    Ok::<_, StoreError>(())
                },
            )
            .await;
        assert_eq!(actual, result);
        assert_eq!(
            base.load(b"guarded").await.unwrap(),
            if expected == b"stale" {
                b"".as_slice()
            } else {
                b"accepted"
            }
        );
    }

    let before = snapshot(store).await;
    let invalid = base
        .run_in_txn(async |txn| {
            txn.save(b"valid", b"atomic")?;
            txn.save(b"", b"invalid")?;
            Ok::<_, StoreError>(())
        })
        .await;
    assert_eq!(
        invalid,
        Err(CallbackTxnError::Store(StoreError::InvalidArgument))
    );
    let after = snapshot(store).await;
    assert_eq!(after.entries, before.entries);
    assert_eq!(after.revision, before.revision);

    let read_error = base
        .run_in_txn(async |txn| {
            txn.save(b"valid", b"must not commit")?;
            txn.load(b"").await?;
            Ok::<_, StoreError>(())
        })
        .await;
    assert_eq!(
        read_error,
        Err(CallbackTxnError::Callback(StoreError::InvalidArgument))
    );
    assert_eq!(snapshot(store).await.entries, before.entries);
}

#[tokio::test]
async fn callback_txn_memory() {
    callback_txn_contract(&MemoryStore::default()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn callback_txn_etcd() {
    let fixture = fixture::Fixture::start();
    callback_txn_contract(&fixture.store().await).await;
}
