// Copyright 2026 AsterSQL.
use crate::{fixture, *};
use tokio::sync::oneshot;

async fn snapshot(store: &dyn MetaStore) -> Snapshot {
    store
        .read(Read::prefix(b"", Consistency::Linearizable))
        .await
        .unwrap()
}

async fn revision_txn_contract(store: &dyn MetaStore) {
    let base = Base::new(store);
    // Both reads return identical bytes; only revision protection rejects a rewrite.
    // Channels form deterministic barriers, without sleeps or simulated transactions.
    for revision in [true, false] {
        base.save(b"source", b"same").await.unwrap();
        base.save(b"keep", b"keep").await.unwrap();
        base.remove(b"output").await.unwrap();
        let before = snapshot(store).await.revision;
        let (read_tx, read_rx) = oneshot::channel();
        let (written_tx, written_rx) = oneshot::channel();
        let reader = base.run_in_txn(async |txn| {
            let value = if revision {
                txn.load_with_revision(b"source").await?
            } else {
                txn.load(b"source").await?
            };
            assert_eq!(value, b"same");
            txn.save(b"output", b"committed")?;
            txn.remove(b"keep")?;
            read_tx.send(()).unwrap();
            written_rx.await.unwrap();
            Ok::<_, StoreError>(())
        });
        let writer = async {
            read_rx.await.unwrap();
            base.save(b"source", b"same").await.unwrap();
            written_tx.send(()).unwrap();
        };
        let (result, ()) = tokio::join!(reader, writer);
        assert_eq!(
            result,
            if revision {
                Err(CallbackTxnError::Conflict)
            } else {
                Ok(())
            }
        );
        assert_eq!(
            base.load(b"output").await.unwrap(),
            if revision {
                b"".as_slice()
            } else {
                b"committed"
            }
        );
        assert_eq!(
            base.load(b"keep").await.unwrap(),
            if revision { b"keep".as_slice() } else { b"" }
        );
        assert_eq!(
            snapshot(store).await.revision,
            before + if revision { 1 } else { 2 }
        );
    }

    // All conflict cases preserve the concurrent state and reject every buffered write.
    for scenario in [
        "changed",
        "deleted",
        "recreated",
        "missing-empty",
        "missing-value",
    ] {
        base.save(b"source", b"same").await.unwrap();
        base.remove(b"missing").await.unwrap();
        base.remove(b"output").await.unwrap();
        let key = if scenario.starts_with("missing") {
            b"missing".as_slice()
        } else {
            b"source"
        };
        let mut concurrent = None;
        let result = base
            .run_in_txn(async |txn| {
                assert_eq!(
                    txn.load_with_revision(key).await?,
                    if scenario.starts_with("missing") {
                        b"".as_slice()
                    } else {
                        b"same"
                    }
                );
                match scenario {
                    "changed" => base.save(key, b"changed").await?,
                    "deleted" => base.remove(key).await?,
                    "recreated" => {
                        base.remove(key).await?;
                        base.save(key, b"same").await?;
                    }
                    "missing-empty" => base.save(key, b"").await?,
                    "missing-value" => base.save(key, b"created").await?,
                    _ => unreachable!(),
                }
                concurrent = Some(snapshot(store).await);
                txn.save(b"output", b"must not commit")?;
                txn.remove(key)?;
                Ok::<_, StoreError>(())
            })
            .await;
        assert_eq!(result, Err(CallbackTxnError::Conflict), "{scenario}");
        let after = snapshot(store).await;
        let expected = concurrent.unwrap();
        assert_eq!(after.entries, expected.entries, "{scenario}");
        assert_eq!(after.revision, expected.revision, "{scenario}");
    }

    // Stable existing, empty, missing and large values commit, including additional guards.
    for value in [b"stable".to_vec(), vec![], vec![0xab; 256 * 1024]] {
        base.save(b"source", &value).await.unwrap();
        base.remove(b"missing").await.unwrap();
        base.run_in_txn_with_conditions(
            vec![Compare::Value(b"source".to_vec(), value.clone())],
            async |txn| {
                txn.save(b"source", b"buffered")?;
                assert_eq!(txn.load_with_revision(b"source").await?, value);
                assert_eq!(txn.load_with_revision(b"missing").await?, b"");
                txn.save(b"missing", b"created")?;
                Ok::<_, StoreError>(())
            },
        )
        .await
        .unwrap();
        assert_eq!(base.load(b"source").await.unwrap(), b"buffered");
        assert_eq!(base.load(b"missing").await.unwrap(), b"created");
    }
    let before = snapshot(store).await;
    let result = base
        .run_in_txn(async |txn| {
            txn.save(b"output", b"must not commit")?;
            txn.load_with_revision(b"").await?;
            Ok::<_, StoreError>(())
        })
        .await;
    assert_eq!(
        result,
        Err(CallbackTxnError::Callback(StoreError::InvalidArgument))
    );
    let after = snapshot(store).await;
    assert_eq!(after.entries, before.entries);
    assert_eq!(after.revision, before.revision);
}

#[tokio::test]
async fn revision_txn_memory() {
    revision_txn_contract(&MemoryStore::default()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn revision_txn_etcd() {
    let fixture = fixture::Fixture::start();
    revision_txn_contract(&fixture.store().await).await;
}
