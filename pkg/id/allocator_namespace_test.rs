// Copyright 2026 AsterSQL.
use super::allocate_count_test::fixture;
use super::*;
use astersql_pd_member::{Member, MemberError};
use astersql_pd_storage_kv::*;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

// Frozen Go cluster_paths oracle, cluster=42 (cluster_paths_test.rs).
const GLOBAL: &[u8] = b"/pd/42/alloc_id";
const KEYSPACE: &[u8] = b"/pd/42/keyspaces/alloc_id";
// Existing TestUint64ToBytes runtime evidence: input 1000, hex 00000000000003e8.
const GO_1000: &[u8] = &[0, 0, 0, 0, 0, 0, 3, 232];

async fn value(store: &dyn MetaStore, key: &[u8]) -> Vec<u8> {
    let snapshot = store
        .read(Read::prefix(key, Consistency::Linearizable))
        .await
        .unwrap();
    let entry = snapshot.entries.iter().find(|e| e.key == key).unwrap();
    assert_eq!(entry.lease, None);
    entry.value.clone()
}

#[test]
fn allocator_namespace_values() {
    assert_eq!(AllocatorNamespace::Global.key(42), GLOBAL);
    assert_eq!(AllocatorNamespace::Keyspace.key(42), KEYSPACE);
    assert_eq!(AllocatorNamespace::KeyspaceNextGen.key(42), KEYSPACE);
    assert_eq!(AllocatorNamespace::Global.effective_end(), u64::MAX);
    assert_eq!(AllocatorNamespace::Keyspace.effective_end(), 16777215);
    assert_eq!(
        AllocatorNamespace::KeyspaceNextGen.effective_end(),
        16776190
    );
}

async fn behavior(store: Arc<dyn MetaStore>) -> (u64, u64) {
    let cancel = CancellationToken::new();
    let member = Member::bootstrap(store.clone(), cancel.clone(), 42, 1, b"namespace".to_vec())
        .await
        .unwrap();
    let leader = Arc::new(member.campaign(30).await.unwrap());
    let global = Allocator::new(store.clone(), leader.clone(), cancel.clone(), 0).unwrap();
    let keyspace = Allocator::with_namespace(
        store.clone(),
        leader.clone(),
        cancel.clone(),
        0,
        AllocatorNamespace::Keyspace,
    )
    .unwrap();
    assert_eq!(global.alloc().await.unwrap(), 1);
    assert_eq!(keyspace.alloc().await.unwrap(), 1); // Independent durable windows.
    assert_eq!(value(store.as_ref(), GLOBAL).await, GO_1000);
    assert_eq!(value(store.as_ref(), KEYSPACE).await, GO_1000);
    let rebuilt = Allocator::with_namespace(
        store.clone(),
        leader.clone(),
        cancel.clone(),
        3,
        AllocatorNamespace::Keyspace,
    )
    .unwrap();
    assert_eq!(rebuilt.alloc_count(5).await.unwrap(), (1005, 5));
    assert_eq!(global.alloc().await.unwrap(), 2);
    assert_eq!(value(store.as_ref(), GLOBAL).await, GO_1000);
    // Revocation occurs before attempting cached consumption and SetBase writes.
    store.revoke(leader.lease_id()).await.unwrap();
    for allocator in [&global, &keyspace, &rebuilt] {
        assert!(matches!(
            allocator.alloc().await,
            Err(AllocatorError::Member(MemberError::NotLeader))
        ));
        assert!(matches!(
            allocator.set_base(2000).await,
            Err(AllocatorError::Member(MemberError::NotLeader))
        ));
    }
    assert_eq!(value(store.as_ref(), GLOBAL).await, GO_1000);
    assert_eq!(
        u64::from_be_bytes(value(store.as_ref(), KEYSPACE).await.try_into().unwrap()),
        1006
    );
    let replacement = Arc::new(member.campaign(30).await.unwrap());
    // Both keyspace modes use the same key; explicit mode only changes the ceiling.
    let nextgen = Allocator::with_namespace(
        store.clone(),
        replacement.clone(),
        cancel.clone(),
        3,
        AllocatorNamespace::KeyspaceNextGen,
    )
    .unwrap();
    assert_eq!(nextgen.alloc().await.unwrap(), 1007);
    let fresh_global =
        Allocator::new(store.clone(), replacement.clone(), cancel.clone(), 3).unwrap();
    assert_eq!(fresh_global.alloc().await.unwrap(), 1001);
    replacement.resign().await.unwrap();

    for (index, namespace) in [
        AllocatorNamespace::Keyspace,
        AllocatorNamespace::KeyspaceNextGen,
    ]
    .into_iter()
    .enumerate()
    {
        let cluster = 7300 + index as u64;
        let member = Member::bootstrap(
            store.clone(),
            CancellationToken::new(),
            cluster,
            1,
            b"edge".to_vec(),
        )
        .await
        .unwrap();
        let leader = Arc::new(member.campaign(30).await.unwrap());
        let allocator = Allocator::with_namespace(
            store.clone(),
            leader.clone(),
            CancellationToken::new(),
            10,
            namespace,
        )
        .unwrap();
        let limit = namespace.effective_end();
        let key = namespace.key(cluster);
        // Ordinary reservations clamp the durable end without moving the base backwards.
        store
            .transact(
                vec![],
                vec![Mutation::put(&key, &(limit - 2).to_be_bytes())],
            )
            .await
            .unwrap();
        assert_eq!(allocator.alloc_count(2).await.unwrap(), (limit, 2));
        assert_eq!(
            u64::from_be_bytes(value(store.as_ref(), &key).await.try_into().unwrap()),
            limit
        );
        assert!(matches!(
            allocator.alloc().await,
            Err(AllocatorError::Exhausted)
        ));
        let rebuilt = Allocator::with_namespace(
            store.clone(),
            leader.clone(),
            CancellationToken::new(),
            10,
            namespace,
        )
        .unwrap();
        assert!(matches!(
            rebuilt.alloc().await,
            Err(AllocatorError::Exhausted)
        ));
        for invalid in [limit, limit + 1, u64::MAX] {
            assert!(matches!(
                allocator.set_base(invalid).await,
                Err(AllocatorError::Exhausted)
            ));
        }
        // Go SetBase clamps then subtracts step, including its possible ID reuse.
        allocator.set_base(limit - 1).await.unwrap();
        assert_eq!(allocator.alloc_count(0).await.unwrap(), (limit - 10, 0));
        assert_eq!(allocator.alloc_count(10).await.unwrap(), (limit, 10));
        assert!(matches!(
            allocator.alloc().await,
            Err(AllocatorError::Exhausted)
        ));
        // Lowering is allowed, just as for the global allocator.
        allocator.set_base(0).await.unwrap();
        assert_eq!(allocator.alloc().await.unwrap(), 1);
        leader.resign().await.unwrap();
    }
    (1003, 1009)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn allocator_namespace_memory() {
    behavior(Arc::new(MemoryStore::default())).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn allocator_namespace_etcd() {
    let mut fixture = fixture::Fixture::start();
    let (global_end, keyspace_end) = behavior(Arc::new(fixture.store().await)).await;
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let member = Member::bootstrap(
        store.clone(),
        CancellationToken::new(),
        42,
        1,
        b"namespace".to_vec(),
    )
    .await
    .unwrap();
    let leader = Arc::new(member.campaign(30).await.unwrap());
    let global =
        Allocator::new(store.clone(), leader.clone(), CancellationToken::new(), 3).unwrap();
    let keyspace = Allocator::with_namespace(
        store,
        leader.clone(),
        CancellationToken::new(),
        3,
        AllocatorNamespace::Keyspace,
    )
    .unwrap();
    assert_eq!(global.alloc().await.unwrap(), global_end + 1);
    assert_eq!(keyspace.alloc().await.unwrap(), keyspace_end + 1);
    leader.resign().await.unwrap();
}
