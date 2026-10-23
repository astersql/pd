// Copyright 2026 AsterSQL.
use astersql_pd_id::*;
use astersql_pd_member::*;
use astersql_pd_storage_kv::*;
use std::{collections::BTreeSet, sync::Arc};
use tokio_util::sync::CancellationToken;

pub async fn behavior(store: Arc<dyn MetaStore>) {
    let cancel = CancellationToken::new();
    let member = Member::bootstrap(store.clone(), cancel.clone(), 42, 1, b"one".to_vec())
        .await
        .unwrap();
    let leader = Arc::new(member.campaign(5).await.unwrap());
    let a = Arc::new(Allocator::new(store.clone(), leader.clone(), cancel.clone(), 7).unwrap());
    let b = Arc::new(Allocator::new(store.clone(), leader.clone(), cancel.clone(), 7).unwrap());
    let mut workers = Vec::new();
    for i in 0..16 {
        let allocator = if i % 2 == 0 { a.clone() } else { b.clone() };
        workers.push(tokio::spawn(async move {
            let mut ids = Vec::new();
            for _ in 0..20 {
                ids.push(allocator.alloc().await.unwrap());
            }
            ids
        }));
    }
    let mut ids = BTreeSet::new();
    for worker in workers {
        for id in worker.await.unwrap() {
            assert!(ids.insert(id));
        }
    }
    assert_eq!(ids.len(), 320);
    let max = *ids.last().unwrap();
    let boundary = store
        .read(Read::prefix(b"/pd/42/alloc_id", Consistency::Linearizable))
        .await
        .unwrap()
        .entries[0]
        .value
        .clone();
    let boundary = u64::from_be_bytes(boundary.try_into().unwrap());
    let restarted = Allocator::new(store.clone(), leader.clone(), cancel.clone(), 7).unwrap();
    let next = restarted.alloc().await.unwrap();
    assert!(next > boundary && boundary >= max);
    eprintln!(
        "ID restart: unique={} max={max} persisted-boundary={boundary} next={next}",
        ids.len()
    );
    store.revoke(leader.lease_id()).await.unwrap();
    assert!(matches!(
        a.alloc().await,
        Err(AllocatorError::Member(MemberError::NotLeader))
    ));
    assert!(matches!(
        restarted.alloc().await,
        Err(AllocatorError::Member(MemberError::NotLeader))
    ));
    let new_leader = Arc::new(member.campaign(5).await.unwrap());
    let new_allocator =
        Allocator::new(store.clone(), new_leader.clone(), cancel.clone(), 7).unwrap();
    assert!(new_allocator.alloc().await.unwrap() > next);
    cancel.cancel();
    assert!(matches!(
        new_allocator.alloc().await,
        Err(AllocatorError::Cancelled)
    ));
    new_leader.resign().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn memory_concurrency_restart_fencing() {
    behavior(Arc::new(MemoryStore::default())).await;
}

pub async fn boundaries(store: Arc<dyn MetaStore>) {
    let cancel = CancellationToken::new();
    let member = Member::bootstrap(store.clone(), cancel.clone(), 44, 1, b"boundary".to_vec())
        .await
        .unwrap();
    let leader = Arc::new(member.campaign(5).await.unwrap());
    store
        .transact(vec![], vec![Mutation::put(b"/pd/44/alloc_id", b"invalid")])
        .await
        .unwrap();
    let allocator = Allocator::new(store.clone(), leader.clone(), cancel.clone(), 7).unwrap();
    assert!(matches!(
        allocator.alloc().await,
        Err(AllocatorError::CorruptBoundary)
    ));
    store
        .transact(
            vec![],
            vec![Mutation::put(
                b"/pd/44/alloc_id",
                &(u64::MAX - 1).to_be_bytes(),
            )],
        )
        .await
        .unwrap();
    assert_eq!(allocator.alloc().await.unwrap(), u64::MAX);
    assert!(matches!(
        allocator.alloc().await,
        Err(AllocatorError::Exhausted)
    ));
    let wrong: Arc<dyn MetaStore> = Arc::new(MemoryStore::default());
    assert!(matches!(
        Allocator::new(wrong, leader.clone(), cancel, 1),
        Err(AllocatorError::StoreMismatch)
    ));
    leader.resign().await.unwrap();
}
#[tokio::test]
async fn memory_corrupt_and_exhausted_boundary() {
    boundaries(Arc::new(MemoryStore::default())).await;
}
