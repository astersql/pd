// Copyright 2026 AsterSQL.
use super::*;
use crate::etcd_fixture as fixture;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{
    Base, Consistency, KeyspaceGroup, MetaStore, Read, SplitState, StorageEndpoint, UserKind,
};
use std::sync::Arc;
fn group(id: u32) -> KeyspaceGroup {
    KeyspaceGroup {
        id,
        user_kind: "standard".into(),
        ..Default::default()
    }
}

#[tokio::test]
async fn mcs_029_behavior() {
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let manager = KeyspaceGroupManager::new(store.clone(), paths.clone());
    manager.bootstrap().await.unwrap();
    let default = manager.get_group(0).await.unwrap();
    assert!(default.is_some(), "Bootstrap must persist default group");
    let default = default.unwrap();
    assert_eq!(default.user_kind, "basic");
    assert_eq!(default.keyspaces, Some(vec![0]));
    let mut second = group(2);
    second.keyspaces = Some(vec![111, 222, 333]);
    manager
        .create_groups(&[group(1), second, group(3)])
        .await
        .unwrap();
    assert_eq!(
        manager
            .get_groups(0, 0)
            .await
            .unwrap()
            .iter()
            .map(|g| g.id)
            .collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
    assert_eq!(
        manager
            .get_groups(1, 2)
            .await
            .unwrap()
            .iter()
            .map(|g| g.id)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        manager.indexed_group_ids(UserKind::STANDARD).await,
        Some(vec![1, 2, 3])
    );
    assert_eq!(
        manager.indexed_group_ids(UserKind::BASIC).await,
        Some(vec![0])
    );
    assert!(manager.indexed_group_ids(UserKind(9)).await.is_none());
    assert_eq!(manager.delete_group(3).await.unwrap().unwrap().id, 3);
    assert!(manager.get_group(3).await.unwrap().is_none());
    assert_eq!(
        manager.indexed_group_ids(UserKind::STANDARD).await,
        Some(vec![1, 2])
    );
    assert!(matches!(
        manager.create_groups(&[group(1)]).await,
        Err(GroupCatalogError::Exists)
    ));
    let revision = store
        .read(Read::prefix(
            paths.keyspace_group_id_prefix().as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .revision;
    assert!(matches!(
        manager.create_groups(&[group(4), group(1)]).await,
        Err(GroupCatalogError::Exists)
    ));
    assert!(manager.get_group(4).await.unwrap().is_none());
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
    assert!(matches!(
        manager.delete_group(0).await,
        Err(GroupCatalogError::Storage(_))
    ));
    assert!(manager.delete_group(999).await.unwrap().is_none());
}

#[tokio::test]
async fn mcs_029_restart_and_store_fault() {
    let mut fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let manager = KeyspaceGroupManager::new(store.clone(), paths.clone());
    manager.bootstrap().await.unwrap();
    manager.create_groups(&[group(1)]).await.unwrap();
    let endpoint = StorageEndpoint::new(store.clone());
    let mut txn = Base::new(store.as_ref()).begin_txn();
    let split = KeyspaceGroup {
        id: 2,
        user_kind: "enterprise".into(),
        split_state: Some(SplitState { split_source: 2 }),
        ..Default::default()
    };
    endpoint
        .save_keyspace_group(&mut txn, &paths, &split)
        .unwrap();
    txn.commit::<String>().await.unwrap();
    assert!(matches!(
        manager.delete_group(2).await,
        Err(GroupCatalogError::InSplit(2))
    ));
    fixture.command("stop", "stopped");
    assert!(manager.create_groups(&[group(3)]).await.is_err());
    assert!(manager.get_group(1).await.is_err());
    assert!(manager.delete_group(1).await.is_err());
    assert_eq!(
        manager.indexed_group_ids(UserKind::STANDARD).await,
        Some(vec![1])
    );
    fixture.command("start", "restarted");
    let restored = KeyspaceGroupManager::new(store.clone(), paths);
    restored.bootstrap().await.unwrap();
    assert_eq!(
        restored.indexed_group_ids(UserKind::STANDARD).await,
        Some(vec![1])
    );
    assert_eq!(
        restored.indexed_group_ids(UserKind::ENTERPRISE).await,
        Some(vec![2])
    );
    assert!(restored.get_group(3).await.unwrap().is_none());
}
