// Copyright 2026 AsterSQL.
use super::*;
use crate::etcd_fixture as fixture;
use astersql_pd_balancer::RoundRobin;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{KeyspaceGroup, KeyspaceGroupMember, MetaStore};
use std::{collections::BTreeSet, sync::Arc};
use tokio_util::sync::CancellationToken;
#[tokio::test]
async fn mcs_033_behavior() {
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let nodes = Arc::new(RoundRobin::new());
    nodes.put("http://tso-1".into());
    nodes.put("http://tso-2".into());
    let manager =
        KeyspaceGroupManager::new(store, ClusterPaths::new(42)).with_nodes_balancer(nodes);
    manager
        .create_groups(&[KeyspaceGroup {
            id: 1,
            user_kind: "standard".into(),
            members: Some(vec![KeyspaceGroupMember {
                address: "http://tso-1".into(),
                priority: 9,
            }]),
            keyspaces: Some(vec![42]),
            ..Default::default()
        }])
        .await
        .unwrap();
    manager
        .alloc_nodes(
            1,
            Some(BTreeSet::from(["http://tso-1".into()])),
            2,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    let group = manager.get_group(1).await.unwrap().unwrap();
    assert_eq!(
        group.members.as_ref().unwrap().len(),
        2,
        "allocation must persist two distinct replicas"
    );
    manager.set_priority(1, "https://tso-1", 5).await.unwrap();
    assert_eq!(
        manager
            .get_group(1)
            .await
            .unwrap()
            .unwrap()
            .members
            .unwrap()
            .iter()
            .find(|m| m.address == "http://tso-1")
            .unwrap()
            .priority,
        5
    );
    manager
        .set_nodes(1, &["http://tso-2".into()])
        .await
        .unwrap();
    assert_eq!(
        manager.get_group(1).await.unwrap().unwrap().members,
        Some(vec![KeyspaceGroupMember {
            address: "http://tso-2".into(),
            priority: 0
        }])
    );
}

fn group(id: u32, members: Vec<KeyspaceGroupMember>) -> KeyspaceGroup {
    KeyspaceGroup {
        id,
        user_kind: "standard".into(),
        members: Some(members),
        keyspaces: Some(vec![42]),
        ..Default::default()
    }
}
fn member(address: &str, priority: i64) -> KeyspaceGroupMember {
    KeyspaceGroupMember {
        address: address.into(),
        priority,
    }
}
#[tokio::test]
async fn mcs_033_replays_duplicate_scheme_and_healthy_noop() {
    use astersql_pd_storage_domain::{Consistency, Read};
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let nodes = Arc::new(RoundRobin::new());
    nodes.put("http://tso-1".into());
    nodes.put("https://tso-1".into());
    let manager =
        KeyspaceGroupManager::new(store.clone(), paths).with_nodes_balancer(nodes.clone());
    let initial = group(1, vec![member("http://tso-1", 0)]);
    manager.create_groups(&[initial.clone()]).await.unwrap();
    assert_eq!(manager.get_nodes_count(), 1);
    assert!(matches!(
        manager
            .alloc_nodes(
                1,
                Some(BTreeSet::from(["http://tso-1".into()])),
                2,
                &CancellationToken::new()
            )
            .await,
        Err(GroupNodesError::NoAvailableNode)
    ));
    assert_eq!(manager.get_group(1).await.unwrap().unwrap(), initial);
    assert!(
        manager
            .alloc_nodes(1, None, 2, &CancellationToken::new())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(manager.get_group(1).await.unwrap().unwrap(), initial);
    nodes.put("http://tso-2".into());
    assert_eq!(manager.get_nodes_count(), 2);
    let allocated = manager
        .alloc_nodes(1, None, 2, &CancellationToken::new())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(allocated.len(), 2);
    assert_ne!(
        astersql_pd_typeutil::trim_scheme(&allocated[0].address),
        astersql_pd_typeutil::trim_scheme(&allocated[1].address)
    );
    let revision = store
        .read(Read::prefix(
            paths.keyspace_group_id_prefix().as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .revision;
    assert!(
        manager
            .alloc_nodes(1, None, 2, &CancellationToken::new())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store
            .read(Read::prefix(
                paths.keyspace_group_id_prefix().as_bytes(),
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .revision,
        revision
    );
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(matches!(
        manager.alloc_nodes(1, None, 2, &cancel).await,
        Err(GroupNodesError::Cancelled)
    ));
}
#[tokio::test]
async fn mcs_033_priority_equivalence_order_and_invalid_member() {
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let manager = KeyspaceGroupManager::new(store, ClusterPaths::new(42));
    manager.create_groups(&[group(1, vec![])]).await.unwrap();
    let nodes = vec![
        "http://tso-1".into(),
        "https://tso-1".into(),
        "http://tso-2".into(),
    ];
    manager.set_nodes(1, &nodes).await.unwrap();
    manager.set_priority(1, "tso-1", -7).await.unwrap();
    let expected = vec![
        member("http://tso-1", -7),
        member("https://tso-1", -7),
        member("http://tso-2", 0),
    ];
    assert_eq!(
        manager.get_group(1).await.unwrap().unwrap().members,
        Some(expected.clone())
    );
    assert!(matches!(
        manager.set_priority(1, "absent", 5).await,
        Err(GroupNodesError::NodeNotInGroup)
    ));
    assert_eq!(
        manager.get_group(1).await.unwrap().unwrap().members,
        Some(expected)
    );
    assert!(manager.registered_node("absent").is_none());
    manager.set_nodes(1, &[]).await.unwrap();
    assert_eq!(
        manager.get_group(1).await.unwrap().unwrap().members,
        Some(vec![])
    );
}
#[tokio::test]
async fn mcs_033_revalidates_storage_roles_and_refreshes_noop_cache() {
    use astersql_pd_storage_domain::{Base, MergeState, SplitState, StorageEndpoint};
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let nodes = Arc::new(RoundRobin::new());
    nodes.put("http://tso-1".into());
    nodes.put("http://tso-2".into());
    let manager = KeyspaceGroupManager::new(store.clone(), paths).with_nodes_balancer(nodes);
    manager
        .create_groups(&[group(1, vec![member("http://tso-1", 0)])])
        .await
        .unwrap();
    let endpoint = StorageEndpoint::new(store.clone());
    let mut latest = group(
        1,
        vec![member("http://tso-1", 5), member("http://tso-2", 6)],
    );
    let mut txn = astersql_pd_storage_domain::Base::new(store.as_ref()).begin_txn();
    endpoint
        .save_keyspace_group(&mut txn, &paths, &latest)
        .unwrap();
    txn.commit::<String>().await.unwrap();
    assert!(
        manager
            .alloc_nodes(1, None, 2, &CancellationToken::new())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(manager.groups.read().await[&1][&1], latest);
    latest.split_state = Some(SplitState { split_source: 1 });
    let mut txn = Base::new(store.as_ref()).begin_txn();
    endpoint
        .save_keyspace_group(&mut txn, &paths, &latest)
        .unwrap();
    txn.commit::<String>().await.unwrap();
    assert!(matches!(
        manager.set_nodes(1, &[]).await,
        Err(GroupNodesError::Catalog(GroupCatalogError::InSplit(1)))
    ));
    latest.split_state = None;
    latest.merge_state = Some(MergeState {
        merge_list: Some(vec![9]),
    });
    let mut txn = Base::new(store.as_ref()).begin_txn();
    endpoint
        .save_keyspace_group(&mut txn, &paths, &latest)
        .unwrap();
    txn.commit::<String>().await.unwrap();
    assert!(matches!(
        manager.set_priority(1, "tso-1", 0).await,
        Err(GroupNodesError::InMerging(1))
    ));
    assert!(matches!(
        manager
            .alloc_nodes(1, None, 2, &CancellationToken::new())
            .await,
        Err(GroupNodesError::InMerging(1))
    ));
    assert!(matches!(
        manager.set_nodes(99, &[]).await,
        Err(GroupNodesError::NotExists(99))
    ));
}
#[tokio::test]
async fn mcs_033_real_commit_failure_and_wal_recovery() {
    let mut fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let nodes = Arc::new(RoundRobin::new());
    nodes.put("http://tso-1".into());
    nodes.put("http://tso-2".into());
    let manager = KeyspaceGroupManager::new(store, paths).with_nodes_balancer(nodes.clone());
    let initial = group(1, vec![member("http://tso-1", 9)]);
    manager.create_groups(&[initial.clone()]).await.unwrap();
    let error = manager
        .set_nodes(1, &["x".repeat(1_700_000)])
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("request is too large"),
        "{error}"
    );
    assert_eq!(manager.get_group(1).await.unwrap().unwrap(), initial);
    assert_eq!(manager.groups.read().await[&1][&1], initial);
    fixture.command("stop", "stopped");
    assert!(
        manager
            .alloc_nodes(1, None, 2, &CancellationToken::new())
            .await
            .is_err()
    );
    assert!(manager.set_priority(1, "tso-1", 5).await.is_err());
    assert_eq!(manager.groups.read().await[&1][&1], initial);
    fixture.command("start", "restarted");
    let restored = KeyspaceGroupManager::new(Arc::new(fixture.store().await), paths)
        .with_nodes_balancer(nodes);
    restored.bootstrap().await.unwrap();
    assert_eq!(restored.get_group(1).await.unwrap().unwrap(), initial);
    restored
        .alloc_nodes(1, None, 2, &CancellationToken::new())
        .await
        .unwrap();
    restored.set_priority(1, "https://tso-1", 5).await.unwrap();
    assert_eq!(
        restored
            .get_group(1)
            .await
            .unwrap()
            .unwrap()
            .members
            .unwrap()
            .len(),
        2
    );
}
