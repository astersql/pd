// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_member::{Member, MemberError};
use astersql_pd_storage_kv::*;
use std::{collections::BTreeSet, sync::Arc};
use tokio_util::sync::CancellationToken;
#[path = "../storage/kv/etcd_fixture_test.rs"]
mod fixture;

async fn behavior(store: Arc<dyn MetaStore>) {
    let cancel = CancellationToken::new();
    let member = Member::bootstrap(store.clone(), cancel.clone(), 71, 1, b"count".to_vec())
        .await
        .unwrap();
    let leader = Arc::new(member.campaign(30).await.unwrap());
    let allocator =
        Arc::new(Allocator::new(store.clone(), leader.clone(), cancel.clone(), 3).unwrap());
    let key = b"/pd/71/alloc_id";
    assert_eq!(allocator.alloc_count(0).await.unwrap(), (0, 0));
    assert!(
        store
            .read(Read::prefix(key, Consistency::Linearizable))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    assert_eq!(allocator.alloc_count(5).await.unwrap(), (5, 5));
    assert_eq!(allocator.alloc_count(0).await.unwrap(), (5, 0));
    let boundary = store
        .read(Read::prefix(key, Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(
        u64::from_be_bytes(boundary.entries[0].value.as_slice().try_into().unwrap()),
        6
    );
    assert_eq!(allocator.alloc().await.unwrap(), 6);
    // A batch holds the same allocator lock across every window: no interleaving.
    let mut workers = Vec::new();
    for _ in 0..16 {
        let allocator = allocator.clone();
        workers.push(tokio::spawn(async move {
            let mut ranges = Vec::new();
            for _ in 0..10 {
                let (last, count) = allocator.alloc_count(5).await.unwrap();
                assert_eq!(count, 5);
                ranges.push(last - 4..=last);
            }
            ranges
        }));
    }
    let mut ids = BTreeSet::new();
    for worker in workers {
        for range in worker.await.unwrap() {
            for id in range {
                assert!(ids.insert(id), "overlapping batch ID {id}");
            }
        }
    }
    assert_eq!(ids, (7..=806).collect());
    // Exhaustion after partial consumption preserves the consumed local base.
    let edge = b"/pd/72/alloc_id";
    let edge_member = Member::bootstrap(store.clone(), cancel.clone(), 72, 1, b"edge".to_vec())
        .await
        .unwrap();
    let edge_leader = Arc::new(edge_member.campaign(30).await.unwrap());
    store
        .transact(
            vec![],
            vec![Mutation::put(edge, &(u64::MAX - 2).to_be_bytes())],
        )
        .await
        .unwrap();
    let edge_allocator =
        Allocator::new(store.clone(), edge_leader.clone(), cancel.clone(), 1).unwrap();
    assert!(matches!(
        edge_allocator.alloc_count(3).await,
        Err(AllocatorError::Exhausted)
    ));
    assert_eq!(edge_allocator.alloc_count(0).await.unwrap(), (u64::MAX, 0));
    assert!(matches!(
        edge_allocator.alloc_count(1).await,
        Err(AllocatorError::Exhausted)
    ));
    let boundary = store
        .read(Read::prefix(edge, Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(
        u64::from_be_bytes(boundary.entries[0].value.as_slice().try_into().unwrap()),
        u64::MAX
    );
    edge_leader.resign().await.unwrap();
    // Consume the last cached ID, then fail decoding on the following reservation.
    assert_eq!(allocator.alloc_count(2).await.unwrap(), (808, 2)); // end=810
    store
        .transact(vec![], vec![Mutation::put(key, b"invalid")])
        .await
        .unwrap();
    assert!(matches!(
        allocator.alloc_count(3).await,
        Err(AllocatorError::CorruptBoundary)
    ));
    assert_eq!(allocator.alloc_count(0).await.unwrap(), (810, 0));
    store
        .transact(vec![], vec![Mutation::put(key, &810_u64.to_be_bytes())])
        .await
        .unwrap();
    assert_eq!(allocator.alloc_count(1).await.unwrap(), (811, 1));
    // A revoked term cannot consume cached IDs. Zero count remains a local read.
    store.revoke(leader.lease_id()).await.unwrap();
    assert!(matches!(
        allocator.alloc_count(2).await,
        Err(AllocatorError::Member(MemberError::NotLeader))
    ));
    assert_eq!(allocator.alloc_count(0).await.unwrap(), (811, 0));
    let replacement = Arc::new(member.campaign(30).await.unwrap());
    let fresh = Allocator::new(store.clone(), replacement.clone(), cancel.clone(), 3).unwrap();
    assert_eq!(fresh.alloc_count(2).await.unwrap(), (815, 2));
    cancel.cancel();
    assert_eq!(fresh.alloc_count(0).await.unwrap(), (815, 0));
    assert!(matches!(
        fresh.alloc_count(1).await,
        Err(AllocatorError::Cancelled)
    ));
    replacement.resign().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn allocate_count_memory() {
    behavior(Arc::new(MemoryStore::default())).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn allocate_count_etcd() {
    let fixture = fixture::Fixture::start();
    behavior(Arc::new(fixture.store().await)).await;
}
