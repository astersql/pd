// Copyright 2026 AsterSQL.
use super::PrimaryObserver;
use astersql_pd_storage_domain::{MemoryStore, MetaStore, Mutation};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
#[allow(dead_code)]
#[path = "../../storage/kv/etcd_fixture_test.rs"]
mod fixture;
fn decode(bytes: &[u8]) -> Option<String> {
    std::str::from_utf8(bytes)
        .ok()
        .filter(|value| value.starts_with("http://"))
        .map(str::to_owned)
}
async fn wait(observer: &PrimaryObserver, expected: Option<&str>) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while observer.current().as_deref() != expected {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
async fn put(store: &dyn MetaStore, key: &[u8], value: &[u8]) {
    store
        .transact(vec![], vec![Mutation::put(key, value)])
        .await
        .unwrap();
}
#[tokio::test]
async fn primary_observer_snapshot_ordered_updates_exact_key_and_delete() {
    let store = Arc::new(MemoryStore::default());
    let key = b"/primary";
    put(store.as_ref(), key, b"http://one").await;
    let cancel = CancellationToken::new();
    let observer = PrimaryObserver::start(
        &tokio::runtime::Handle::current(),
        &cancel,
        store.clone(),
        key.to_vec(),
        decode,
    );
    wait(&observer, Some("http://one")).await;
    put(store.as_ref(), b"/primary/other", b"http://wrong").await;
    put(store.as_ref(), key, b"invalid participant").await;
    put(store.as_ref(), key, b"http://two").await;
    wait(&observer, Some("http://two")).await;
    store
        .transact(vec![], vec![Mutation::Delete { key: key.to_vec() }])
        .await
        .unwrap();
    wait(&observer, None).await;
    put(store.as_ref(), key, b"http://three").await;
    wait(&observer, Some("http://three")).await;
    observer.shutdown().await;
    put(store.as_ref(), key, b"http://after-stop").await;
    assert_eq!(observer.current().as_deref(), Some("http://three"));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn primary_observer_keeps_route_through_backend_outage_and_recovers() {
    let mut fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let key = b"/primary";
    put(store.as_ref(), key, b"http://one").await;
    let cancel = CancellationToken::new();
    let observer = PrimaryObserver::start(
        &tokio::runtime::Handle::current(),
        &cancel,
        store.clone(),
        key.to_vec(),
        decode,
    );
    wait(&observer, Some("http://one")).await;
    {
        let _suspended = fixture.suspend();
        tokio::time::sleep(Duration::from_millis(250)).await;
        for _ in 0..100 {
            assert_eq!(observer.current().as_deref(), Some("http://one"));
        }
    }
    put(store.as_ref(), key, b"http://two").await;
    wait(&observer, Some("http://two")).await;
    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(2), observer.shutdown())
        .await
        .unwrap();
}
#[tokio::test]
async fn primary_observer_reloads_after_compacted_watch_and_reconciles_absence() {
    let store = Arc::new(MemoryStore::default());
    let key = b"/primary";
    put(store.as_ref(), key, b"http://one").await;
    let cancel = CancellationToken::new();
    let observer = PrimaryObserver::start(
        &tokio::runtime::Handle::current(),
        &cancel,
        store.clone(),
        key.to_vec(),
        decode,
    );
    wait(&observer, Some("http://one")).await;
    // All MemoryStore operations complete without yielding on this one-thread
    // executor. Compact past the observer cursor before it can consume delete.
    store
        .transact(vec![], vec![Mutation::Delete { key: key.to_vec() }])
        .await
        .unwrap();
    let commit = store
        .transact(vec![], vec![Mutation::put(b"/unrelated", b"advance")])
        .await
        .unwrap();
    store.compact(commit.revision).await.unwrap();
    assert_eq!(observer.current().as_deref(), Some("http://one"));
    wait(&observer, None).await;
    put(store.as_ref(), key, b"http://new-primary").await;
    wait(&observer, Some("http://new-primary")).await;
    observer.shutdown().await;
}
