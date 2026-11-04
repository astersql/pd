// Copyright 2026 AsterSQL.
use super::allocate_count_test::fixture;
use super::*;
use astersql_pd_member::{Member, MemberError};
use astersql_pd_storage_kv::*;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
async fn boundary(store: &dyn MetaStore, key: &[u8]) -> Option<u64> {
    store
        .read(Read::range(key, b"", Consistency::Linearizable))
        .await
        .unwrap()
        .entries
        .first()
        .map(|e| u64::from_be_bytes(e.value.as_slice().try_into().unwrap()))
}
#[tokio::test]
async fn reserves_without_consuming_and_rebase_abandons_unused_ids() {
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let cancel = CancellationToken::new();
    let member = Member::bootstrap(store.clone(), cancel.clone(), 7220, 1, b"rebase".to_vec())
        .await
        .unwrap();
    let term = Arc::new(member.campaign(30).await.unwrap());
    let a = Allocator::new(store.clone(), term.clone(), cancel.clone(), 10).unwrap();
    let key = b"/pd/7220/alloc_id";
    assert_eq!(boundary(store.as_ref(), key).await, None);
    a.rebase().await.unwrap();
    assert_eq!(a.alloc_count(0).await.unwrap(), (0, 0));
    assert_eq!(boundary(store.as_ref(), key).await, Some(10));
    assert_eq!(a.alloc().await.unwrap(), 1);
    a.rebase().await.unwrap();
    assert_eq!(a.alloc_count(0).await.unwrap(), (10, 0));
    assert_eq!(boundary(store.as_ref(), key).await, Some(20));
    assert_eq!(a.alloc().await.unwrap(), 11);
    let fresh = Allocator::new(store.clone(), term.clone(), cancel, 10).unwrap();
    fresh.rebase().await.unwrap();
    assert_eq!(fresh.alloc().await.unwrap(), 21);
    assert_eq!(boundary(store.as_ref(), key).await, Some(30));
    term.resign().await.unwrap();
}
#[tokio::test]
async fn failures_preserve_window_and_cancellation_joins_pending_io() {
    let mut fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let cancel = CancellationToken::new();
    let member = Member::bootstrap(
        store.clone(),
        CancellationToken::new(),
        7221,
        1,
        b"rebase".to_vec(),
    )
    .await
    .unwrap();
    let term = Arc::new(member.campaign(30).await.unwrap());
    let a = Allocator::new(store.clone(), term.clone(), cancel.clone(), 10).unwrap();
    let key = b"/pd/7221/alloc_id";
    a.rebase().await.unwrap();
    assert_eq!(a.alloc().await.unwrap(), 1);
    store
        .transact(vec![], vec![Mutation::put(key, b"broken")])
        .await
        .unwrap();
    assert!(matches!(
        a.rebase().await,
        Err(AllocatorError::CorruptBoundary)
    ));
    assert_eq!(a.alloc_count(0).await.unwrap(), (1, 0));
    store
        .transact(vec![], vec![Mutation::put(key, &10u64.to_be_bytes())])
        .await
        .unwrap();
    let paused = fixture.suspend();
    let mut pending = Box::pin(a.rebase());
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(30), pending.as_mut())
            .await
            .is_err()
    );
    cancel.cancel();
    assert!(matches!(
        tokio::time::timeout(std::time::Duration::from_millis(500), pending)
            .await
            .unwrap(),
        Err(AllocatorError::Cancelled)
    ));
    drop(paused);
    assert_eq!(boundary(store.as_ref(), key).await, Some(10));
    assert_eq!(a.alloc_count(0).await.unwrap(), (1, 0));
    let stale = Allocator::new(store.clone(), term.clone(), CancellationToken::new(), 10).unwrap();
    store.revoke(term.lease_id()).await.unwrap();
    assert!(matches!(
        stale.rebase().await,
        Err(AllocatorError::Member(MemberError::NotLeader))
    ));
    assert_eq!(boundary(store.as_ref(), key).await, Some(10));
}
#[tokio::test]
async fn explicit_rebase_retains_go_unsigned_wrap_and_namespace_cap() {
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let member = Member::bootstrap(
        store.clone(),
        CancellationToken::new(),
        7222,
        1,
        b"edge".to_vec(),
    )
    .await
    .unwrap();
    let term = Arc::new(member.campaign(30).await.unwrap());
    let a = Allocator::new(store.clone(), term.clone(), CancellationToken::new(), 10).unwrap();
    let key = b"/pd/7222/alloc_id";
    store
        .transact(
            vec![],
            vec![Mutation::put(key, &(u64::MAX - 1).to_be_bytes())],
        )
        .await
        .unwrap();
    a.rebase().await.unwrap();
    assert_eq!(boundary(store.as_ref(), key).await, Some(8));
    assert_eq!(a.alloc_count(0).await.unwrap(), (u64::MAX - 1, 0));
    assert_eq!(a.alloc().await.unwrap(), u64::MAX);
    assert!(matches!(a.alloc().await, Err(AllocatorError::Exhausted)));
    let scoped = Allocator::with_namespace(
        store.clone(),
        term.clone(),
        CancellationToken::new(),
        10,
        AllocatorNamespace::Keyspace,
    )
    .unwrap();
    let limit = AllocatorNamespace::Keyspace.effective_end();
    let scoped_key = AllocatorNamespace::Keyspace.key(7222);
    store
        .transact(
            vec![],
            vec![Mutation::put(&scoped_key, &(limit - 1).to_be_bytes())],
        )
        .await
        .unwrap();
    scoped.rebase().await.unwrap();
    assert_eq!(boundary(store.as_ref(), &scoped_key).await, Some(limit));
    assert_eq!(scoped.alloc_count(0).await.unwrap(), (limit - 10, 0));
    term.resign().await.unwrap();
}
