// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_storage_endpoint::{EndpointError, LoadState};
use std::sync::Arc;

use super::etcd_fixture as fixture;

async fn contract(store: Arc<dyn MetaStore>) {
    let mut storage = Storage::new(store.clone());
    let other = Storage::new(store.clone());
    let base = storage.base();
    let key = b"/facade/value";
    assert_eq!(
        storage.endpoint().load(key).await.unwrap(),
        LoadState::Missing
    );
    // Go TestGCStateJSONUtil's existing byte oracle: JSON integer 1 is b"1".
    storage.endpoint().save_json(key, &1).await.unwrap();
    assert_eq!(base.load(key).await.unwrap(), b"1");
    assert_eq!(Base::new(store.as_ref()).load(key).await.unwrap(), b"1");
    assert_eq!(other.endpoint().load_json::<u64>(key).await.unwrap(), 1);
    base.save(key, b"2").await.unwrap();
    assert_eq!(storage.endpoint().load_json::<u64>(key).await.unwrap(), 2);
    base.run_in_txn(async |txn| {
        assert_eq!(txn.load(key).await?, b"2");
        txn.save(key, b"3")
    })
    .await
    .unwrap();
    assert_eq!(storage.endpoint().load_json::<u64>(key).await.unwrap(), 3);
    let result = base
        .run_in_txn(async |txn| {
            txn.save(key, b"4").unwrap();
            Err::<(), _>("rollback")
        })
        .await;
    assert_eq!(result, Err(CallbackTxnError::Callback("rollback")));
    assert_eq!(storage.endpoint().load_json::<u64>(key).await.unwrap(), 3);
    base.save(key, b"{").await.unwrap();
    assert!(matches!(
        storage.endpoint().load_json::<u64>(key).await,
        Err(EndpointError::JsonUnmarshal(_))
    ));
    base.save(key, b"").await.unwrap();
    assert_eq!(
        storage.endpoint().load(key).await.unwrap(),
        LoadState::Empty
    );
    assert_eq!(storage.endpoint().load_json::<u64>(key).await.unwrap(), 0);
    base.remove(key).await.unwrap();
    assert_eq!(
        storage.endpoint().load(key).await.unwrap(),
        LoadState::Missing
    );
    assert!(matches!(
        storage.endpoint().save_json(b"", &1).await,
        Err(EndpointError::Store(_))
    ));
    assert!(base.save(b"", b"invalid").await.is_err());
    storage.endpoint_mut().save_json(key, &1).await.unwrap();
    assert_eq!(storage.base().load(key).await.unwrap(), b"1");
    drop(storage);
    assert_eq!(other.base().load(key).await.unwrap(), b"1");
}

#[tokio::test]
async fn facade_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}

#[tokio::test]
async fn facade_etcd() {
    let fixture = fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
}
