// Copyright 2026 AsterSQL.
use crate::{fixture, *};

fn pair(key: &[u8], value: &[u8]) -> RawTxnResponseItem {
    RawTxnResponseItem {
        key_value_pairs: vec![KeyValuePair {
            key: key.to_vec(),
            value: value.to_vec(),
        }],
    }
}
async fn snapshot(store: &dyn MetaStore) -> Snapshot {
    store
        .read(Read::prefix(b"", Consistency::Linearizable))
        .await
        .unwrap()
}
async fn raw_get_contract(store: &dyn MetaStore) {
    let base = Base::new(store);
    base.save(b"a", b"old").await.unwrap();
    base.save(b"aa", b"prefix-neighbor").await.unwrap();
    base.save(b"empty", b"").await.unwrap();
    base.save(&[255, 0], &[0, 255]).await.unwrap();
    for succeeds in [true, false] {
        base.save(b"victim", b"delete-me").await.unwrap();
        let txn = RawTxn::new(store).when(vec![RawTxnCondition {
            key: b"gate".to_vec(),
            cmp_type: RawTxnCmpType::Exists,
            value: vec![],
        }]);
        // Change values after builder construction: gets must execute at commit.
        if succeeds {
            base.save(b"gate", b"yes").await.unwrap();
        } else {
            base.remove(b"gate").await.unwrap();
        }
        base.save(b"a", b"at-commit").await.unwrap();
        let ops = vec![
            RawTxnOp::Get(vec![255, 0]),
            Mutation::put(b"written", b"new").into(),
            RawTxnOp::Get(b"missing".to_vec()),
            Mutation::Delete {
                key: b"victim".to_vec(),
            }
            .into(),
            RawTxnOp::Get(b"empty".to_vec()),
            RawTxnOp::Get(b"a".to_vec()),
            RawTxnOp::Get(b"a".to_vec()),
        ];
        let unselected = vec![RawTxnOp::Get(b"aa".to_vec())];
        let txn = if succeeds {
            txn.then_ops(ops).else_ops(unselected)
        } else {
            txn.then_ops(unselected).else_ops(ops)
        };
        let before = snapshot(store).await;
        let result = txn.commit().await.unwrap();
        assert_eq!(result.succeeded, succeeds);
        assert_eq!(
            result.responses,
            vec![
                pair(&[255, 0], &[0, 255]),
                RawTxnResponseItem::default(),
                RawTxnResponseItem::default(),
                RawTxnResponseItem::default(),
                pair(b"empty", b""),
                pair(b"a", b"at-commit"),
                pair(b"a", b"at-commit"),
            ]
        );
        assert_eq!(result.revision, before.revision + 1);
        assert_eq!(base.load(b"written").await.unwrap(), b"new");
        assert_eq!(base.load(b"victim").await.unwrap(), b"");
    }
    let before = snapshot(store).await;
    let read_only = RawTxn::new(store)
        .then_ops(vec![RawTxnOp::Get(b"a".to_vec())])
        .commit()
        .await
        .unwrap();
    assert_eq!(read_only.responses, vec![pair(b"a", b"at-commit")]);
    assert_eq!(read_only.revision, before.revision);
    let empty = RawTxn::new(store).commit().await.unwrap();
    assert!(empty.responses.is_empty());
    assert_eq!(empty.revision, before.revision);
    // Invalid reads in either branch reject the entire transaction, even if unselected.
    for invalid_then in [true, false] {
        let invalid = vec![
            Mutation::put(b"partial", b"bad").into(),
            RawTxnOp::Get(vec![]),
        ];
        let valid = vec![Mutation::put(b"partial", b"bad").into()];
        let txn = RawTxn::new(store);
        let txn = if invalid_then {
            txn.then_ops(invalid).else_ops(valid)
        } else {
            txn.then_ops(valid).else_ops(invalid)
        };
        assert!(matches!(
            txn.commit().await,
            Err(StoreError::InvalidArgument)
        ));
        let after = snapshot(store).await;
        assert_eq!(after.entries, before.entries);
        assert_eq!(after.revision, before.revision);
    }
}
#[tokio::test]
async fn raw_get_memory() {
    raw_get_contract(&MemoryStore::default()).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn raw_get_etcd() {
    let fixture = fixture::Fixture::start();
    raw_get_contract(&fixture.store().await).await;
}
