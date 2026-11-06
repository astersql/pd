// Copyright 2026 AsterSQL.
use crate::*;
use tokio_util::sync::CancellationToken;
async fn contract(store: &dyn MetaStore) {
    let seed = store
        .transact(
            vec![],
            vec![
                Mutation::put(b"range/a", b"one"),
                Mutation::put(b"range/ab", b"neighbor"),
                Mutation::put(b"range/b", b"last"),
            ],
        )
        .await
        .unwrap();
    let mut read = Read::range(b"range/", b"range0", Consistency::Linearizable);
    read.limit = 1;
    let (page, count) = store.read_counted(read).await.unwrap();
    assert_eq!(count, 3);
    assert!(page.more);
    assert_eq!(page.entries.len(), 1);
    assert_eq!(page.entries[0].version, 1);
    let cancel = CancellationToken::new();
    let mut point = store
        .watch_range(b"range/a".to_vec(), vec![], seed.revision, true)
        .await
        .unwrap();
    let first = point.next(&cancel).await.unwrap();
    assert_eq!(first.events.len(), 1);
    assert_eq!(first.events[0].key(), b"range/a");
    let next = store
        .transact(vec![], vec![Mutation::put(b"range/a", b"two")])
        .await
        .unwrap();
    let updated = point.next(&cancel).await.unwrap();
    let Event::Put(entry) = &updated.events[0] else {
        panic!("expected put")
    };
    assert_eq!(entry.version, 2);
    assert_eq!(entry.create_revision, seed.revision);
    assert_eq!(entry.mod_revision, next.revision);
    assert_eq!(point.previous_value(&updated.events[0]).unwrap(), b"one");
    let outside = store
        .transact(vec![], vec![Mutation::put(b"outside", b"newer-header")])
        .await
        .unwrap();
    let mut replay = store
        .watch_range(b"range/a".to_vec(), vec![], seed.revision, false)
        .await
        .unwrap();
    let batch = replay.next(&cancel).await.unwrap();
    assert_eq!(batch.revision, next.revision);
    assert_eq!(replay.header_revision(), Some(outside.revision));
    let mut bounded = store
        .watch_range(
            b"range/a".to_vec(),
            b"range/b".to_vec(),
            seed.revision,
            false,
        )
        .await
        .unwrap();
    let first = bounded.next(&cancel).await.unwrap();
    assert!(first.events.iter().all(|e| e.key() < b"range/b".as_slice()));
    assert!(first.events.iter().any(|e| e.key() == b"range/ab"));
    let mut from = store
        .watch_range(b"range/b".to_vec(), vec![0], seed.revision, false)
        .await
        .unwrap();
    let first = from.next(&cancel).await.unwrap();
    assert_eq!(first.events.len(), 1);
    assert_eq!(first.events[0].key(), b"range/b");
    cancel.cancel();
    assert_eq!(
        point.next(&cancel).await.unwrap_err(),
        StoreError::Cancelled
    );
}
#[tokio::test]
async fn mcs_067_memory_counted_range_and_versions() {
    contract(&MemoryStore::default()).await;
}
#[tokio::test]
async fn mcs_067_etcd_counted_range_and_versions() {
    let fixture = crate::fixture::Fixture::start();
    let store = fixture.store().await;
    contract(&store).await;
}
