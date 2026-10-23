// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_storage_kv::*;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

async fn entries(store: &dyn MetaStore, key: &[u8]) -> Vec<Entry> {
    store
        .read(Read::range(key, b"", Consistency::Linearizable))
        .await
        .unwrap()
        .entries
}
async fn behavior(store: Arc<dyn MetaStore>) {
    let key = b"/delete/leader";
    let campaign = || {
        Leadership::campaign_with_key(
            store.clone(),
            CancellationToken::new(),
            42,
            key.to_vec(),
            b"same".to_vec(),
            30,
            vec![],
        )
    };
    let old = campaign().await.unwrap();
    store
        .transact(vec![], vec![Mutation::Delete { key: key.to_vec() }])
        .await
        .unwrap();
    let new = campaign().await.unwrap();
    assert_ne!(old.lease_id(), new.lease_id());
    assert!(new.revision() > old.revision());
    let saved = entries(store.as_ref(), key).await;
    // Same value, different lease: stale deletion must conflict, without local reset.
    assert!(matches!(
        old.delete_leader_key_by_revision(old.revision()).await,
        Err(ElectionError::Contended)
    ));
    assert!(!old.is_expired());
    assert!(store.ttl(old.lease_id()).await.is_ok());
    assert_eq!(entries(store.as_ref(), key).await, saved);
    new.validate().await.unwrap();

    // Same lease and value, but a newer mod revision still fences stale cleanup.
    store
        .transact(
            vec![],
            vec![Mutation::Put {
                key: key.to_vec(),
                value: b"same".to_vec(),
                lease: Some(new.lease_id()),
            }],
        )
        .await
        .unwrap();
    let rewritten = entries(store.as_ref(), key).await;
    assert!(rewritten[0].mod_revision > new.revision());
    assert!(matches!(
        new.delete_leader_key_by_revision(new.revision()).await,
        Err(ElectionError::Contended)
    ));
    assert!(!new.is_expired());
    assert_eq!(entries(store.as_ref(), key).await, rewritten);

    // Delete exactly the observed revision. Neighboring keys survive.
    store
        .transact(
            vec![],
            vec![Mutation::put(b"/delete/leader-neighbor", b"safe")],
        )
        .await
        .unwrap();
    new.delete_leader_key_by_revision(rewritten[0].mod_revision)
        .await
        .unwrap();
    assert!(entries(store.as_ref(), key).await.is_empty());
    assert!(new.is_expired());
    assert!(matches!(
        store.ttl(new.lease_id()).await,
        Err(StoreError::LeaseNotFound)
    ));
    assert_eq!(
        entries(store.as_ref(), b"/delete/leader-neighbor").await[0].value,
        b"safe"
    );
    // Already absent succeeds even with a nonmatching revision, and cleans up old lease.
    old.delete_leader_key_by_revision(old.revision())
        .await
        .unwrap();
    assert!(old.is_expired());
    assert!(matches!(
        store.ttl(old.lease_id()).await,
        Err(StoreError::LeaseNotFound)
    ));
    old.delete_leader_key_by_revision(0).await.unwrap();
    // A new campaign after idempotent cleanup is independent of the old term.
    let next = campaign().await.unwrap();
    assert!(matches!(
        old.delete_leader_key_by_revision(old.revision()).await,
        Err(ElectionError::Contended)
    ));
    next.validate().await.unwrap();
    // Cover the Go regression's changed-value replacement as well.
    store
        .transact(
            vec![],
            vec![Mutation::Put {
                key: key.to_vec(),
                value: b"changed".to_vec(),
                lease: Some(next.lease_id()),
            }],
        )
        .await
        .unwrap();
    let changed = entries(store.as_ref(), key).await;
    assert!(matches!(
        next.delete_leader_key_by_revision(next.revision()).await,
        Err(ElectionError::Contended)
    ));
    assert_eq!(entries(store.as_ref(), key).await, changed);
    assert!(!next.is_expired());
    next.delete_leader_key_by_revision(changed[0].mod_revision)
        .await
        .unwrap();
}
#[tokio::test]
async fn leader_delete_memory() {
    behavior(Arc::new(MemoryStore::default())).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn leader_delete_etcd() {
    let fixture = super::etcd_fixture::Fixture::start();
    behavior(Arc::new(fixture.store().await)).await;
}
