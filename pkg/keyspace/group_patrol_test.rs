// Copyright 2026 AsterSQL.
use super::*;
use crate::etcd_fixture as fixture;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{
    Base, KeyspaceGroup, KeyspaceGroupMember, MetaStore, StorageEndpoint,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
fn group(id: u32, start: u32, count: u32) -> KeyspaceGroup {
    KeyspaceGroup {
        id,
        user_kind: if id == 0 {
            "basic".into()
        } else {
            "standard".into()
        },
        keyspaces: Some((start..start + count).collect()),
        members: Some(vec![KeyspaceGroupMember::default(); 2]),
        ..Default::default()
    }
}
async fn save(store: &Arc<dyn MetaStore>, paths: &ClusterPaths, group: &KeyspaceGroup) {
    let mut txn = Base::new(store.as_ref()).begin_txn();
    StorageEndpoint::new(store.clone())
        .save_keyspace_group(&mut txn, paths, group)
        .unwrap();
    txn.commit::<String>().await.unwrap();
}
#[tokio::test]
async fn mcs_035_behavior() {
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let manager = KeyspaceGroupManager::new(store.clone(), paths);
    manager.bootstrap().await.unwrap();
    save(&store, &paths, &group(0, 0, 40_001)).await;
    manager
        .patrol_auto_split(&CancellationToken::new())
        .await
        .unwrap();
    let target = manager.get_group(1).await.unwrap();
    assert!(
        target.is_some(),
        "a 40001-keyspace group must be split by the patrol"
    );
    let source = manager.get_group(0).await.unwrap().unwrap();
    let target = target.unwrap();
    assert_eq!(source.keyspaces, Some((0..20_000).collect()));
    assert_eq!(target.keyspaces, Some((20_000..40_001).collect()));
    assert!(source.is_split_source());
    assert!(target.is_split_target());
    assert_eq!(target.split_source(), 0);
}

#[tokio::test]
async fn mcs_035_threshold_cancel_and_protected_only_tail() {
    let mut fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let manager = KeyspaceGroupManager::new(store.clone(), paths);
    save(&store, &paths, &group(0, 0, 40_000)).await;
    manager.bootstrap().await.unwrap();
    assert!(
        manager
            .patrol_auto_split(&CancellationToken::new())
            .await
            .unwrap()
            .split
            .is_none()
    );
    assert_eq!(
        manager
            .get_group(0)
            .await
            .unwrap()
            .unwrap()
            .keyspaces
            .unwrap()
            .len(),
        40_000
    );
    assert!(manager.get_group(1).await.unwrap().is_none());
    let initial = group(0, 0, 40_001);
    save(&store, &paths, &initial).await;
    let cancel = CancellationToken::new();
    cancel.cancel();
    fixture.command("stop", "stopped");
    assert!(
        manager
            .patrol_auto_split(&cancel)
            .await
            .unwrap()
            .split
            .is_none()
    );
    fixture.command("start", "restarted");
    let fresh: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let manager = KeyspaceGroupManager::new(fresh.clone(), paths);
    assert_eq!(manager.get_group(0).await.unwrap().unwrap(), initial);
    let mut protected = group(0, 1, 20_000);
    protected
        .keyspaces
        .as_mut()
        .unwrap()
        .extend(vec![0; 20_001]);
    save(&fresh, &paths, &protected).await;
    assert!(
        manager
            .patrol_auto_split(&CancellationToken::new())
            .await
            .unwrap()
            .split
            .is_none()
    );
    assert_eq!(manager.get_group(0).await.unwrap().unwrap(), protected);
    assert!(manager.get_group(1).await.unwrap().is_none());
}
#[tokio::test]
async fn mcs_035_largest_ties_transition_and_replica_guards() {
    use astersql_pd_storage_domain::{MergeState, SplitState};
    for scenario in 0..4 {
        let fixture = fixture::Fixture::start();
        let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
        let paths = ClusterPaths::new(42);
        let manager = KeyspaceGroupManager::new(store.clone(), paths);
        let mut groups = match scenario {
            0 => vec![group(0, 0, 40_001), group(1, 100_000, 40_002)],
            1 => vec![
                group(0, 0, 40_001),
                group(1, 100_000, 40_001),
                group(2, 200_000, 40_001),
            ],
            2 => vec![group(0, 0, 40_001), group(1, 100_000, 40_001)],
            _ => vec![group(0, 0, 40_001), group(1, 100_000, 40_001)],
        };
        if scenario == 1 {
            groups[0].split_state = Some(SplitState { split_source: 0 });
            groups[1].merge_state = Some(MergeState {
                merge_list: Some(vec![1]),
            });
        }
        if scenario == 2 {
            groups[0].members = Some(vec![KeyspaceGroupMember::default()]);
        }
        for group in &groups {
            save(&store, &paths, group).await;
        }
        manager.bootstrap().await.unwrap();
        let report = manager
            .patrol_auto_split(&CancellationToken::new())
            .await
            .unwrap();
        let (source, target) = match scenario {
            0 => (1, 2),
            1 => (2, 3),
            2 => (1, 2),
            _ => (0, 2),
        };
        assert_eq!(report.split, Some((source, target)));
        assert!(report.split_error.is_none());
        let original = groups
            .iter()
            .find(|g| g.id == source)
            .unwrap()
            .keyspaces
            .as_ref()
            .unwrap();
        let split_index = original.len() / 2;
        assert_eq!(
            manager
                .get_group(source)
                .await
                .unwrap()
                .unwrap()
                .keyspaces
                .as_ref()
                .unwrap(),
            &original[..split_index]
        );
        assert_eq!(
            manager
                .get_group(target)
                .await
                .unwrap()
                .unwrap()
                .keyspaces
                .as_ref()
                .unwrap(),
            &original[split_index..]
        );
        for untouched in groups.iter().filter(|g| g.id != source) {
            assert_eq!(
                manager.get_group(untouched.id).await.unwrap().unwrap(),
                *untouched
            );
        }
        assert_eq!(
            manager.get_groups(0, 0).await.unwrap().len(),
            groups.len() + 1,
            "one split per patrol"
        );
    }
}
#[tokio::test]
async fn mcs_035_smallest_id_and_real_dense_exhaustion() {
    use astersql_pd_storage_domain::{Consistency, Mutation, Read};
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let manager = KeyspaceGroupManager::new(store.clone(), paths);
    save(&store, &paths, &group(0, 0, 40_001)).await;
    for (id, key) in [(1, 101), (2, 102), (3, 103), (4, 104), (4095, 105)] {
        save(&store, &paths, &group(id, key, 1)).await;
    }
    manager.bootstrap().await.unwrap();
    assert_eq!(
        manager
            .patrol_auto_split(&CancellationToken::new())
            .await
            .unwrap()
            .split,
        Some((0, 5))
    );
    // Seed the same full 4096-ID directory as Go, in native transaction-sized fixture chunks.
    let all: Vec<_> = (0..4096)
        .map(|id| {
            if id == 0 {
                group(0, 0, 40_001)
            } else {
                group(id, id + 1000, 1)
            }
        })
        .collect();
    for chunk in all.chunks(100) {
        let writes = chunk
            .iter()
            .map(|group| {
                Mutation::put(
                    paths.keyspace_group_id_path(group.id).as_bytes(),
                    &serde_json::to_vec(group).unwrap(),
                )
            })
            .collect();
        assert!(store.transact(vec![], writes).await.unwrap().succeeded);
    }
    let manager = KeyspaceGroupManager::new(store.clone(), paths);
    manager.bootstrap().await.unwrap();
    let before = store
        .read(Read::prefix(
            paths.keyspace_group_id_prefix().as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert_eq!(before.entries.len(), 4096);
    let report = manager
        .patrol_auto_split(&CancellationToken::new())
        .await
        .unwrap();
    assert!(report.split.is_none());
    assert!(report.split_error.is_none());
    let after = store
        .read(Read::prefix(
            paths.keyspace_group_id_prefix().as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert_eq!(before.revision, after.revision);
    assert_eq!(manager.get_group(0).await.unwrap().unwrap(), all[0]);
}
#[test]
fn mcs_035_replays_go_id_selection_cases() {
    for (ids, max, expected) in [
        (vec![0, 1, 2, 3, 4, 4095], 4096, Some(5)),
        (vec![0, 1, 2, 5], 4096, Some(3)),
        (vec![0, 1, 2], 6, Some(3)),
        (vec![0, 1, 2, 3, 4, 5], 6, None),
        (vec![0], 1, None),
    ] {
        let groups: Vec<_> = ids
            .into_iter()
            .map(|id| KeyspaceGroup {
                id,
                ..Default::default()
            })
            .collect();
        assert_eq!(find_next_available_group_id(&groups, max), expected);
    }
}
#[tokio::test]
async fn mcs_035_deferred_reconciliation_uses_loaded_snapshot() {
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let nodes = Arc::new(astersql_pd_balancer::RoundRobin::new());
    for node in ["http://tso-1", "https://tso-1", "http://tso-2"] {
        nodes.put(node.to_owned());
    }
    let manager = KeyspaceGroupManager::new(store, paths).with_nodes_balancer(nodes);
    let mut initial = group(1, 42, 1);
    initial.members = Some(vec![KeyspaceGroupMember {
        address: "http://tso-1".into(),
        priority: 0,
    }]);
    manager.create_groups(&[initial]).await.unwrap();
    let report = manager
        .patrol_auto_split(&CancellationToken::new())
        .await
        .unwrap();
    assert!(report.split.is_none());
    assert_eq!(report.reconcile.attempted, vec![1]);
    assert!(report.reconcile.failures.is_empty());
    let members = manager
        .get_group(1)
        .await
        .unwrap()
        .unwrap()
        .members
        .unwrap();
    assert_eq!(members.len(), 2);
    assert_ne!(
        astersql_pd_typeutil::trim_scheme(&members[0].address),
        astersql_pd_typeutil::trim_scheme(&members[1].address)
    );
}
#[tokio::test]
async fn mcs_035_real_commit_failure_and_wal_recovery() {
    let mut fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let manager = KeyspaceGroupManager::new(store.clone(), paths);
    let mut initial = group(0, 0, 40_001);
    for member in initial.members.as_mut().unwrap() {
        member.address = "x".repeat(400_000);
    }
    save(&store, &paths, &initial).await;
    manager.bootstrap().await.unwrap();
    let report = manager
        .patrol_auto_split(&CancellationToken::new())
        .await
        .unwrap();
    assert!(report.split.is_none());
    assert!(
        report
            .split_error
            .as_ref()
            .unwrap()
            .to_string()
            .contains("request is too large")
    );
    assert_eq!(manager.get_group(0).await.unwrap().unwrap(), initial);
    assert!(manager.get_group(1).await.unwrap().is_none());
    fixture.command("stop", "stopped");
    assert!(
        manager
            .patrol_auto_split(&CancellationToken::new())
            .await
            .is_err()
    );
    assert_eq!(manager.groups.read().await[&0][&0], initial);
    fixture.command("start", "restarted");
    let fresh: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let manager = KeyspaceGroupManager::new(fresh.clone(), paths);
    assert_eq!(manager.get_group(0).await.unwrap().unwrap(), initial);
    let clean = group(0, 0, 40_001);
    save(&fresh, &paths, &clean).await;
    assert_eq!(
        manager
            .patrol_auto_split(&CancellationToken::new())
            .await
            .unwrap()
            .split,
        Some((0, 1))
    );
}
#[tokio::test]
async fn mcs_035_immutable_term_fences_patrol() {
    use astersql_pd_election::{ElectionError, Leadership};
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let manager = Arc::new(KeyspaceGroupManager::new(store.clone(), paths));
    let initial = group(0, 0, 40_001);
    save(&store, &paths, &initial).await;
    let previous = Arc::new(
        Leadership::campaign(
            store.clone(),
            CancellationToken::new(),
            42,
            b"same-leader".to_vec(),
            60,
        )
        .await
        .unwrap(),
    );
    let old = manager
        .start_reconcile(previous.clone(), &CancellationToken::new())
        .await
        .unwrap();
    previous.resign().await.unwrap();
    let current = Arc::new(
        Leadership::campaign(
            store,
            CancellationToken::new(),
            42,
            b"same-leader".to_vec(),
            60,
        )
        .await
        .unwrap(),
    );
    let report = manager.patrol_auto_split_in_term(&old.term).await.unwrap();
    assert!(matches!(
        report.split_error,
        Some(GroupSplitError::Leadership(ElectionError::NotLeader))
    ));
    assert_eq!(manager.get_group(0).await.unwrap().unwrap(), initial);
    assert!(manager.get_group(1).await.unwrap().is_none());
    let active = manager
        .start_reconcile(current.clone(), &CancellationToken::new())
        .await
        .unwrap();
    assert!(
        manager
            .patrol_auto_split_in_term(&old.term)
            .await
            .unwrap()
            .split
            .is_none()
    );
    assert_eq!(
        manager
            .patrol_auto_split_in_term(&active.term)
            .await
            .unwrap()
            .split,
        Some((0, 1))
    );
    current.resign().await.unwrap();
}
