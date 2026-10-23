// Copyright 2026 AsterSQL.
use crate::{fixture, *};

fn range(key: &[u8], end: &[u8], limit: usize) -> RawTxnOp {
    RawTxnOp::GetRange {
        key: key.to_vec(),
        end: end.to_vec(),
        limit,
    }
}
fn item(pairs: &[(&[u8], &[u8])]) -> RawTxnResponseItem {
    RawTxnResponseItem {
        key_value_pairs: pairs
            .iter()
            .map(|(key, value)| KeyValuePair {
                key: key.to_vec(),
                value: value.to_vec(),
            })
            .collect(),
    }
}
async fn raw_range_contract(store: &dyn MetaStore) {
    let base = Base::new(store);
    let pairs: &[(&[u8], &[u8])] = &[
        (b"a", b"old"),
        (b"a\0", b"nul"),
        (b"aa", b""),
        (b"b", b"bee"),
        (b"ba", b"tail"),
        (b"c", b"end"),
        (b"\xff", b"\0\xff"),
        (b"\xff\xff", b"last"),
    ];
    for &(key, value) in pairs.iter().rev() {
        base.save(key, value).await.unwrap();
    }
    let cases: &[(&[u8], &[u8], usize, &[(&[u8], &[u8])])] = &[
        (
            b"a",
            b"c",
            0,
            &[
                (b"a", b"at-commit"),
                (b"a\0", b"nul"),
                (b"aa", b""),
                (b"b", b"bee"),
                (b"ba", b"tail"),
            ],
        ),
        (b"a", b"c", 2, &[(b"a", b"at-commit"), (b"a\0", b"nul")]),
        (b"a", b"a\0", 0, &[(b"a", b"at-commit")]),
        (
            b"a",
            b"\0",
            0,
            &[(b"a", b"at-commit"), (b"a\0", b"nul"), (b"aa", b"")],
        ),
        (b"\xff", b"\0", 0, &pairs[6..]),
        (b"b", b"", 0, &pairs[3..4]),
        (b"b", b"b", 0, &[]),
        (b"c", b"b", 0, &[]),
        (b"d", b"e", 1, &[]),
    ];
    for succeeds in [true, false] {
        let mut ops: Vec<_> = cases.iter().map(|(k, e, l, _)| range(k, e, *l)).collect();
        ops.insert(1, Mutation::put(b"written", b"new").into());
        ops.push(RawTxnOp::Get(b"aa".to_vec()));
        let txn = RawTxn::new(store).when(vec![RawTxnCondition {
            key: b"gate".to_vec(),
            cmp_type: RawTxnCmpType::Exists,
            value: vec![],
        }]);
        let txn = if succeeds {
            txn.then_ops(ops).else_ops(vec![range(b"b", b"", 0)])
        } else {
            txn.then_ops(vec![range(b"b", b"", 0)]).else_ops(ops)
        };
        if succeeds {
            base.save(b"gate", b"yes").await.unwrap();
        } else {
            base.remove(b"gate").await.unwrap();
        }
        base.save(b"a", b"at-commit").await.unwrap();
        let before = store
            .read(Read::prefix(b"", Consistency::Linearizable))
            .await
            .unwrap();
        let response = txn.commit().await.unwrap();
        let mut expected: Vec<_> = cases.iter().map(|(_, _, _, p)| item(p)).collect();
        expected.insert(1, RawTxnResponseItem::default());
        expected.push(item(&[(b"aa", b"")]));
        assert_eq!(response.responses, expected, "branch={succeeds}");
        assert_eq!(response.succeeded, succeeds);
        assert_eq!(response.revision, before.revision + 1);
        assert_eq!(base.load(b"written").await.unwrap(), b"new");
    }
    let before = store
        .read(Read::prefix(b"", Consistency::Linearizable))
        .await
        .unwrap();
    let all = RawTxn::new(store)
        .then_ops(vec![range(b"", b"\0", 0)])
        .commit()
        .await
        .unwrap();
    let expected = RawTxnResponseItem {
        key_value_pairs: before
            .entries
            .iter()
            .map(|e| KeyValuePair {
                key: e.key.clone(),
                value: e.value.clone(),
            })
            .collect(),
    };
    assert_eq!(all.responses, vec![expected]);
    assert_eq!(all.revision, before.revision);
    for invalid in [range(b"", b"c", 0), range(b"a", b"c", usize::MAX)] {
        for selected in [true, false] {
            let bad = vec![Mutation::put(b"partial", b"bad").into(), invalid.clone()];
            let txn = if selected {
                RawTxn::new(store).then_ops(bad)
            } else {
                RawTxn::new(store).else_ops(bad)
            };
            assert_eq!(txn.commit().await.unwrap_err(), StoreError::InvalidArgument);
            let after = store
                .read(Read::prefix(b"", Consistency::Linearizable))
                .await
                .unwrap();
            assert_eq!(after.entries, before.entries);
            assert_eq!(after.revision, before.revision);
        }
    }
}
#[tokio::test]
async fn raw_range_memory() {
    raw_range_contract(&MemoryStore::default()).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn raw_range_etcd() {
    let fixture = fixture::Fixture::start();
    raw_range_contract(&fixture.store().await).await;
}
