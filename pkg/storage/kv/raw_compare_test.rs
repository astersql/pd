// Copyright 2026 AsterSQL.
use crate::{fixture, *};

fn condition(kind: RawTxnCmpType, value: &[u8]) -> Compare {
    RawTxnCondition {
        key: b"source".to_vec(),
        cmp_type: kind,
        value: value.to_vec(),
    }
    .into()
}

async fn snapshot(store: &dyn MetaStore) -> Snapshot {
    store
        .read(Read::prefix(b"", Consistency::Linearizable))
        .await
        .unwrap()
}

async fn raw_compare_contract(store: &dyn MetaStore) {
    use RawTxnCmpType::*;
    let base = Base::new(store);
    // Missing, present-empty, ASCII and non-UTF8 byte values use the same matrix.
    for actual in [None, Some(b"".as_slice()), Some(b"v1"), Some(&[0x80, 0])] {
        base.remove(b"source").await.unwrap();
        if let Some(value) = actual {
            base.save(b"source", value).await.unwrap();
        }
        for operand in [
            b"".as_slice(),
            b"v0",
            b"v1",
            b"v2",
            &[0x7f, 255],
            &[0x80, 0],
            &[255],
        ] {
            for kind in [Equal, NotEqual, Less, Greater, Exists, NotExists] {
                let expected = match kind {
                    Equal => actual.is_some_and(|v| v == operand),
                    NotEqual => actual.is_some_and(|v| v != operand),
                    Less => actual.is_some_and(|v| v < operand),
                    Greater => actual.is_some_and(|v| v > operand),
                    Exists => actual.is_some(),
                    NotExists => actual.is_none(),
                };
                base.remove(b"output").await.unwrap();
                base.save(b"keep", b"original").await.unwrap();
                let before = snapshot(store).await;
                let result = store
                    .transact(
                        vec![condition(kind, operand)],
                        vec![
                            Mutation::put(b"output", b"written"),
                            Mutation::Delete {
                                key: b"keep".to_vec(),
                            },
                        ],
                    )
                    .await
                    .unwrap();
                assert_eq!(
                    result.succeeded, expected,
                    "actual={actual:?} operand={operand:?} kind={kind:?}"
                );
                let after = snapshot(store).await;
                if expected {
                    assert_eq!(after.revision, before.revision + 1);
                    assert_eq!(base.load(b"output").await.unwrap(), b"written");
                    assert_eq!(base.load(b"keep").await.unwrap(), b"");
                    let output = after.entries.iter().find(|e| e.key == b"output").unwrap();
                    assert_eq!(output.mod_revision, result.revision);
                } else {
                    assert_eq!(after.entries, before.entries);
                    assert_eq!(after.revision, before.revision);
                    assert_eq!(result.revision, before.revision);
                }
            }
        }
    }
    // Build conditions before an external mutation: commit sees the latest state.
    for (kind, initial, final_value, expected) in [
        (
            Equal,
            Some(b"old".as_slice()),
            Some(b"new".as_slice()),
            true,
        ),
        (NotEqual, Some(b"new"), Some(b"old"), true),
        (Less, Some(b"z"), Some(b"a"), true),
        (Greater, Some(b"a"), Some(b"z"), true),
        (Exists, None, Some(b""), true),
        (NotExists, Some(b""), None, true),
        (Equal, Some(b"new"), Some(b"old"), false),
    ] {
        base.remove(b"source").await.unwrap();
        if let Some(value) = initial {
            base.save(b"source", value).await.unwrap();
        }
        let compare = condition(kind, b"new");
        if let Some(value) = final_value {
            base.save(b"source", value).await.unwrap();
        } else {
            base.remove(b"source").await.unwrap();
        }
        let before = snapshot(store).await;
        let result = store
            .transact(
                vec![compare, Compare::CreateRevision(b"absent".to_vec(), 0)],
                vec![],
            )
            .await
            .unwrap();
        assert_eq!(result.succeeded, expected, "commit-time {kind:?}");
        assert_eq!(result.revision, before.revision);
    }
    base.save(b"source", b"new").await.unwrap();
    let result = store
        .transact(
            vec![condition(Equal, b"new"), condition(NotExists, b"ignored")],
            vec![Mutation::put(b"output", b"rejected")],
        )
        .await
        .unwrap();
    assert!(!result.succeeded, "all conditions must hold");
    assert!(store.transact(vec![], vec![]).await.unwrap().succeeded);
    let invalid: Compare = RawTxnCondition {
        key: vec![],
        cmp_type: Exists,
        value: vec![],
    }
    .into();
    assert!(matches!(
        store.transact(vec![invalid], vec![]).await,
        Err(StoreError::InvalidArgument)
    ));
}

#[tokio::test]
async fn raw_compare_memory() {
    raw_compare_contract(&MemoryStore::default()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn raw_compare_etcd() {
    let fixture = fixture::Fixture::start();
    raw_compare_contract(&fixture.store().await).await;
}
