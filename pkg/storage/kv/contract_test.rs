// Copyright 2026 AsterSQL.
//! Shared observable contract, run against both backends without network mocks.
use astersql_pd_storage_kv::*;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub async fn read(store: &dyn MetaStore, prefix: &[u8]) -> Snapshot {
    store
        .read(Read::prefix(prefix, Consistency::Linearizable))
        .await
        .unwrap()
}
pub async fn put(store: &dyn MetaStore, key: &[u8], value: &[u8]) -> Commit {
    store
        .transact(vec![], vec![Mutation::put(key, value)])
        .await
        .unwrap()
}
pub async fn batch(watch: &mut dyn MetaWatch) -> WatchBatch {
    tokio::time::timeout(
        Duration::from_secs(10),
        watch.next(&CancellationToken::new()),
    )
    .await
    .expect("watch stalled")
    .unwrap()
}
pub async fn cas_atomic_revision(store: &dyn MetaStore) {
    let initial = read(store, b"/").await.revision;
    let first = store
        .transact(
            vec![Compare::CreateRevision(b"/cas/a".to_vec(), 0)],
            vec![
                Mutation::put(b"/cas/a", b"one"),
                Mutation::put(b"/cas/b", b"two"),
            ],
        )
        .await
        .unwrap();
    assert!(first.succeeded);
    assert_eq!(first.revision, initial + 1);
    let conflict = store
        .transact(
            vec![Compare::CreateRevision(b"/cas/a".to_vec(), 0)],
            vec![
                Mutation::put(b"/cas/a", b"bad"),
                Mutation::put(b"/cas/c", b"bad"),
            ],
        )
        .await
        .unwrap();
    assert!(!conflict.succeeded);
    assert_eq!(first.revision, conflict.revision);
    let snapshot = read(store, b"/cas/").await;
    assert_eq!(snapshot.entries.len(), 2);
    assert_eq!(snapshot.entries[0].value, b"one");
    assert!(
        snapshot
            .entries
            .iter()
            .all(|e| e.mod_revision == first.revision && e.create_revision == first.revision)
    );
    let update = store
        .transact(
            vec![
                Compare::ModRevision(b"/cas/a".to_vec(), first.revision),
                Compare::Value(b"/cas/a".to_vec(), b"one".to_vec()),
            ],
            vec![Mutation::put(b"/cas/a", b"new")],
        )
        .await
        .unwrap();
    assert!(update.succeeded);
    assert_eq!(update.revision, first.revision + 1);
    let e = &read(store, b"/cas/a").await.entries[0];
    assert_eq!(e.create_revision, first.revision);
    assert_eq!(e.mod_revision, update.revision);
    let noop = store
        .transact(
            vec![],
            vec![Mutation::Delete {
                key: b"/absent".to_vec(),
            }],
        )
        .await
        .unwrap();
    assert_eq!(noop.revision, update.revision);
}
pub async fn snapshot_scan(store: &dyn MetaStore) {
    let old = put(store, b"/scan/a", b"old").await.revision;
    put(store, b"/scan/a", b"new").await;
    put(store, b"/scan/b", b"second").await;
    put(store, b"/other", b"excluded").await;
    for consistency in [Consistency::Linearizable, Consistency::Serializable] {
        let mut request = Read::prefix(b"/scan/", consistency);
        request.limit = 1;
        let first = store.read(request.clone()).await.unwrap();
        assert!(first.more);
        assert_eq!(first.entries[0].key, b"/scan/a");
        request.start = Some(b"/scan/b".to_vec());
        request.revision = Some(first.revision);
        let second = store.read(request).await.unwrap();
        assert!(!second.more);
        assert_eq!(second.entries[0].key, b"/scan/b");
    }
    let mut historical = Read::prefix(b"/scan/", Consistency::Linearizable);
    historical.revision = Some(old);
    let snapshot = store.read(historical).await.unwrap();
    assert_eq!(snapshot.entries.len(), 1);
    assert_eq!(snapshot.entries[0].value, b"old");
    assert!(snapshot.revision >= old);
    let mut future = Read::prefix(b"/", Consistency::Linearizable);
    future.revision = Some(snapshot.revision + 100);
    assert_eq!(
        store.read(future).await.unwrap_err(),
        StoreError::FutureRevision
    );
    put(store, &[255, 255, 1], b"binary").await;
    assert_eq!(read(store, &[255, 255]).await.entries.len(), 1);
    assert!(read(store, b"").await.entries.len() >= 4);
    let mut all = Read::prefix(b"", Consistency::Linearizable);
    all.start = Some(Vec::new());
    assert!(store.read(all).await.unwrap().entries.len() >= 4);
}
pub async fn watch_replay(store: &dyn MetaStore) {
    let revision = read(store, b"/watch/").await.revision;
    let first = store
        .transact(
            vec![],
            vec![
                Mutation::put(b"/watch/a", b"a"),
                Mutation::put(b"/watch/b", b"b"),
            ],
        )
        .await
        .unwrap();
    let deleted = store
        .transact(
            vec![],
            vec![Mutation::Delete {
                key: b"/watch/a".to_vec(),
            }],
        )
        .await
        .unwrap();
    put(store, b"/unrelated", b"ahead-of-replay-header").await;
    let mut watch = store
        .watch(b"/watch/".to_vec(), revision + 1)
        .await
        .unwrap();
    let mut events = Vec::new();
    while events.len() < 3 {
        events.extend(batch(watch.as_mut()).await.events);
    }
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].revision(), first.revision);
    assert_eq!(events[1].revision(), first.revision);
    assert_eq!(
        events[2],
        Event::Delete {
            key: b"/watch/a".to_vec(),
            revision: deleted.revision
        }
    );
    drop(watch);
    let mut resumed = store
        .watch(b"/watch/".to_vec(), deleted.revision + 1)
        .await
        .unwrap();
    let next = put(store, b"/watch/c", b"c").await;
    let b = batch(resumed.as_mut()).await;
    assert_eq!(b.events.len(), 1);
    assert_eq!(b.revision, next.revision);
    assert_eq!(b.events[0].key(), b"/watch/c");
}
pub async fn compaction_recovery(store: &dyn MetaStore) {
    let old = put(store, b"/compact/a", b"old").await.revision;
    let compact = put(store, b"/compact/a", b"new").await.revision;
    store.compact(compact).await.unwrap();
    let mut r = Read::prefix(b"/compact/", Consistency::Linearizable);
    r.revision = Some(old);
    assert!(matches!(
        store.read(r).await,
        Err(StoreError::Compacted { .. })
    ));
    let mut stale = store.watch(b"/compact/".to_vec(), old).await.unwrap();
    let err = tokio::time::timeout(
        Duration::from_secs(5),
        stale.next(&CancellationToken::new()),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert!(matches!(err, StoreError::Compacted { compact_revision: Some(r), .. } if r == compact));
    let fresh = read(store, b"/compact/").await;
    assert_eq!(fresh.entries[0].value, b"new");
    let mut recovered = store
        .watch(b"/compact/".to_vec(), fresh.revision + 1)
        .await
        .unwrap();
    let committed = put(store, b"/compact/a", b"after").await;
    assert_eq!(batch(recovered.as_mut()).await.revision, committed.revision);
    eprintln!(
        "compaction recovery: old={old}, boundary={compact}, recovered={}",
        committed.revision
    );
}
pub async fn lease_expiry(store: &dyn MetaStore) {
    let lease = store.grant(1).await.unwrap();
    assert!(lease.ttl_seconds >= 1);
    let committed = store
        .transact(
            vec![],
            vec![Mutation::Put {
                key: b"/expiry/a".to_vec(),
                value: b"leader".to_vec(),
                lease: Some(lease.id),
            }],
        )
        .await
        .unwrap();
    assert_eq!(
        read(store, b"/expiry/").await.entries[0].lease,
        Some(lease.id)
    );
    let mut watch = store
        .watch(b"/expiry/".to_vec(), committed.revision + 1)
        .await
        .unwrap();
    let deleted = batch(watch.as_mut()).await;
    assert_eq!(deleted.events.len(), 1);
    assert!(matches!(&deleted.events[0], Event::Delete { key, .. } if key == b"/expiry/a"));
    assert!(read(store, b"/expiry/").await.entries.is_empty());
    assert_eq!(
        store.ttl(lease.id).await.unwrap_err(),
        StoreError::LeaseNotFound
    );
    assert_eq!(
        store.keep_alive(lease.id).await.unwrap_err(),
        StoreError::LeaseNotFound
    );
    let err = store
        .transact(
            vec![],
            vec![
                Mutation::put(b"/expiry/partial", b"bad"),
                Mutation::Put {
                    key: b"/expiry/a".to_vec(),
                    value: vec![],
                    lease: Some(lease.id),
                },
            ],
        )
        .await
        .unwrap_err();
    assert_eq!(err, StoreError::LeaseNotFound);
    assert!(read(store, b"/expiry/").await.entries.is_empty());
    eprintln!(
        "lease expiry: lease={}, delete_revision={}",
        lease.id, deleted.revision
    );
}
pub async fn lease_renew_revoke(store: &dyn MetaStore) {
    let lease = store.grant(2).await.unwrap();
    store
        .transact(
            vec![],
            vec![Mutation::Put {
                key: b"/renew/a".to_vec(),
                value: vec![],
                lease: Some(lease.id),
            }],
        )
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let renewed = store.keep_alive(lease.id).await.unwrap();
    assert_eq!(renewed.id, lease.id);
    assert!(renewed.ttl_seconds > 0);
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert_eq!(read(store, b"/renew/").await.entries.len(), 1);
    assert!(store.ttl(lease.id).await.unwrap() >= 0);
    store.revoke(lease.id).await.unwrap();
    assert!(read(store, b"/renew/").await.entries.is_empty());
    assert_eq!(
        store.ttl(lease.id).await.unwrap_err(),
        StoreError::LeaseNotFound
    );
}
pub async fn election_fencing(store: &dyn MetaStore) {
    let a = store.grant(10).await.unwrap();
    let b = store.grant(10).await.unwrap();
    let campaign = |id| Mutation::Put {
        key: b"/election/leader".to_vec(),
        value: b"same-name".to_vec(),
        lease: Some(id),
    };
    let (one, two) = tokio::join!(
        store.transact(
            vec![Compare::CreateRevision(b"/election/leader".to_vec(), 0)],
            vec![campaign(a.id)]
        ),
        store.transact(
            vec![Compare::CreateRevision(b"/election/leader".to_vec(), 0)],
            vec![campaign(b.id)]
        )
    );
    assert_ne!(
        one.as_ref().unwrap().succeeded,
        two.as_ref().unwrap().succeeded
    );
    let (winner, loser) = if one.unwrap().succeeded {
        (a, b)
    } else {
        (b, a)
    };
    let fence = |id| {
        vec![
            Compare::Value(b"/election/leader".to_vec(), b"same-name".to_vec()),
            Compare::Lease(b"/election/leader".to_vec(), id),
        ]
    };
    assert!(
        store
            .transact(
                fence(winner.id),
                vec![Mutation::put(b"/election/data", b"one")]
            )
            .await
            .unwrap()
            .succeeded
    );
    store.revoke(winner.id).await.unwrap();
    assert!(
        store
            .transact(
                vec![Compare::CreateRevision(b"/election/leader".to_vec(), 0)],
                vec![campaign(loser.id)]
            )
            .await
            .unwrap()
            .succeeded
    );
    assert!(
        !store
            .transact(
                fence(winner.id),
                vec![Mutation::put(b"/election/data", b"stale")]
            )
            .await
            .unwrap()
            .succeeded
    );
    assert!(
        store
            .transact(
                fence(loser.id),
                vec![Mutation::put(b"/election/data", b"two")]
            )
            .await
            .unwrap()
            .succeeded
    );
    assert_eq!(
        read(store, b"/election/data").await.entries[0].value,
        b"two"
    );
    store.revoke(loser.id).await.unwrap();
}
pub async fn invalid_atomic(store: &dyn MetaStore) {
    let rev = read(store, b"/").await.revision;
    assert_eq!(
        store
            .transact(
                vec![],
                vec![
                    Mutation::put(b"/invalid/a", b"a"),
                    Mutation::put(b"/invalid/a", b"b")
                ]
            )
            .await
            .unwrap_err(),
        StoreError::InvalidArgument
    );
    assert_eq!(
        store
            .transact(vec![], vec![Mutation::put(b"", b"bad")])
            .await
            .unwrap_err(),
        StoreError::InvalidArgument
    );
    assert_eq!(
        store.grant(0).await.unwrap_err(),
        StoreError::InvalidArgument
    );
    let mut r = Read::prefix(b"/invalid/", Consistency::Linearizable);
    r.start = Some(b"/outside".to_vec());
    assert_eq!(
        store.read(r).await.unwrap_err(),
        StoreError::InvalidArgument
    );
    assert_eq!(read(store, b"/").await.revision, rev);
}
pub async fn watch_cancel(store: &dyn MetaStore) {
    let revision = read(store, b"/").await.revision;
    let mut watch = store
        .watch(b"/cancel/".to_vec(), revision + 1)
        .await
        .unwrap();
    let cancel = CancellationToken::new();
    let (result, ()) = tokio::join!(watch.next(&cancel), async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel.cancel();
    });
    assert_eq!(result.unwrap_err(), StoreError::Cancelled);
    assert_eq!(
        watch.next(&CancellationToken::new()).await.unwrap_err(),
        StoreError::Cancelled
    );
    let mut pre = store
        .watch(b"/cancel/".to_vec(), revision + 1)
        .await
        .unwrap();
    assert_eq!(pre.next(&cancel).await.unwrap_err(), StoreError::Cancelled);
}
