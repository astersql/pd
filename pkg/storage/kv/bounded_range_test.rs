// Copyright 2026 AsterSQL.
use crate::fixture;
use crate::*;

async fn bounded_range_contract(store: &dyn MetaStore) {
    let keys: &[&[u8]] = &[b"a", b"a\0", b"aa", b"b", b"ba", b"c", b"\xff", b"\xff\xff"];
    let commit = store
        .transact(
            vec![],
            keys.iter().map(|key| Mutation::put(key, key)).collect(),
        )
        .await
        .unwrap();
    let cases: &[(&[u8], &[u8], usize, &[&[u8]], bool)] = &[
        (b"a", b"c", 0, &keys[..5], false),
        (b"a", b"c", 2, &keys[..2], true),
        (b"a", b"c", 5, &keys[..5], false),
        (b"ab", b"c", 0, &keys[3..5], false),
        (b"b", b"b", 0, &[], false),
        (b"c", b"b", 0, &[], false),
        (b"d", b"e", 0, &[], false),
        (b"a", b"\0", 0, &keys[..3], false),
        (b"", b"\0", 0, keys, false),
        (b"\xff", b"\0", 0, &keys[6..], false),
        (b"a", b"a\0", 0, &keys[..1], false),
        (b"b", b"", 0, &keys[3..4], false),
        (b"bb", b"", 0, &[], false),
    ];
    for consistency in [Consistency::Linearizable, Consistency::Serializable] {
        for &(start, end, limit, expected, more) in cases {
            let mut request = Read::range(start, end, consistency);
            request.limit = limit;
            let snapshot = store.read(request).await.unwrap();
            let actual: Vec<_> = snapshot.entries.iter().map(|e| e.key.as_slice()).collect();
            assert_eq!(
                actual, expected,
                "start={start:?} end={end:?} limit={limit}"
            );
            assert_eq!(snapshot.more, more);
            assert_eq!(snapshot.revision, commit.revision);
            for entry in snapshot.entries {
                assert_eq!(entry.value, entry.key);
                assert_eq!(entry.mod_revision, commit.revision);
            }
        }
    }
    // A bounded read retains historical revision semantics from the unique Read.
    store
        .transact(vec![], vec![Mutation::put(b"b", b"new")])
        .await
        .unwrap();
    let mut request = Read::range(b"b", b"c", Consistency::Linearizable);
    request.revision = Some(commit.revision);
    let old = store.read(request).await.unwrap();
    assert_eq!(old.entries.len(), 2);
    assert_eq!(old.entries[0].value, b"b");
    assert_eq!(old.entries[1].key, b"ba");

    let mut page = Read::prefix(b"a", Consistency::Linearizable);
    page.start = Some(b"a\0".to_vec());
    page.limit = 1;
    let page = store.read(page).await.unwrap();
    assert_eq!(page.entries.len(), 1);
    assert_eq!(page.entries[0].key, b"a\0");
    assert!(page.more);

    let mut invalid_revision = Read::range(b"a", b"c", Consistency::Linearizable);
    invalid_revision.revision = Some(0);
    let mut invalid_limit = Read::range(b"a", b"c", Consistency::Linearizable);
    invalid_limit.limit = usize::MAX;
    let mut mixed = Read::range(b"a", b"c", Consistency::Linearizable);
    mixed.prefix = b"a".to_vec();
    for invalid in [
        Read::range(b"", b"c", Consistency::Linearizable),
        Read::range(b"", b"", Consistency::Linearizable),
        invalid_revision,
        invalid_limit,
        mixed,
    ] {
        assert_eq!(
            store.read(invalid).await.unwrap_err(),
            StoreError::InvalidArgument
        );
    }
    eprintln!("bounded range: 26 selections and historical read passed");
}

#[test]
fn bounded_range_request() {
    let prefix = Read::range(b"a", b"\0", Consistency::Linearizable);
    assert_eq!(prefix.prefix, b"a");
    assert!(prefix.end.is_none());
    let bounded = Read::range(b"a", b"a\0", Consistency::Serializable);
    assert!(bounded.prefix.is_empty());
    assert_eq!(bounded.start.as_deref(), Some(b"a".as_slice()));
    assert_eq!(bounded.end.as_deref(), Some(b"a\0".as_slice()));
    assert_eq!(bounded.consistency, Consistency::Serializable);
    assert_eq!(bounded.limit, 0);
    assert!(bounded.revision.is_none());
    bounded.validate().unwrap();
}

#[tokio::test]
async fn bounded_range_memory() {
    bounded_range_contract(&MemoryStore::default()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bounded_range_etcd() {
    let fixture = fixture::Fixture::start();
    bounded_range_contract(&fixture.store().await).await;
}
