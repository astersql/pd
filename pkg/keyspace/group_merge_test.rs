// Copyright 2026 AsterSQL.
use super::*;
use crate::etcd_fixture as fixture;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{KeyspaceGroup, KeyspaceGroupMember, MetaStore};
use std::sync::Arc;
fn group(id: u32, keys: Vec<u32>) -> KeyspaceGroup {
    KeyspaceGroup {
        id,
        user_kind: "basic".into(),
        keyspaces: Some(keys),
        members: Some(vec![KeyspaceGroupMember::default(); 2]),
        ..Default::default()
    }
}
#[tokio::test]
async fn mcs_032_behavior() {
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let manager = KeyspaceGroupManager::new(store, ClusterPaths::new(42));
    manager.bootstrap().await.unwrap();
    manager
        .create_groups(&[group(1, vec![111, 222, 333]), group(3, vec![444, 555])])
        .await
        .unwrap();
    manager.split_group(1, 2, &[333], &[]).await.unwrap();
    manager.finish_split(2).await.unwrap();
    manager.merge_groups(1, &[2, 3]).await.unwrap();
    assert!(
        manager.get_group(2).await.unwrap().is_none(),
        "merge must delete its source groups"
    );
    assert!(manager.get_group(3).await.unwrap().is_none());
    let merged = manager.get_group(1).await.unwrap().unwrap();
    assert_eq!(merged.keyspaces, Some(vec![111, 222, 333, 444, 555]));
    assert!(merged.is_merge_target());
    assert_eq!(merged.merge_state.unwrap().merge_list, Some(vec![2, 3]));
    manager.finish_merge(1).await.unwrap();
    assert!(!manager.get_group(1).await.unwrap().unwrap().is_merging());
    assert!(manager.merge_groups(4, &[5]).await.is_err());
    assert!(manager.merge_groups(1, &vec![2; 60]).await.is_err());
    assert!(manager.merge_groups(1, &[0]).await.is_err());
}

#[tokio::test]
async fn mcs_032_default_target_at_capacity_boundary() {
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let manager = KeyspaceGroupManager::new(store, ClusterPaths::new(42));
    manager.bootstrap().await.unwrap();
    manager.merge_groups(999, &[]).await.unwrap();
    assert!(matches!(
        manager.merge_groups(0, &vec![0; 60]).await,
        Err(GroupMergeError::Capacity)
    ));
    let sources: Vec<_> = (1..60).collect();
    let groups: Vec<_> = sources.iter().map(|&id| group(id, vec![id, id])).collect();
    manager.create_groups(&groups).await.unwrap();
    manager.merge_groups(0, &sources).await.unwrap();
    let rows = manager.get_groups(0, 0).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].keyspaces, Some((0..60).collect()));
    assert_eq!(
        rows[0].merge_state.as_ref().unwrap().merge_list,
        Some(sources)
    );
    assert_eq!(
        manager
            .indexed_group_ids(astersql_pd_storage_domain::UserKind::BASIC)
            .await,
        Some(vec![0])
    );
    manager.finish_merge(0).await.unwrap();
    assert!(!manager.get_group(0).await.unwrap().unwrap().is_merging());
}

#[tokio::test]
async fn mcs_032_guards_and_reappearing_source() {
    use astersql_pd_storage_domain::{Base, MergeState, SplitState, StorageEndpoint};
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let manager = KeyspaceGroupManager::new(store.clone(), paths);
    manager
        .create_groups(&[
            group(1, vec![111]),
            group(2, vec![222]),
            group(3, vec![333]),
        ])
        .await
        .unwrap();
    assert!(matches!(
        manager.finish_merge(9).await,
        Err(GroupMergeError::NotExists(9))
    ));
    assert!(matches!(
        manager.finish_merge(3).await,
        Err(GroupMergeError::NotInMerging(3))
    ));
    let endpoint = StorageEndpoint::new(store.clone());
    let mut source = manager.get_group(2).await.unwrap().unwrap();
    source.split_state = Some(SplitState { split_source: 2 });
    let mut txn = Base::new(store.as_ref()).begin_txn();
    endpoint
        .save_keyspace_group(&mut txn, &paths, &source)
        .unwrap();
    txn.commit::<String>().await.unwrap();
    assert!(matches!(
        manager.merge_groups(1, &[2]).await,
        Err(GroupMergeError::Catalog(GroupCatalogError::InSplit(2)))
    ));
    source.split_state = None;
    source.merge_state = Some(MergeState {
        merge_list: Some(vec![9]),
    });
    let mut txn = Base::new(store.as_ref()).begin_txn();
    endpoint
        .save_keyspace_group(&mut txn, &paths, &source)
        .unwrap();
    txn.commit::<String>().await.unwrap();
    assert!(matches!(
        manager.merge_groups(1, &[2]).await,
        Err(GroupMergeError::InMerging(2))
    ));
    let mut target = manager.get_group(3).await.unwrap().unwrap();
    target.merge_state = Some(MergeState {
        merge_list: Some(vec![1]),
    });
    let mut txn = Base::new(store.as_ref()).begin_txn();
    endpoint
        .save_keyspace_group(&mut txn, &paths, &target)
        .unwrap();
    txn.commit::<String>().await.unwrap();
    for _ in 0..2 {
        assert!(matches!(
            manager.finish_merge(3).await,
            Err(GroupMergeError::NotInMerging(1))
        ));
    }
    assert_eq!(manager.get_group(3).await.unwrap().unwrap(), target);
    manager.delete_group(1).await.unwrap();
    manager.finish_merge(3).await.unwrap();
    assert!(matches!(
        manager.finish_merge(3).await,
        Err(GroupMergeError::NotInMerging(3))
    ));
}

#[tokio::test]
async fn mcs_032_real_commit_failure_and_finish_wal_recovery() {
    let mut fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let manager = KeyspaceGroupManager::new(store.clone(), paths);
    manager.bootstrap().await.unwrap();
    let mut target = group(1, vec![111, 222, 333]);
    for member in target.members.as_mut().unwrap() {
        member.address = "x".repeat(700_000);
    }
    let source = group(2, (0..50_000).collect());
    manager.create_groups(&[target.clone()]).await.unwrap();
    manager.create_groups(&[source.clone()]).await.unwrap();
    let error = manager.merge_groups(1, &[2]).await.unwrap_err();
    assert!(
        error.to_string().contains("request is too large"),
        "{error}"
    );
    assert_eq!(manager.get_group(1).await.unwrap().unwrap(), target);
    assert_eq!(manager.get_group(2).await.unwrap().unwrap(), source);
    {
        let index = manager.groups.read().await;
        assert_eq!(index[&0][&1], target);
        assert_eq!(index[&0][&2], source);
    }
    // The same source can merge into a small default target; finish then crosses a real outage.
    manager.merge_groups(0, &[2]).await.unwrap();
    assert!(manager.get_group(2).await.unwrap().is_none());
    fixture.command("stop", "stopped");
    assert!(manager.finish_merge(0).await.is_err());
    assert!(manager.groups.read().await[&0][&0].is_merge_target());
    fixture.command("start", "restarted");
    let restored = KeyspaceGroupManager::new(Arc::new(fixture.store().await), paths);
    restored.bootstrap().await.unwrap();
    assert!(
        restored
            .get_group(0)
            .await
            .unwrap()
            .unwrap()
            .is_merge_target()
    );
    assert!(restored.get_group(2).await.unwrap().is_none());
    assert_eq!(restored.get_group(1).await.unwrap().unwrap(), target);
    restored.finish_merge(0).await.unwrap();
    assert_eq!(
        restored.get_group(0).await.unwrap().unwrap().keyspaces,
        Some((0..50_000).collect())
    );
}

#[tokio::test]
async fn mcs_032_concurrent_merges_have_one_winner() {
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let manager = Arc::new(KeyspaceGroupManager::new(store, ClusterPaths::new(42)));
    manager
        .create_groups(&[
            group(1, vec![111]),
            group(2, vec![222]),
            group(3, vec![333]),
        ])
        .await
        .unwrap();
    let mut tasks = Vec::new();
    for source in [2, 3] {
        let m = manager.clone();
        tasks.push(tokio::spawn(
            async move { m.merge_groups(1, &[source]).await },
        ));
    }
    let mut winners = 0;
    for task in tasks {
        match task.await.unwrap() {
            Ok(()) => winners += 1,
            Err(GroupMergeError::InMerging(1)) => {}
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(winners, 1);
    let groups = manager.get_groups(0, 0).await.unwrap();
    assert_eq!(groups.len(), 2);
    let mut keys: Vec<_> = groups
        .iter()
        .flat_map(|g| g.keyspaces.iter().flatten().copied())
        .collect();
    keys.sort();
    assert_eq!(keys, vec![111, 222, 333]);
    manager.finish_merge(1).await.unwrap();
}
