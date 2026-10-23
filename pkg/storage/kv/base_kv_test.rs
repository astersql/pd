// Copyright 2026 AsterSQL.
use crate::fixture;
use crate::*;

async fn base_kv_contract(store: &dyn MetaStore) {
    let base = Base::new(store);
    assert_eq!(base.load(b"key").await.unwrap(), b"");
    base.remove(b"key").await.unwrap();
    base.save(b"key/adjacent", b"neighbor").await.unwrap();
    // A missing exact key must not return its only prefix neighbor.
    assert_eq!(base.load(b"key").await.unwrap(), b"");
    base.save(b"key", b"value").await.unwrap();
    assert_eq!(base.load(b"key").await.unwrap(), b"value");
    base.save(b"key", b"overwritten").await.unwrap();
    assert_eq!(base.load(b"key").await.unwrap(), b"overwritten");
    base.save(b"key", b"").await.unwrap();
    assert_eq!(base.load(b"key").await.unwrap(), b"");
    let stored = store
        .read(Read::range(b"key", b"", Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(
        stored.entries.len(),
        1,
        "empty value remains an existing key"
    );
    assert!(stored.entries[0].value.is_empty());
    assert_eq!(stored.entries[0].lease, None);
    base.remove(b"key").await.unwrap();
    base.remove(b"key").await.unwrap();
    assert_eq!(base.load(b"key").await.unwrap(), b"");
    assert!(
        store
            .read(Read::range(b"key", b"", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    assert_eq!(base.load(b"key/adjacent").await.unwrap(), b"neighbor");
    // Byte keys and values do not undergo UTF-8 or other codec conversion.
    base.save(b"\xff\0", b"\0\xff").await.unwrap();
    assert_eq!(base.load(b"\xff\0").await.unwrap(), b"\0\xff");
    base.remove(b"\xff\0").await.unwrap();
    assert_eq!(base.load(b"\xff\0").await.unwrap(), b"");
    assert_eq!(
        base.load(b"").await.unwrap_err(),
        StoreError::InvalidArgument
    );
    assert_eq!(
        base.save(b"", b"bad").await.unwrap_err(),
        StoreError::InvalidArgument
    );
    assert_eq!(
        base.remove(b"").await.unwrap_err(),
        StoreError::InvalidArgument
    );
    assert_eq!(base.load(b"key/adjacent").await.unwrap(), b"neighbor");
}

#[tokio::test]
async fn base_kv_memory() {
    base_kv_contract(&MemoryStore::default()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn base_kv_etcd() {
    let fixture = fixture::Fixture::start();
    base_kv_contract(&fixture.store().await).await;
}
