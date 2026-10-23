// Copyright 2026 AsterSQL.
use crate::{fixture, *};
use tokio_util::sync::CancellationToken;

async fn snapshot(store: &dyn MetaStore) -> Snapshot {
    store
        .read(Read::prefix(b"", Consistency::Linearizable))
        .await
        .unwrap()
}
fn condition() -> RawTxnCondition {
    RawTxnCondition {
        key: b"gate".to_vec(),
        cmp_type: RawTxnCmpType::Exists,
        value: vec![],
    }
}
fn delete(key: &[u8]) -> Mutation {
    Mutation::Delete { key: key.to_vec() }
}

async fn raw_branches_contract(store: &dyn MetaStore) {
    let base = Base::new(store);
    for succeeds in [true, false] {
        base.save(b"gate", b"open").await.unwrap();
        base.save(b"victim", b"old").await.unwrap();
        base.remove(b"a").await.unwrap();
        base.remove(b"b").await.unwrap();
        base.remove(b"unselected").await.unwrap();
        let txn = RawTxn::new(store).when(vec![condition()]);
        // Construct first, then mutate the condition key: selection is at commit.
        if !succeeds {
            base.remove(b"gate").await.unwrap();
        }
        let selected = vec![
            Mutation::put(b"a", b""),
            Mutation::put(b"b", &[0, 255]),
            delete(b"victim"),
            delete(b"missing"),
        ];
        let other = vec![Mutation::put(b"unselected", b"bad")];
        let txn = if succeeds {
            txn.and_then(selected).or_else(other)
        } else {
            txn.and_then(other).or_else(selected)
        };
        let before = snapshot(store).await;
        let result = txn.commit().await.unwrap();
        assert_eq!(result.succeeded, succeeds);
        let after = snapshot(store).await;
        assert_eq!(
            after.revision,
            before.revision + 1,
            "selected branch allocates one revision"
        );
        assert_eq!(result.revision, after.revision);
        for key in [b"a", b"b"] {
            assert_eq!(
                after
                    .entries
                    .iter()
                    .find(|e| e.key == key)
                    .unwrap()
                    .mod_revision,
                result.revision
            );
        }
        assert_eq!(base.load(b"b").await.unwrap(), [0, 255]);
        assert!(
            !after
                .entries
                .iter()
                .any(|e| e.key == b"victim" || e.key == b"unselected")
        );
    }
    for succeeds in [true, false] {
        if succeeds {
            base.save(b"gate", b"open").await.unwrap();
        } else {
            base.remove(b"gate").await.unwrap();
        }
        let before = snapshot(store).await;
        let result = RawTxn::new(store)
            .when(vec![condition()])
            .commit()
            .await
            .unwrap();
        assert_eq!(result.succeeded, succeeds);
        assert_eq!(result.revision, before.revision);
        // Request errors in either branch cannot partially commit the valid put.
        for invalid_then in [true, false] {
            let invalid = vec![
                Mutation::put(b"partial", b"bad"),
                Mutation::put(b"", b"invalid"),
            ];
            let valid = vec![Mutation::put(b"partial", b"bad")];
            let txn = RawTxn::new(store).when(vec![condition()]);
            let txn = if invalid_then {
                txn.and_then(invalid).or_else(valid)
            } else {
                txn.and_then(valid).or_else(invalid)
            };
            assert!(matches!(
                txn.commit().await,
                Err(StoreError::InvalidArgument)
            ));
            let after = snapshot(store).await;
            assert_eq!(after.entries, before.entries);
            assert_eq!(after.revision, before.revision);
        }
        // Backend lease failure is checked before any write or delete lands.
        let lease = store.grant(60).await.unwrap();
        store.revoke(lease.id).await.unwrap();
        let failed = vec![
            Mutation::put(b"partial", b"bad"),
            delete(b"b"),
            Mutation::Put {
                key: b"leased".to_vec(),
                value: vec![],
                lease: Some(lease.id),
            },
        ];
        let txn = RawTxn::new(store).when(vec![condition()]);
        let txn = if succeeds {
            txn.and_then(failed)
        } else {
            txn.or_else(failed)
        };
        assert!(matches!(txn.commit().await, Err(StoreError::LeaseNotFound)));
        let after = snapshot(store).await;
        assert_eq!(after.entries, before.entries);
        assert_eq!(after.revision, before.revision);
        let cancel = CancellationToken::new();
        cancel.cancel();
        let result = RawTxn::new(store)
            .when(vec![condition()])
            .and_then(vec![Mutation::put(b"partial", b"bad")])
            .or_else(vec![delete(b"b")])
            .commit_with_cancel(&cancel)
            .await;
        assert!(matches!(result, Err(StoreError::Cancelled)));
        let after = snapshot(store).await;
        assert_eq!(after.entries, before.entries);
        assert_eq!(after.revision, before.revision);
    }
    let before = snapshot(store).await;
    let result = RawTxn::new(store)
        .and_then(vec![Mutation::put(b"no-condition", b"yes")])
        .or_else(vec![delete(b"a")])
        .commit()
        .await
        .unwrap();
    assert!(result.succeeded);
    assert_eq!(result.revision, before.revision + 1);
    assert!(snapshot(store).await.entries.iter().any(|e| e.key == b"a"));
}
#[tokio::test]
async fn raw_branches_memory() {
    raw_branches_contract(&MemoryStore::default()).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn raw_branches_etcd() {
    let fixture = fixture::Fixture::start();
    raw_branches_contract(&fixture.store().await).await;
}
