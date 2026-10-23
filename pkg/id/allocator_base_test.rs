// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_member::{Member, MemberError};
use astersql_pd_storage_kv::*;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
// One fixture module per test process keeps its directory counter unique.
use super::allocate_count_test::fixture;

async fn boundary(store: &dyn MetaStore, key: &[u8]) -> u64 {
    let snapshot = store
        .read(Read::prefix(key, Consistency::Linearizable))
        .await
        .unwrap();
    let entry = snapshot
        .entries
        .iter()
        .find(|entry| entry.key == key)
        .unwrap();
    assert_eq!(entry.lease, None);
    u64::from_be_bytes(entry.value.as_slice().try_into().unwrap())
}

async fn behavior(store: Arc<dyn MetaStore>) -> u64 {
    let cancel = CancellationToken::new();
    let member = Member::bootstrap(store.clone(), cancel.clone(), 7200, 1, b"base".to_vec())
        .await
        .unwrap();
    let leader = Arc::new(member.campaign(30).await.unwrap());
    let allocator = Allocator::new(store.clone(), leader.clone(), cancel.clone(), 10).unwrap();
    let key = b"/pd/7200/alloc_id";
    assert_eq!(allocator.alloc().await.unwrap(), 1);
    allocator.set_base(100).await.unwrap();
    assert_eq!(allocator.alloc_count(0).await.unwrap(), (100, 0));
    assert_eq!(boundary(store.as_ref(), key).await, 110);
    assert_eq!(allocator.alloc().await.unwrap(), 101);

    // Go compatibility: lowering is allowed and can reuse an already returned ID.
    allocator.set_base(0).await.unwrap();
    assert_eq!(boundary(store.as_ref(), key).await, 10);
    assert_eq!(allocator.alloc().await.unwrap(), 1);
    allocator.set_base(1).await.unwrap(); // Equal base also reserves a new window.
    assert_eq!(boundary(store.as_ref(), key).await, 11);
    assert_eq!(allocator.alloc().await.unwrap(), 2);
    allocator.set_base(9).await.unwrap();
    assert_eq!(allocator.alloc_count(12).await.unwrap(), (21, 12));
    assert_eq!(boundary(store.as_ref(), key).await, 29);
    assert!(matches!(
        allocator.set_base(u64::MAX).await,
        Err(AllocatorError::Exhausted)
    ));
    assert_eq!(allocator.alloc_count(0).await.unwrap(), (21, 0));
    assert_eq!(boundary(store.as_ref(), key).await, 29);

    // SetBase does not read/compare the old allocator boundary, even if corrupt.
    store
        .transact(vec![], vec![Mutation::put(key, b"invalid")])
        .await
        .unwrap();
    allocator.set_base(200).await.unwrap();
    assert_eq!(boundary(store.as_ref(), key).await, 210);
    assert_eq!(allocator.alloc().await.unwrap(), 201);

    // Independent instances may overwrite each other's reservation, as in Go.
    let mut workers = Vec::new();
    for i in 0..8_u64 {
        let allocator = Allocator::new(store.clone(), leader.clone(), cancel.clone(), 10).unwrap();
        workers.push(tokio::spawn(async move {
            let base = 1000 + i * 100;
            allocator.set_base(base).await.unwrap();
            assert_eq!(allocator.alloc_count(0).await.unwrap(), (base, 0));
        }));
    }
    for worker in workers {
        worker.await.unwrap();
    }
    let persisted = boundary(store.as_ref(), key).await;
    assert!((1010..=1710).step_by(100).any(|end| end == persisted));
    let rebuilt = Allocator::new(store.clone(), leader.clone(), cancel.clone(), 10).unwrap();
    assert_eq!(rebuilt.alloc().await.unwrap(), persisted + 1);
    assert_eq!(boundary(store.as_ref(), key).await, persisted + 10);

    // A losing leader cannot overwrite the durable boundary; its base is retained.
    store.revoke(leader.lease_id()).await.unwrap();
    assert!(matches!(
        allocator.set_base(400).await,
        Err(AllocatorError::Member(MemberError::NotLeader))
    ));
    assert_eq!(allocator.alloc_count(0).await.unwrap(), (201, 0));
    assert_eq!(boundary(store.as_ref(), key).await, persisted + 10);
    let replacement = Arc::new(member.campaign(30).await.unwrap());
    let fresh = Allocator::new(store.clone(), replacement.clone(), cancel.clone(), 10).unwrap();
    assert_eq!(fresh.alloc().await.unwrap(), persisted + 11);
    cancel.cancel();
    assert!(matches!(
        fresh.set_base(500).await,
        Err(AllocatorError::Cancelled)
    ));
    assert_eq!(fresh.alloc_count(0).await.unwrap(), (persisted + 11, 0));
    assert_eq!(boundary(store.as_ref(), key).await, persisted + 20);
    replacement.resign().await.unwrap();

    // Go uint64 addition wraps in SetBase; allocation must still stop at MAX.
    let edge_member = Member::bootstrap(
        store.clone(),
        CancellationToken::new(),
        7201,
        1,
        b"edge".to_vec(),
    )
    .await
    .unwrap();
    let edge_leader = Arc::new(edge_member.campaign(30).await.unwrap());
    let edge = Allocator::new(
        store.clone(),
        edge_leader.clone(),
        CancellationToken::new(),
        10,
    )
    .unwrap();
    edge.set_base(u64::MAX - 1).await.unwrap();
    assert_eq!(edge.alloc_count(0).await.unwrap(), (u64::MAX - 1, 0));
    assert_eq!(boundary(store.as_ref(), b"/pd/7201/alloc_id").await, 8);
    assert_eq!(edge.alloc().await.unwrap(), u64::MAX);
    assert!(matches!(edge.alloc().await, Err(AllocatorError::Exhausted)));
    edge.set_base(u64::MAX - 10).await.unwrap();
    assert_eq!(
        boundary(store.as_ref(), b"/pd/7201/alloc_id").await,
        u64::MAX
    );
    assert_eq!(edge.alloc_count(10).await.unwrap(), (u64::MAX, 10));
    assert!(matches!(edge.alloc().await, Err(AllocatorError::Exhausted)));
    edge_leader.resign().await.unwrap();
    persisted + 20
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn allocator_base_memory() {
    behavior(Arc::new(MemoryStore::default())).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn allocator_base_etcd() {
    let mut fixture = fixture::Fixture::start();
    let persisted = behavior(Arc::new(fixture.store().await)).await;
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let member = Member::bootstrap(
        store.clone(),
        CancellationToken::new(),
        7200,
        1,
        b"base".to_vec(),
    )
    .await
    .unwrap();
    let leader = Arc::new(member.campaign(30).await.unwrap());
    let allocator = Allocator::new(store, leader.clone(), CancellationToken::new(), 10).unwrap();
    assert_eq!(allocator.alloc().await.unwrap(), persisted + 1);
    leader.resign().await.unwrap();
}
