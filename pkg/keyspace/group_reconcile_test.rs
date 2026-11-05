// Copyright 2026 AsterSQL.
use super::*;
use crate::etcd_fixture as fixture;
use astersql_pd_election::Leadership;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{KeyspaceGroup, KeyspaceGroupMember, MetaStore};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test]
async fn mcs_034_behavior() {
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let manager = Arc::new(KeyspaceGroupManager::new(
        store.clone(),
        ClusterPaths::new(42),
    ));
    manager.bootstrap().await.unwrap();
    manager
        .create_groups(&[KeyspaceGroup {
            id: 7,
            user_kind: "standard".into(),
            members: Some(vec![KeyspaceGroupMember {
                address: "http://tso-1".into(),
                priority: 1,
            }]),
            keyspaces: Some(vec![1, 2, 3]),
            ..Default::default()
        }])
        .await
        .unwrap();
    let leadership = Arc::new(
        Leadership::campaign(store, CancellationToken::new(), 42, b"leader".to_vec(), 60)
            .await
            .unwrap(),
    );
    let observer = manager
        .start_reconcile(leadership.clone(), &CancellationToken::new())
        .await
        .unwrap();
    assert!(
        observer.term.entries().iter().any(|g| g.id == 7),
        "initial group watch must publish persisted membership"
    );
    leadership.resign().await.unwrap();
}

fn group(id: u32, address: &str, keys: Vec<u32>) -> KeyspaceGroup {
    KeyspaceGroup {
        id,
        user_kind: "standard".into(),
        members: if address.is_empty() {
            None
        } else {
            Some(vec![KeyspaceGroupMember {
                address: address.into(),
                priority: 0,
            }])
        },
        keyspaces: Some(keys),
        ..Default::default()
    }
}
async fn save(store: &Arc<dyn MetaStore>, paths: &ClusterPaths, group: &KeyspaceGroup) {
    let mut txn = astersql_pd_storage_domain::Base::new(store.as_ref()).begin_txn();
    astersql_pd_storage_domain::StorageEndpoint::new(store.clone())
        .save_keyspace_group(&mut txn, paths, group)
        .unwrap();
    txn.commit::<String>().await.unwrap();
}
fn registrations(
    paths: &ClusterPaths,
    addresses: &[String],
) -> Vec<astersql_pd_storage_domain::Mutation> {
    addresses.iter().map(|address|astersql_pd_storage_domain::Mutation::put(paths.registry_path("tso",address).as_bytes(),serde_json::json!({"name":address,"service-addr":address,"version":"fixture","git-hash":"","deploy-path":"","start-timestamp":0}).to_string().as_bytes())).collect()
}
fn listeners() -> ([std::net::TcpListener; 3], Vec<String>) {
    let sockets = [
        std::net::TcpListener::bind("127.0.0.1:0").unwrap(),
        std::net::TcpListener::bind("127.0.0.1:0").unwrap(),
        std::net::TcpListener::bind("127.0.0.1:0").unwrap(),
    ];
    let addresses = sockets
        .iter()
        .map(|socket| format!("http://{}", socket.local_addr().unwrap()))
        .collect();
    (sockets, addresses)
}
#[tokio::test]
async fn mcs_034_tracks_external_changes_like_go() {
    use astersql_pd_storage_domain::{Base, SplitState, StorageEndpoint};
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let manager = Arc::new(KeyspaceGroupManager::new(store.clone(), paths));
    manager.bootstrap().await.unwrap();
    manager
        .create_groups(&[group(1, "http://initial-tso", vec![41])])
        .await
        .unwrap();
    let leadership = Arc::new(
        Leadership::campaign(
            store.clone(),
            CancellationToken::new(),
            42,
            b"leader".to_vec(),
            60,
        )
        .await
        .unwrap(),
    );
    let mut observer = manager
        .start_reconcile(leadership.clone(), &CancellationToken::new())
        .await
        .unwrap();
    let mut external = group(2, "http://tso-1", vec![42]);
    save(&store, &paths, &external).await;
    observer.groups.next().await.unwrap();
    assert!(
        observer
            .term
            .entries()
            .iter()
            .any(|g| g.id == 2 && g.members.as_ref().unwrap()[0].address == "http://tso-1")
    );
    assert!(!manager.groups.read().await[&1].contains_key(&2));
    external.user_kind = "enterprise".into();
    external.split_state = Some(SplitState { split_source: 2 });
    external.members.as_mut().unwrap()[0].address = "http://tso-2".into();
    external.keyspaces = Some(vec![42, 43]);
    save(&store, &paths, &external).await;
    observer.groups.next().await.unwrap();
    assert!(observer.term.entries().iter().any(|g| g.id == 2
        && g.transitioning
        && g.members.as_ref().unwrap()[0].address == "http://tso-2"));
    let mut txn = Base::new(store.as_ref()).begin_txn();
    StorageEndpoint::new(store.clone())
        .delete_keyspace_group(&mut txn, &paths, 2)
        .unwrap();
    txn.commit::<String>().await.unwrap();
    observer.groups.next().await.unwrap();
    assert!(!observer.term.entries().iter().any(|g| g.id == 2));
    let (_sockets, addresses) = listeners();
    store
        .transact(vec![], registrations(&paths, &addresses[..2]))
        .await
        .unwrap();
    observer.nodes.next().await.unwrap();
    assert_eq!(manager.get_nodes_count(), 2);
    let report = manager.reconcile_once(&observer.term).await.unwrap();
    assert_eq!(report.attempted, vec![0, 1]);
    assert!(report.failures.is_empty());
    let actual = manager.get_group(1).await.unwrap().unwrap();
    assert_eq!(actual.keyspaces, Some(vec![41]));
    assert_eq!(actual.members.as_ref().unwrap().len(), 2);
    while observer
        .term
        .entries()
        .iter()
        .find(|g| g.id == 1)
        .unwrap()
        .members
        .as_ref()
        .unwrap()
        .len()
        != 2
    {
        tokio::time::timeout(std::time::Duration::from_secs(3), observer.groups.next())
            .await
            .unwrap()
            .unwrap();
    }
    leadership.resign().await.unwrap();
}
#[tokio::test]
async fn mcs_034_compaction_reload_and_same_wal_restart() {
    use astersql_pd_storage_domain::{Base, Consistency, Mutation, Read, StorageEndpoint};
    let mut fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let (_sockets, addresses) = listeners();
    let manager = Arc::new(KeyspaceGroupManager::new(store.clone(), paths));
    manager
        .create_groups(&[group(1, "", vec![41]), group(2, "", vec![42])])
        .await
        .unwrap();
    store
        .transact(vec![], registrations(&paths, &addresses[..2]))
        .await
        .unwrap();
    let leadership = Arc::new(
        Leadership::campaign(
            store.clone(),
            CancellationToken::new(),
            42,
            b"leader".to_vec(),
            60,
        )
        .await
        .unwrap(),
    );
    let mut observer = manager
        .start_reconcile(leadership.clone(), &CancellationToken::new())
        .await
        .unwrap();
    let old_cursor = observer.groups.next_revision();
    let mut txn = Base::new(store.as_ref()).begin_txn();
    let endpoint = StorageEndpoint::new(store.clone());
    endpoint.delete_keyspace_group(&mut txn, &paths, 2).unwrap();
    endpoint
        .save_keyspace_group(&mut txn, &paths, &group(3, "", vec![43]))
        .unwrap();
    txn.commit::<String>().await.unwrap();
    let mut changes = registrations(&paths, &addresses[2..]);
    changes.push(Mutation::Delete {
        key: paths.registry_path("tso", &addresses[1]).into_bytes(),
    });
    store.transact(vec![], changes).await.unwrap();
    let revision = store
        .read(Read::prefix(b"", Consistency::Linearizable))
        .await
        .unwrap()
        .revision;
    assert!(revision >= old_cursor);
    store.compact(revision).await.unwrap();
    observer.groups.next().await.unwrap();
    observer.nodes.next().await.unwrap();
    assert_eq!(
        observer
            .term
            .entries()
            .iter()
            .map(|g| g.id)
            .collect::<Vec<_>>(),
        vec![1, 3]
    );
    assert_eq!(manager.get_nodes_count(), 2);
    assert!(manager.registered_node(&addresses[1]).is_none());
    assert!(observer.groups.next_revision() > revision);
    let report = manager.reconcile_once(&observer.term).await.unwrap();
    assert_eq!(report.attempted, vec![1, 3]);
    assert!(report.failures.is_empty());
    for id in [1, 3] {
        let value = manager.get_group(id).await.unwrap().unwrap();
        let mut nodes = value
            .members
            .unwrap()
            .into_iter()
            .map(|m| m.address)
            .collect::<Vec<_>>();
        nodes.sort();
        let mut expected = vec![addresses[0].clone(), addresses[2].clone()];
        expected.sort();
        assert_eq!(nodes, expected);
    }
    while observer
        .term
        .entries()
        .iter()
        .any(|group| group.members.as_ref().map_or(0, Vec::len) != 2)
    {
        tokio::time::timeout(std::time::Duration::from_secs(3), observer.groups.next())
            .await
            .unwrap()
            .unwrap();
    }
    fixture.command("stop", "stopped");
    // A healthy indexed pass must not scan unavailable storage.
    assert!(
        manager
            .reconcile_once(&observer.term)
            .await
            .unwrap()
            .attempted
            .is_empty()
    );

    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(3), observer.groups.next())
            .await
            .unwrap()
            .is_err()
    );
    fixture.command("start", "restarted");
    let fresh: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    save(&fresh, &paths, &group(4, "", vec![44])).await;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    while !observer.term.entries().iter().any(|g| g.id == 4) {
        match tokio::time::timeout(std::time::Duration::from_secs(3), observer.groups.next())
            .await
            .unwrap()
        {
            Ok(_) => {}
            Err(astersql_pd_storage_domain::StoreError::Unavailable) => {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await
            }
            other => panic!("{other:?}"),
        }
        assert!(tokio::time::Instant::now() < deadline);
    }
    let report = manager.reconcile_once(&observer.term).await.unwrap();
    assert!(report.failures.is_empty());
    assert!(report.attempted.contains(&4));
    assert_eq!(
        manager
            .get_group(4)
            .await
            .unwrap()
            .unwrap()
            .members
            .unwrap()
            .len(),
        2
    );
    assert!(manager.get_group(2).await.unwrap().is_none());
    leadership.resign().await.unwrap();
}
#[tokio::test]
async fn mcs_034_term_aba_fences_old_writes_and_observers() {
    use astersql_pd_election::ElectionError;
    use astersql_pd_storage_domain::{Consistency, Read, StoreError};
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let (_sockets, addresses) = listeners();
    let manager = Arc::new(KeyspaceGroupManager::new(store.clone(), paths));
    manager
        .create_groups(&[group(1, "", vec![41])])
        .await
        .unwrap();
    store
        .transact(vec![], registrations(&paths, &addresses[..2]))
        .await
        .unwrap();
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
    let mut old = manager
        .start_reconcile(previous.clone(), &CancellationToken::new())
        .await
        .unwrap();
    previous.resign().await.unwrap();
    let current = Arc::new(
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
    assert_ne!(previous.lease_id(), current.lease_id());
    assert_ne!(previous.revision(), current.revision());
    save(&store, &paths, &group(10, "", vec![50])).await;
    let before = store
        .read(Read::range(
            paths.keyspace_group_id_path(10).as_bytes(),
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    // The old local token is still active: the actual lease/revision fence must reject this write.
    assert!(!old.term.is_cancelled());
    assert!(matches!(
        manager
            .alloc_nodes_in_term(10, None, 2, &CancellationToken::new(), Some(&old.term))
            .await,
        Err(GroupNodesError::Leadership(ElectionError::NotLeader))
    ));
    let after = store
        .read(Read::range(
            paths.keyspace_group_id_path(10).as_bytes(),
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert_eq!(before.entries[0].value, after.entries[0].value);
    assert_eq!(
        before.entries[0].mod_revision,
        after.entries[0].mod_revision
    );
    println!(
        "MCS034_FENCE {}",
        serde_json::json!({"old_lease":previous.lease_id(),"new_lease":current.lease_id(),"old_term_revision":previous.revision(),"new_term_revision":current.revision(),"group_revision_before":before.entries[0].mod_revision,"group_revision_after":after.entries[0].mod_revision})
    );
    assert!(!manager.groups.read().await[&1].contains_key(&10));
    let observer = manager
        .start_reconcile(current.clone(), &CancellationToken::new())
        .await
        .unwrap();
    assert!(old.term.is_cancelled());
    assert_eq!(old.groups.next().await.unwrap_err(), StoreError::Cancelled);
    assert!(manager.reconcile_once(&old.term).await.is_err());
    assert!(!old.term.entries().iter().any(|g| g.id == 10));
    assert!(observer.term.entries().iter().any(|g| g.id == 10));
    let report = manager.reconcile_once(&observer.term).await.unwrap();
    assert!(report.failures.is_empty());
    assert_eq!(
        manager
            .get_group(10)
            .await
            .unwrap()
            .unwrap()
            .members
            .unwrap()
            .len(),
        2
    );
    current.resign().await.unwrap();
}
#[tokio::test]
async fn mcs_034_invalid_batch_is_atomic_and_reloads_after_repair() {
    use astersql_pd_storage_domain::{Mutation, StoreError};
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let manager = Arc::new(KeyspaceGroupManager::new(store.clone(), paths));
    manager
        .create_groups(&[group(1, "", vec![41])])
        .await
        .unwrap();
    let leadership = Arc::new(
        Leadership::campaign(
            store.clone(),
            CancellationToken::new(),
            42,
            b"leader".to_vec(),
            60,
        )
        .await
        .unwrap(),
    );
    let mut observer = manager
        .start_reconcile(leadership.clone(), &CancellationToken::new())
        .await
        .unwrap();
    let cursor = observer.groups.next_revision();
    store
        .transact(
            vec![],
            vec![
                Mutation::put(
                    paths.keyspace_group_id_path(2).as_bytes(),
                    &serde_json::to_vec(&group(2, "", vec![42])).unwrap(),
                ),
                Mutation::put(
                    paths.keyspace_group_id_path(3).as_bytes(),
                    &serde_json::to_vec(&group(4, "", vec![43])).unwrap(),
                ),
            ],
        )
        .await
        .unwrap();
    assert_eq!(
        observer.groups.next().await.unwrap_err(),
        StoreError::Backend
    );
    assert_eq!(observer.groups.next_revision(), cursor);
    assert_eq!(
        observer
            .term
            .entries()
            .iter()
            .map(|g| g.id)
            .collect::<Vec<_>>(),
        vec![1]
    );
    save(&store, &paths, &group(3, "", vec![43])).await;
    observer.groups.reload().await.unwrap();
    assert_eq!(
        observer
            .term
            .entries()
            .iter()
            .map(|g| g.id)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert!(observer.groups.next_revision() > cursor);
    leadership.resign().await.unwrap();
}
#[tokio::test]
async fn mcs_034_prefix_selection_and_error_scope() {
    use astersql_pd_storage_domain::StoreError;
    let entry = decode_group_reconcile_entry(
        br#"{"id":7,"members":[{"address":"http://tso-1","priority":1}],"keyspaces":[1,2,3]}"#,
    )
    .unwrap();
    assert_eq!(entry.id, 7);
    assert!(!entry.transitioning);
    assert_eq!(entry.members.unwrap()[0].priority, 1);
    // Canonical persisted field order lets Go stop without scanning a large keyspace tail.
    assert!(decode_group_reconcile_entry(br#"{"id":7,"members":null,"keyspaces":["#).is_ok());
    assert!(decode_group_reconcile_entry(br#"{"id":7}"#).is_err());
    assert!(abort_group_reconcile(&GroupNodesError::Catalog(
        GroupCatalogError::Store(StoreError::Unavailable)
    )));
    assert!(!abort_group_reconcile(&GroupNodesError::Catalog(
        GroupCatalogError::Store(StoreError::RequestTooLarge)
    )));
    assert!(!abort_group_reconcile(&GroupNodesError::NotExists(9)));
    assert!(abort_group_reconcile(&GroupNodesError::NoAvailableNode));
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let manager = Arc::new(KeyspaceGroupManager::new(
        store.clone(),
        ClusterPaths::new(42),
    ));
    manager
        .create_groups(&[
            group(3, "", vec![]),
            group(1, "", vec![]),
            group(2, "", vec![]),
        ])
        .await
        .unwrap();
    let leadership = Arc::new(
        Leadership::campaign(store, CancellationToken::new(), 42, b"leader".to_vec(), 60)
            .await
            .unwrap(),
    );
    let observer = manager
        .start_reconcile(leadership.clone(), &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(observer.term.groups_needing_allocation(&[]), vec![1, 2, 3]);
    assert!(
        manager
            .reconcile_once(&observer.term)
            .await
            .unwrap()
            .attempted
            .is_empty()
    );
    leadership.resign().await.unwrap();
}

#[tokio::test]
async fn mcs_034_fenced_request_too_large_does_not_abort_other_groups() {
    use astersql_pd_storage_domain::{Mutation, StoreError};
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let (sockets, _addresses) = listeners();
    let addresses: Vec<_> = sockets[..2]
        .iter()
        .map(|socket| {
            format!(
                "http://{}/{}",
                socket.local_addr().unwrap(),
                "x".repeat(700_000)
            )
        })
        .collect();
    for address in &addresses {
        store
            .transact(
                vec![],
                vec![Mutation::put(
                    paths.registry_path("tso", address).as_bytes(),
                    serde_json::json!({"service-addr":address})
                        .to_string()
                        .as_bytes(),
                )],
            )
            .await
            .unwrap();
    }
    let manager = Arc::new(KeyspaceGroupManager::new(store.clone(), paths));
    manager
        .create_groups(&[group(1, "", (0..50_000).collect()), group(2, "", vec![42])])
        .await
        .unwrap();
    let leadership = Arc::new(
        Leadership::campaign(store, CancellationToken::new(), 42, b"leader".to_vec(), 60)
            .await
            .unwrap(),
    );
    let observer = manager
        .start_reconcile(leadership.clone(), &CancellationToken::new())
        .await
        .unwrap();
    let report = manager.reconcile_once(&observer.term).await.unwrap();
    assert_eq!(
        report.attempted,
        vec![1, 2],
        "one oversized group must not stop the following valid group"
    );
    assert!(!report.aborted);
    assert_eq!(report.failures.len(), 1);
    assert!(matches!(
        &report.failures[0].1,
        GroupNodesError::Leadership(astersql_pd_election::ElectionError::Store(
            StoreError::RequestTooLarge
        ))
    ));
    assert!(
        manager
            .get_group(1)
            .await
            .unwrap()
            .unwrap()
            .members
            .is_none()
    );
    assert_eq!(
        manager
            .get_group(2)
            .await
            .unwrap()
            .unwrap()
            .members
            .unwrap()
            .len(),
        2
    );
    leadership.resign().await.unwrap();
}

#[tokio::test]
async fn mcs_034_native_fixture_retries_only_initial_bind_collision() {
    use astersql_pd_storage_domain::{Consistency, Mutation, Read};
    let client = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let peer = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let _held_peer = peer.try_clone().unwrap();
    let fixture = fixture::Fixture::start_with_native_ports(Some((client, peer)));
    let store = fixture.store().await;
    assert!(
        store
            .transact(vec![], vec![Mutation::put(b"/native-bind-retry", b"ready")])
            .await
            .unwrap()
            .succeeded
    );
    let rows = store
        .read(Read::range(
            b"/native-bind-retry",
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert_eq!(rows.entries[0].value, b"ready");
}
