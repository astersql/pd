// Copyright 2026 AsterSQL.
use super::*;
use crate::etcd_fixture as fixture;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{KeyspaceGroup, KeyspaceGroupMember, MetaStore};
use std::sync::Arc;
fn group(id: u32, keys: Vec<u32>) -> KeyspaceGroup {
    KeyspaceGroup {
        id,
        user_kind: "standard".into(),
        keyspaces: Some(keys),
        members: Some(vec![KeyspaceGroupMember::default(); 2]),
        ..Default::default()
    }
}
#[tokio::test]
async fn mcs_031_behavior() {
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let manager = KeyspaceGroupManager::new(store, ClusterPaths::new(42));
    manager.bootstrap().await.unwrap();
    manager
        .create_groups(&[group(2, vec![111, 222, 333])])
        .await
        .unwrap();
    manager.split_group(2, 4, &[333], &[]).await.unwrap();
    let target = manager.get_group(4).await.unwrap();
    assert!(target.is_some(), "split must persist target group");
    let target = target.unwrap();
    let source = manager.get_group(2).await.unwrap().unwrap();
    assert_eq!(source.keyspaces, Some(vec![111, 222]));
    assert_eq!(target.keyspaces, Some(vec![333]));
    assert!(source.is_split_source());
    assert!(target.is_split_target());
    assert_eq!(source.members, target.members);
    assert_eq!(source.user_kind, target.user_kind);
    assert!(manager.finish_split(2).await.is_err());
    assert!(manager.finish_split(5).await.is_err());
    assert!(manager.split_group(2, 5, &[111], &[]).await.is_err());
    assert!(manager.delete_group(2).await.is_err());
    manager.finish_split(4).await.unwrap();
    assert!(!manager.get_group(2).await.unwrap().unwrap().is_splitting());
    assert!(!manager.get_group(4).await.unwrap().unwrap().is_splitting());
}

#[test]
fn mcs_031_replays_go_partition_fixtures() {
    type Case = (Vec<u32>, Vec<u32>, u32, u32, Option<(Vec<u32>, Vec<u32>)>);
    let cases: Vec<Case> = vec![
        (
            vec![1, 2, 3, 4, 5],
            vec![1, 2, 3, 4, 5],
            0,
            0,
            Some((vec![], vec![1, 2, 3, 4, 5])),
        ),
        (
            vec![1, 2, 3, 4, 5],
            vec![1],
            0,
            0,
            Some((vec![2, 3, 4, 5], vec![1])),
        ),
        (vec![1, 2, 3, 4, 5], vec![6], 0, 0, None),
        (vec![1, 2], vec![2, 2], 0, 0, Some((vec![1], vec![2]))),
        (
            vec![0, 1, 2, 3, 4, 5],
            vec![],
            2,
            4,
            Some((vec![0, 1, 5], vec![2, 3, 4])),
        ),
        (
            vec![0, 1, 2, 3, 4, 5],
            vec![],
            0,
            4,
            Some((vec![0, 5], vec![1, 2, 3, 4])),
        ),
        (
            vec![1, 2, 3, 4, 5],
            vec![],
            2,
            4,
            Some((vec![1, 5], vec![2, 3, 4])),
        ),
        (
            vec![1, 2, 3, 4, 5],
            vec![],
            5,
            6,
            Some((vec![1, 2, 3, 4], vec![5])),
        ),
        (
            vec![1, 2, 3, 4, 5],
            vec![],
            2,
            6,
            Some((vec![1], vec![2, 3, 4, 5])),
        ),
        (
            vec![1, 2, 3, 4, 5],
            vec![],
            1,
            1,
            Some((vec![2, 3, 4, 5], vec![1])),
        ),
        (
            vec![1, 2, 3, 4, 5],
            vec![],
            0,
            6,
            Some((vec![], vec![1, 2, 3, 4, 5])),
        ),
        (vec![1, 2, 3, 4, 5], vec![], 7, 10, None),
        (vec![1, 2, 3, 4, 5], vec![], 0, 0, None),
    ];
    for (index, (old, new, start, end, expected)) in cases.into_iter().enumerate() {
        let result = build_split_keyspaces(&old, &new, start, end, Kernel::Classic);
        if let Some(expected) = expected {
            assert_eq!(result.unwrap(), expected, "Go fixture {index}");
        } else {
            assert!(result.is_err(), "Go fixture {index}");
        }
    }
    assert_eq!(
        build_split_keyspaces(&[0, 1, 2], &[], 0, 2, Kernel::NextGeneration).unwrap(),
        (vec![], vec![0, 1, 2])
    );
    assert!(
        build_split_keyspaces(&[0, 1], &[0], 0, 0, Kernel::Classic)
            .unwrap_err()
            .to_string()
            .contains("ErrModifyDefaultKeyspace")
    );
    assert!(
        build_split_keyspaces(
            &[0x00ff_fffe, 1],
            &[0x00ff_fffe],
            0,
            0,
            Kernel::NextGeneration
        )
        .unwrap_err()
        .to_string()
        .contains("ErrModifyReservedKeyspace")
    );
    // Explicit unique list order takes precedence over a conflicting range.
    assert_eq!(
        build_split_keyspaces(&[1, 2, 3], &[3, 1], 2, 2, Kernel::Classic).unwrap(),
        (vec![2], vec![3, 1])
    );
}

#[tokio::test]
async fn mcs_031_range_and_validation() {
    let mut fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let manager = KeyspaceGroupManager::new(store.clone(), paths);
    manager.bootstrap().await.unwrap();
    let mut small = group(1, vec![444]);
    small.members = None;
    manager
        .create_groups(&[small, group(2, vec![111, 333, 444, 555, 666])])
        .await
        .unwrap();
    assert!(
        manager
            .split_group(0, 4, &[0], &[])
            .await
            .unwrap_err()
            .to_string()
            .contains("ErrModifyDefaultKeyspace")
    );
    assert!(matches!(
        manager.split_group(1, 4, &[444], &[]).await,
        Err(GroupSplitError::NotEnoughReplicas)
    ));
    assert!(matches!(
        manager.split_group(2, 4, &[], &[]).await,
        Err(GroupSplitError::NotInGroup)
    ));
    assert!(matches!(
        manager.split_group(9, 4, &[], &[]).await,
        Err(GroupSplitError::NotExists(9))
    ));
    manager.split_group(2, 4, &[], &[222, 555]).await.unwrap();
    assert_eq!(
        manager.get_group(2).await.unwrap().unwrap().keyspaces,
        Some(vec![111, 666])
    );
    assert_eq!(
        manager.get_group(4).await.unwrap().unwrap().keyspaces,
        Some(vec![333, 444, 555])
    );
    fixture.command("stop", "stopped");
    assert!(manager.finish_split(4).await.is_err());
    assert!(manager.groups.read().await[&1][&4].is_split_target());
    fixture.command("start", "restarted");
    let restored = KeyspaceGroupManager::new(store, paths);
    restored.bootstrap().await.unwrap();
    assert!(
        restored
            .get_group(4)
            .await
            .unwrap()
            .unwrap()
            .is_split_target()
    );
    restored.finish_split(4).await.unwrap();
    assert!(!restored.get_group(2).await.unwrap().unwrap().is_splitting());
    assert!(!restored.get_group(4).await.unwrap().unwrap().is_splitting());
    assert!(matches!(
        restored.split_group(2, 4, &[111], &[]).await,
        Err(GroupSplitError::Catalog(GroupCatalogError::Exists))
    ));
    assert!(matches!(
        restored.split_group(2, 5, &[111, 222, 444], &[]).await,
        Err(GroupSplitError::NotInGroup)
    ));
}

#[tokio::test]
async fn mcs_031_real_split_and_finish_commit_failures() {
    use astersql_pd_storage_domain::{Base, SplitState, StorageEndpoint};
    let mut fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let manager = KeyspaceGroupManager::new(store.clone(), paths);
    let mut source = group(2, vec![111, 222, 333]);
    for member in source.members.as_mut().unwrap() {
        member.address = "x".repeat(425_000);
    }
    manager.create_groups(&[source.clone()]).await.unwrap();
    let error = manager.split_group(2, 4, &[333], &[]).await.unwrap_err();
    assert!(
        error.to_string().contains("request is too large"),
        "{error}"
    );
    assert_eq!(manager.get_group(2).await.unwrap().unwrap(), source);
    assert!(manager.get_group(4).await.unwrap().is_none());
    assert_eq!(manager.groups.read().await[&1][&2], source);
    // Admit an existing large split pair with separate legitimate writes.
    source.split_state = Some(SplitState { split_source: 2 });
    source.keyspaces = Some(vec![111, 222]);
    let mut target = source.clone();
    target.id = 4;
    target.keyspaces = Some(vec![333]);
    let endpoint = StorageEndpoint::new(store.clone());
    for group in [&source, &target] {
        let mut txn = Base::new(store.as_ref()).begin_txn();
        endpoint
            .save_keyspace_group(&mut txn, &paths, group)
            .unwrap();
        txn.commit::<String>().await.unwrap();
    }
    let restored = KeyspaceGroupManager::new(store.clone(), paths);
    restored.bootstrap().await.unwrap();
    let error = restored.finish_split(4).await.unwrap_err();
    assert!(
        error.to_string().contains("request is too large"),
        "{error}"
    );
    assert_eq!(restored.get_group(2).await.unwrap().unwrap(), source);
    assert_eq!(restored.get_group(4).await.unwrap().unwrap(), target);
    assert!(restored.groups.read().await[&1][&4].is_split_target());
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let recovered = KeyspaceGroupManager::new(Arc::new(fixture.store().await), paths);
    recovered.bootstrap().await.unwrap();
    assert_eq!(recovered.get_group(2).await.unwrap().unwrap(), source);
    assert_eq!(recovered.get_group(4).await.unwrap().unwrap(), target);
}

#[tokio::test]
async fn mcs_031_concurrent_splits_have_one_winner() {
    use astersql_pd_storage_domain::{Consistency, Read};
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let manager = Arc::new(KeyspaceGroupManager::new(store.clone(), paths));
    manager
        .create_groups(&[group(2, vec![111, 222, 333])])
        .await
        .unwrap();
    let mut tasks = Vec::new();
    for (target, id) in [(4, 111), (5, 222)] {
        let m = manager.clone();
        tasks.push(tokio::spawn(async move {
            m.split_group(2, target, &[id], &[]).await
        }));
    }
    let mut winners = 0;
    for task in tasks {
        match task.await.unwrap() {
            Ok(()) => winners += 1,
            Err(GroupSplitError::Catalog(GroupCatalogError::InSplit(2))) => {}
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(winners, 1);
    let groups = manager.get_groups(0, 0).await.unwrap();
    assert_eq!(groups.len(), 2);
    let mut keys: Vec<_> = groups
        .iter()
        .flat_map(|g| g.keyspaces.clone().unwrap())
        .collect();
    keys.sort();
    assert_eq!(keys, vec![111, 222, 333]);
    let rows = store
        .read(Read::prefix(
            paths.keyspace_group_id_prefix().as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .entries;
    assert_eq!(
        rows[0].mod_revision, rows[1].mod_revision,
        "both records belong to one actual commit"
    );
}
