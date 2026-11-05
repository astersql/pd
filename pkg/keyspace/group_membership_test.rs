// Copyright 2026 AsterSQL.
use super::*;
use crate::etcd_fixture as fixture;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{KeyspaceGroup, KeyspaceGroupMember, MetaStore, UserKind};
use std::sync::Arc;
fn group(id: u32, keys: Vec<u32>, address: &str, priority: i64) -> KeyspaceGroup {
    KeyspaceGroup {
        id,
        user_kind: "standard".into(),
        keyspaces: Some(keys),
        members: Some(vec![KeyspaceGroupMember {
            address: address.into(),
            priority,
        }]),
        ..Default::default()
    }
}
#[tokio::test]
async fn mcs_030_behavior() {
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let manager = KeyspaceGroupManager::new(store, ClusterPaths::new(42));
    let old = group(1, vec![42, 43], "http://old-tso", 1);
    let new = group(2, vec![44, 45], "http://new-tso", 2);
    manager
        .create_groups(&[old.clone(), new.clone()])
        .await
        .unwrap();
    // Match Go's stale-cache input: both persisted lists and members are newer.
    {
        let mut index = manager.groups.write().await;
        index
            .get_mut(&1)
            .unwrap()
            .insert(1, group(1, vec![42], "stale", 0));
        index
            .get_mut(&1)
            .unwrap()
            .insert(2, group(2, vec![44], "stale", 0));
    }
    manager
        .update_keyspace_group("1", "2", UserKind::STANDARD, UserKind::STANDARD, 42)
        .await
        .unwrap();
    let got_old = manager.get_group(1).await.unwrap().unwrap();
    let got_new = manager.get_group(2).await.unwrap().unwrap();
    assert_eq!(got_old.keyspaces, Some(vec![43]));
    assert_eq!(got_new.keyspaces, Some(vec![42, 44, 45]));
    assert_eq!(got_old.members, old.members);
    assert_eq!(got_new.members, new.members);
    manager
        .update_keyspace_group("1", "2", UserKind::STANDARD, UserKind::STANDARD, 42)
        .await
        .unwrap();
    assert_eq!(
        manager.get_group(2).await.unwrap().unwrap().keyspaces,
        Some(vec![42, 44, 45])
    );
    let index = manager.groups.read().await;
    assert_eq!(index[&1][&1], got_old);
    assert_eq!(index[&1][&2], got_new);
}

#[tokio::test]
async fn mcs_030_validation_same_group_and_transition_guards() {
    use astersql_pd_storage_domain::{
        Base, Consistency, MergeState, Read, SplitState, StorageEndpoint,
    };
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let manager = KeyspaceGroupManager::new(store.clone(), paths);
    manager
        .create_groups(&[
            group(1, vec![100, 222, 300], "old", 1),
            group(2, vec![10, 500], "new", 2),
        ])
        .await
        .unwrap();
    let revision = store
        .read(Read::prefix(
            paths.keyspace_group_id_prefix().as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .revision;
    for (old, new, kind, message) in [
        (
            "\0",
            "2",
            UserKind::STANDARD,
            "strconv.ParseUint: parsing \"\\x00\": invalid syntax",
        ),
        (
            "+1",
            "2",
            UserKind::STANDARD,
            "strconv.ParseUint: parsing \"+1\": invalid syntax",
        ),
        (
            "bad",
            "2",
            UserKind::STANDARD,
            "strconv.ParseUint: parsing \"bad\": invalid syntax",
        ),
        (
            "18446744073709551616",
            "2",
            UserKind::STANDARD,
            "strconv.ParseUint: parsing \"18446744073709551616\": value out of range",
        ),
        (
            "9",
            "2",
            UserKind::STANDARD,
            "keyspace group 9 not found in standard group",
        ),
        (
            "1",
            "9",
            UserKind::STANDARD,
            "keyspace group 9 not found in standard group",
        ),
        (
            "1",
            "2",
            UserKind::BASIC,
            "keyspace group 1 not found in basic group",
        ),
    ] {
        assert_eq!(
            manager
                .update_keyspace_group(old, new, kind, UserKind::STANDARD, 222)
                .await
                .unwrap_err()
                .to_string(),
            message
        );
    }
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
    let endpoint = StorageEndpoint::new(store.clone());
    let mut old = manager.get_group(1).await.unwrap().unwrap();
    let mut new = manager.get_group(2).await.unwrap().unwrap();
    for (source, split) in [(true, true), (false, true), (true, false), (false, false)] {
        let group = if source { &mut old } else { &mut new };
        if split {
            group.split_state = Some(SplitState {
                split_source: group.id,
            });
        } else {
            group.merge_state = Some(MergeState { merge_list: None });
        }
        let mut txn = Base::new(store.as_ref()).begin_txn();
        endpoint
            .save_keyspace_group(&mut txn, &paths, group)
            .unwrap();
        txn.commit::<String>().await.unwrap();
        let expected = format!(
            "[PD:keyspace:ErrKeyspaceGroupIn{}]keyspace group {} is in {} state",
            if split { "Split" } else { "Merging" },
            group.id,
            if split { "split" } else { "merging" }
        );
        assert_eq!(
            manager
                .update_keyspace_group("1", "2", UserKind::STANDARD, UserKind::STANDARD, 222)
                .await
                .unwrap_err()
                .to_string(),
            expected
        );
        group.split_state = None;
        group.merge_state = None;
        let mut txn = Base::new(store.as_ref()).begin_txn();
        endpoint
            .save_keyspace_group(&mut txn, &paths, group)
            .unwrap();
        txn.commit::<String>().await.unwrap();
    }
    // Go aliases old/new when IDs compare equal, so the member is removed.
    manager
        .update_keyspace_group("01", "1", UserKind::STANDARD, UserKind::STANDARD, 222)
        .await
        .unwrap();
    assert_eq!(
        manager.get_group(1).await.unwrap().unwrap().keyspaces,
        Some(vec![100, 300])
    );
}

#[tokio::test]
async fn mcs_030_real_commit_failure_and_wal_recovery() {
    let mut fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let manager = KeyspaceGroupManager::new(store.clone(), paths);
    // Each individual group fits etcd's request limit; their atomic two-write move does not.
    let address = "x".repeat(850_000);
    let old = group(1, vec![100, 222, 300], &address, 1);
    let new = group(2, vec![10, 500], &address, 2);
    manager.create_groups(&[old.clone()]).await.unwrap();
    manager.create_groups(&[new.clone()]).await.unwrap();
    let error = manager
        .update_keyspace_group("1", "2", UserKind::STANDARD, UserKind::STANDARD, 222)
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("request is too large"),
        "{error}"
    );
    assert_eq!(manager.get_group(1).await.unwrap().unwrap(), old);
    assert_eq!(manager.get_group(2).await.unwrap().unwrap(), new);
    {
        let index = manager.groups.read().await;
        assert_eq!(index[&1][&1], old);
        assert_eq!(index[&1][&2], new);
    }
    fixture.command("stop", "stopped");
    assert!(
        manager
            .update_keyspace_group("1", "2", UserKind::STANDARD, UserKind::STANDARD, 222)
            .await
            .is_err()
    );
    fixture.command("start", "restarted");
    let restored = KeyspaceGroupManager::new(store, paths);
    restored.bootstrap().await.unwrap();
    assert_eq!(restored.get_group(1).await.unwrap().unwrap(), old);
    assert_eq!(restored.get_group(2).await.unwrap().unwrap(), new);
}

#[tokio::test]
async fn mcs_030_concurrent_moves_do_not_lose_members() {
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let manager = Arc::new(KeyspaceGroupManager::new(store, ClusterPaths::new(42)));
    manager
        .create_groups(&[
            group(1, vec![42, 43], "old", 1),
            group(2, vec![44, 45], "new", 2),
        ])
        .await
        .unwrap();
    let mut tasks = Vec::new();
    for id in [42, 43, 42, 43] {
        let manager = manager.clone();
        tasks.push(tokio::spawn(async move {
            manager
                .update_keyspace_group("1", "2", UserKind::STANDARD, UserKind::STANDARD, id)
                .await
        }));
    }
    for task in tasks {
        task.await.unwrap().unwrap();
    }
    assert_eq!(
        manager.get_group(1).await.unwrap().unwrap().keyspaces,
        Some(vec![])
    );
    assert_eq!(
        manager.get_group(2).await.unwrap().unwrap().keyspaces,
        Some(vec![42, 43, 44, 45])
    );
    let index = manager.groups.read().await;
    assert_eq!(index[&1][&2].keyspaces, Some(vec![42, 43, 44, 45]));
}
