// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{Keyspace, KeyspaceGroup, KeyspaceIdentity, MetaStore};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test]
async fn mcs_042_behavior() {
    let f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let group = KeyspaceGroupManager::new(store.clone(), ClusterPaths::new(42));
    let keyspaces = KeyspaceManager::new(
        astersql_pd_storage_endpoint::StorageEndpoint::new(store)
            .keyspace_storage(ClusterPaths::new(42)),
    );
    group
        .create_groups(&[KeyspaceGroup {
            id: 1,
            user_kind: "basic".into(),
            keyspaces: Some(vec![1, 2, 3]),
            ..Default::default()
        }])
        .await
        .unwrap();
    for (id, state) in [(1, 2), (2, 0)] {
        keyspaces
            .save_new(&Keyspace {
                identity: Some(KeyspaceIdentity::Id(id)),
                name: format!("space{id}"),
                state,
                ..Default::default()
            })
            .await
            .unwrap();
    }
    let result = group
        .remove_keyspaces_from_group(&CancellationToken::new(), 1, &keyspaces, None, &[1, 2, 3])
        .await
        .unwrap();
    assert_eq!(
        result.keyspaces,
        Some(vec![2, 3]),
        "only eligible existing metadata should be removed from membership"
    );
    assert!(keyspaces.load_by_id(1).await.is_err());
    assert!(keyspaces.load_by_id(2).await.is_ok());
}
use astersql_pd_storage_domain::{
    Base, Consistency, MetaServiceGroupStatus, Mutation, Read, StorageEndpoint, UserKind,
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    time::Duration,
};
async fn setup(
    store: Arc<dyn MetaStore>,
    count: usize,
    distinct: bool,
    padding: usize,
) -> (Arc<KeyspaceGroupManager>, Arc<KeyspaceManager>) {
    let paths = ClusterPaths::new(42);
    let groups = Arc::new(KeyspaceGroupManager::new(store.clone(), paths.clone()));
    groups
        .create_groups(&[KeyspaceGroup {
            id: 1,
            user_kind: "standard".into(),
            keyspaces: Some((1..=count as u32).collect()),
            ..Default::default()
        }])
        .await
        .unwrap();
    let directories: BTreeMap<_, _> = (1..=if distinct { count } else { 1 })
        .map(|i| {
            (
                if distinct {
                    format!("g{i}")
                } else {
                    "g".into()
                },
                "127.0.0.1:10001".into(),
            )
        })
        .collect();
    let mgm = Arc::new(MetaServiceGroupManager::new(
        store.clone(),
        paths.clone(),
        directories.clone(),
    ));
    let m = Arc::new(KeyspaceManager::with_resources(
        astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone())
            .keyspace_storage(paths.clone()),
        KeyspaceCreateResources {
            meta_groups: Some(mgm),
            groups: Some(groups.clone()),
            ..Default::default()
        },
    ));
    for id in 1..=count as u32 {
        let mut config = HashMap::from([
            ("tso_keyspace_group_id".into(), "1".into()),
            ("user_kind".into(), "standard".into()),
            (
                "meta_service_group_id".into(),
                if distinct {
                    format!("g{id}")
                } else {
                    "g".into()
                },
            ),
        ]);
        if padding > 0 {
            config.insert("padding".into(), "x".repeat(padding));
        }
        m.save_new(&Keyspace {
            identity: Some(KeyspaceIdentity::Id(id)),
            name: format!("space{id}"),
            state: 2,
            config,
            ..Default::default()
        })
        .await
        .unwrap();
    }
    for chunk in directories.keys().collect::<Vec<_>>().chunks(100) {
        let mut txn = Base::new(store.as_ref()).begin_txn();
        for id in chunk {
            StorageEndpoint::new(store.clone())
                .save_meta_service_group_status(
                    &mut txn,
                    &paths,
                    id,
                    Some(&MetaServiceGroupStatus {
                        enabled: true,
                        assignment_count: if distinct { 1 } else { count as i64 },
                    }),
                )
                .unwrap();
        }
        txn.commit::<String>().await.unwrap();
    }
    (groups, m)
}
async fn counts(
    store: Arc<dyn MetaStore>,
    dirs: &BTreeMap<String, String>,
) -> BTreeMap<String, i64> {
    let mut txn = Base::new(store.as_ref()).begin_txn();
    StorageEndpoint::new(store.clone())
        .load_meta_service_group_status(&mut txn, &ClusterPaths::new(42), dirs)
        .await
        .unwrap()
        .into_iter()
        .map(|(k, v)| (k, v.assignment_count))
        .collect()
}
#[tokio::test]
async fn mcs_042_real_120_op_budget_large_values_and_distinct_assignment_batches() {
    for (n, distinct, padding) in [(122, false, 32 * 1024), (71, true, 0)] {
        let f = crate::etcd_fixture::Fixture::start_with_txn_limit(120);
        let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
        let (g, m) = setup(store.clone(), n, distinct, padding).await;
        let ids = (1..=n as u32).collect::<Vec<_>>();
        let mut job = g
            .removal_job(&CancellationToken::new(), 1, &m, None, &ids)
            .await
            .unwrap();
        let mut removed = BTreeSet::new();
        let mut batches = vec![];
        while let Some(batch) = job.next_batch().await.unwrap() {
            assert!(batch.operation_count <= 120);
            assert!(batch.removed_ids.len() <= 59);
            for id in &batch.removed_ids {
                assert!(removed.insert(*id));
            }
            batches.push((
                batch.removed_ids.len(),
                batch.operation_count,
                batch.revision,
            ));
        }
        assert_eq!(removed.len(), n);
        assert_eq!(batches.len(), if distinct { 2 } else { 3 });
        assert_eq!(batches[0].0, if distinct { 39 } else { 59 });
        assert_eq!(batches[0].1, if distinct { 118 } else { 120 });
        assert!(batches.windows(2).all(|p| p[0].2 < p[1].2));
        assert_eq!(
            g.get_group(1).await.unwrap().unwrap().keyspaces,
            Some(vec![])
        );
        let dirs = m.resources.meta_groups.as_ref().unwrap().get_groups().await;
        assert!(counts(store, &dirs).await.values().all(|n| *n == 0));
        for id in ids {
            assert!(m.cache.get(id).is_none());
            assert!(matches!(
                m.load_by_id(id).await,
                Err(KeyspaceError::NotFound)
            ));
        }
    }
}
#[tokio::test]
async fn mcs_042_partial_cancel_and_stopped_store_retry_preserve_ineligible_members() {
    let mut f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let (g, m) = setup(store.clone(), 130, false, 0).await;
    let mut txn = Base::new(store.as_ref()).begin_txn();
    let mut active = m
        .storage
        .load_keyspace_meta(&mut txn, &m.storage.paths, 130)
        .await
        .unwrap()
        .unwrap();
    active.state = 0;
    m.storage
        .save_keyspace_meta(&mut txn, &m.storage.paths, &active)
        .unwrap();
    txn.commit::<String>().await.unwrap();
    let cancel = CancellationToken::new();
    let ids = (1..=130).collect::<Vec<_>>();
    let mut job = g.removal_job(&cancel, 1, &m, None, &ids).await.unwrap();
    let first = job.next_batch().await.unwrap().unwrap();
    assert_eq!(first.removed_ids, (1..=59).collect::<Vec<_>>());
    cancel.cancel();
    assert!(matches!(
        job.next_batch().await,
        Err(GroupRemovalError::Cancelled)
    ));
    drop(job);
    assert_eq!(
        g.get_group(1).await.unwrap().unwrap().keyspaces,
        Some((60..=130).collect())
    );
    let mut job = g
        .removal_job(&CancellationToken::new(), 1, &m, None, &ids)
        .await
        .unwrap();
    let prepared = job.prepare_next_batch().await.unwrap().unwrap();
    let staged = prepared.removed_ids().to_vec();
    f.command("stop", "stopped");
    assert!(prepared.commit().await.is_err());
    assert!(staged.iter().all(|id| m.cache.get(*id).is_none()));
    assert_eq!(
        g.groups.read().await[&UserKind::STANDARD.0][&1].keyspaces,
        Some((60..=130).collect())
    );
    drop(job);
    f.command("start", "restarted");
    let fresh: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let ng = KeyspaceGroupManager::new(fresh.clone(), ClusterPaths::new(42));
    let nm = KeyspaceManager::with_resources(
        astersql_pd_storage_endpoint::StorageEndpoint::new(fresh.clone())
            .keyspace_storage(ClusterPaths::new(42)),
        KeyspaceCreateResources {
            meta_groups: Some(Arc::new(MetaServiceGroupManager::new(
                fresh.clone(),
                ClusterPaths::new(42),
                BTreeMap::from([("g".into(), String::new())]),
            ))),
            ..Default::default()
        },
    );
    let result = ng
        .remove_keyspaces_from_group(&CancellationToken::new(), 1, &nm, None, &ids)
        .await
        .unwrap();
    assert_eq!(result.keyspaces, Some(vec![130]));
    assert_eq!(nm.load_by_id(130).await.unwrap().state, 0);
    assert_eq!(
        counts(fresh, &BTreeMap::from([("g".into(), String::new())])).await["g"],
        1
    );
}
#[tokio::test]
async fn mcs_042_stage_failure_evicts_only_staged_cache_and_keeps_persistent_rows() {
    let f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let (g, m) = setup(store.clone(), 3, false, 0).await;
    Base::new(store.as_ref())
        .save(
            ClusterPaths::new(42).keyspace_meta_path(2).as_bytes(),
            b"\xff",
        )
        .await
        .unwrap();
    let key = ClusterPaths::new(42).keyspace_meta_path(1);
    let before = store
        .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
        .await
        .unwrap();
    assert!(
        g.remove_keyspaces_from_group(&CancellationToken::new(), 1, &m, None, &[1, 2, 3])
            .await
            .is_err()
    );
    assert!(m.cache.get(1).is_none());
    assert!(m.cache.get(2).is_some());
    assert_eq!(
        before.entries,
        store
            .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
    );
    assert_eq!(
        g.groups.read().await[&UserKind::STANDARD.0][&1].keyspaces,
        Some(vec![1, 2, 3])
    );
    assert_eq!(
        counts(store, &BTreeMap::from([("g".into(), String::new())])).await["g"],
        3
    );
}
#[tokio::test]
async fn mcs_042_lost_real_commit_result_evicts_warm_cache_and_retry_reloads_directory() {
    use std::{
        future::{Future, poll_fn},
        task::Poll,
    };
    let f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let (g, m) = setup(store.clone(), 80, false, 0).await;
    let observer = f.store().await;
    let key = ClusterPaths::new(42).keyspace_group_id_path(1);
    let before = observer
        .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
        .await
        .unwrap();
    let mut watch = observer
        .watch(key.into_bytes(), before.revision + 1)
        .await
        .unwrap();
    let ids = (1..=80).collect::<Vec<_>>();
    let mut job = g
        .removal_job(&CancellationToken::new(), 1, &m, None, &ids)
        .await
        .unwrap();
    let prepared = job.prepare_next_batch().await.unwrap().unwrap();
    let staged = prepared.removed_ids().to_vec();
    let mut commit = Box::pin(prepared.commit());
    let mut seen = false;
    for _ in 0..10 {
        let poll = poll_fn(|cx| Poll::Ready(commit.as_mut().poll(cx))).await;
        assert!(
            poll.is_pending(),
            "commit result must remain unconsumed while actual backend outcome is observed"
        );
        if let Ok(result) = tokio::time::timeout(
            Duration::from_millis(500),
            watch.next(&CancellationToken::new()),
        )
        .await
        {
            let batch = result.unwrap();
            if !batch.events.is_empty() {
                seen = true;
                break;
            }
        }
    }
    assert!(
        seen,
        "real etcd must have committed before dropping the response future"
    );
    drop(commit);
    assert!(staged.iter().all(|id| m.cache.get(*id).is_none()));
    assert!(g.membership_gate.try_write().is_ok());
    assert_eq!(
        g.groups.read().await[&UserKind::STANDARD.0][&1]
            .keyspaces
            .as_ref()
            .unwrap()
            .len(),
        80
    );
    assert_eq!(
        g.get_group(1)
            .await
            .unwrap()
            .unwrap()
            .keyspaces
            .as_ref()
            .unwrap()
            .len(),
        21
    );
    assert!(matches!(
        job.next_batch().await,
        Err(GroupRemovalError::Stopped)
    ));
    drop(job);
    let result = g
        .remove_keyspaces_from_group(&CancellationToken::new(), 1, &m, None, &ids)
        .await
        .unwrap();
    assert_eq!(result.keyspaces, Some(vec![]));
    assert_eq!(
        g.groups.read().await[&UserKind::STANDARD.0][&1].keyspaces,
        Some(vec![])
    );
    assert_eq!(
        counts(store, &BTreeMap::from([("g".into(), String::new())])).await["g"],
        0
    );
}
#[tokio::test]
async fn mcs_042_same_value_new_epoch_fences_later_batch_then_new_term_retries() {
    let f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let (g, m) = setup(store.clone(), 130, false, 0).await;
    let role = astersql_pd_election::Leadership::campaign(
        store.clone(),
        CancellationToken::new(),
        42,
        b"same".to_vec(),
        120,
    )
    .await
    .unwrap();
    let ids = (1..=130).collect::<Vec<_>>();
    let mut job = g
        .removal_job(&CancellationToken::new(), 1, &m, Some(&role), &ids)
        .await
        .unwrap();
    assert_eq!(
        job.next_batch().await.unwrap().unwrap().removed_ids.len(),
        59
    );
    let new_lease = store.grant(120).await.unwrap();
    let key = b"/pd/42/leader";
    store
        .transact(
            vec![],
            vec![Mutation::Put {
                key: key.to_vec(),
                value: b"same".to_vec(),
                lease: Some(new_lease.id),
            }],
        )
        .await
        .unwrap();
    assert!(matches!(
        job.next_batch().await,
        Err(GroupRemovalError::Leadership(
            astersql_pd_election::ElectionError::NotLeader
        ))
    ));
    drop(job);
    assert_eq!(
        g.get_group(1).await.unwrap().unwrap().keyspaces,
        Some((60..=130).collect())
    );
    store
        .transact(vec![], vec![Mutation::Delete { key: key.to_vec() }])
        .await
        .unwrap();
    let fresh = astersql_pd_election::Leadership::campaign(
        store.clone(),
        CancellationToken::new(),
        42,
        b"same".to_vec(),
        120,
    )
    .await
    .unwrap();
    assert_ne!(fresh.revision(), role.revision());
    g.remove_keyspaces_from_group(&CancellationToken::new(), 1, &m, Some(&fresh), &ids)
        .await
        .unwrap();
    assert_eq!(
        g.get_group(1).await.unwrap().unwrap().keyspaces,
        Some(vec![])
    );
    fresh.resign().await.unwrap();
}
#[tokio::test]
async fn mcs_042_large_filtering_preserves_protected_missing_foreign_and_duplicate_members() {
    let f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let (g, m) = setup(store.clone(), 3, false, 0).await;
    let mut txn = Base::new(store.as_ref()).begin_txn();
    let mut group = g.get_group(1).await.unwrap().unwrap();
    group.keyspaces = Some(vec![0, 1, 1, 2, 3, 50000]);
    StorageEndpoint::new(store.clone())
        .save_keyspace_group(&mut txn, &ClusterPaths::new(42), &group)
        .unwrap();
    let mut active = m
        .storage
        .load_keyspace_meta(&mut txn, &m.storage.paths, 3)
        .await
        .unwrap()
        .unwrap();
    active.state = 0;
    m.storage
        .save_keyspace_meta(&mut txn, &m.storage.paths, &active)
        .unwrap();
    txn.commit::<String>().await.unwrap();
    m.save_new(&Keyspace {
        identity: Some(KeyspaceIdentity::Id(6)),
        name: "foreign".into(),
        state: 2,
        ..Default::default()
    })
    .await
    .unwrap();
    let mut ids = vec![0, 1, 1, 2, 3, 6, 50000];
    ids.extend(1_000_000..1_000_100);
    let result = g
        .remove_keyspaces_from_group(&CancellationToken::new(), 1, &m, None, &ids)
        .await
        .unwrap();
    assert_eq!(result.keyspaces, Some(vec![0, 3, 50000]));
    assert!(m.load_by_id(6).await.is_ok());
    assert_eq!(
        counts(store, &BTreeMap::from([("g".into(), String::new())])).await["g"],
        1
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_042_membership_gate_blocks_relocation_but_allows_new_members_between_batches() {
    let f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let (g, m) = setup(store.clone(), 80, false, 0).await;
    g.create_groups(&[KeyspaceGroup {
        id: 2,
        user_kind: "enterprise".into(),
        keyspaces: Some(vec![]),
        ..Default::default()
    }])
    .await
    .unwrap();
    let mut txn = Base::new(store.as_ref()).begin_txn();
    let mut active = m
        .storage
        .load_keyspace_meta(&mut txn, &m.storage.paths, 80)
        .await
        .unwrap()
        .unwrap();
    active.state = 0;
    m.storage
        .save_keyspace_meta(&mut txn, &m.storage.paths, &active)
        .unwrap();
    txn.commit::<String>().await.unwrap();
    let mut m = Arc::try_unwrap(m).ok().unwrap();
    let labeler = Arc::new(tokio::sync::Mutex::new(
        astersql_pd_schedule_labeler::RegionLabeler::load(
            StorageEndpoint::new(store.clone()),
            ClusterPaths::new(42),
            0,
        )
        .await
        .unwrap(),
    ));
    m.resources.splitter = Some(Arc::new(KeyspaceRegionSplit::new(
        Some(labeler),
        None,
        Arc::new(std::sync::RwLock::new(KeyspaceSplitConfig::default())),
    )));
    m.resources.config.write().unwrap().wait_region_split = false;
    let m = Arc::new(m);
    let ids = (1..=80).collect::<Vec<_>>();
    let mut job = g
        .removal_job(&CancellationToken::new(), 1, &m, None, &ids)
        .await
        .unwrap();
    assert_eq!(
        job.next_batch().await.unwrap().unwrap().removed_ids.len(),
        59
    );
    let started = Arc::new(tokio::sync::Notify::new());
    let ready = started.clone();
    let worker = m.clone();
    let writer = tokio::spawn(async move {
        ready.notify_one();
        worker
            .update_config(
                "space80",
                &[
                    ConfigMutation {
                        op: ConfigOp::Put,
                        key: "tso_keyspace_group_id".into(),
                        value: "2".into(),
                    },
                    ConfigMutation {
                        op: ConfigOp::Put,
                        key: "user_kind".into(),
                        value: "enterprise".into(),
                    },
                ],
            )
            .await
    });
    started.notified().await;
    tokio::task::yield_now().await;
    assert!(!writer.is_finished());
    let new = tokio::time::timeout(
        Duration::from_secs(3),
        m.create_by_id(
            &CancellationToken::new(),
            CreateKeyspaceByIdRequest {
                id: Some(10000),
                name: "new_member".into(),
                config: HashMap::from([("user_kind".into(), "standard".into())]),
                create_time: 100,
            },
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(new.config["tso_keyspace_group_id"], "1");
    assert!(!writer.is_finished());
    let result = job.run().await.unwrap();
    assert_eq!(result.keyspaces, Some(vec![80, 10000]));
    let moved = tokio::time::timeout(Duration::from_secs(3), writer)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(moved.config["tso_keyspace_group_id"], "2");
    assert_eq!(
        g.get_group(1).await.unwrap().unwrap().keyspaces,
        Some(vec![10000])
    );
    assert_eq!(
        g.get_group(2).await.unwrap().unwrap().keyspaces,
        Some(vec![80])
    );
    assert_eq!(
        counts(store, &BTreeMap::from([("g".into(), String::new())])).await["g"],
        2
    );
}
#[tokio::test]
async fn mcs_042_context_scope_and_transition_guards() {
    let f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let (g, m) = setup(store.clone(), 1, false, 0).await;
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(matches!(
        g.remove_keyspaces_from_group(&cancel, 1, &m, None, &[1])
            .await,
        Err(GroupRemovalError::Cancelled)
    ));
    let wrong = KeyspaceManager::new(
        astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone())
            .keyspace_storage(ClusterPaths::new(43)),
    );
    assert!(matches!(
        g.remove_keyspaces_from_group(&CancellationToken::new(), 1, &wrong, None, &[1])
            .await,
        Err(GroupRemovalError::StoreMismatch)
    ));
    assert!(matches!(
        g.remove_keyspaces_from_group(&CancellationToken::new(), 99, &m, None, &[1])
            .await,
        Err(GroupRemovalError::NotExists(99))
    ));
    let mut group = g.get_group(1).await.unwrap().unwrap();
    for split in [true, false] {
        group.split_state = if split {
            Some(Default::default())
        } else {
            None
        };
        group.merge_state = if split {
            None
        } else {
            Some(Default::default())
        };
        let mut txn = Base::new(store.as_ref()).begin_txn();
        StorageEndpoint::new(store.clone())
            .save_keyspace_group(&mut txn, &ClusterPaths::new(42), &group)
            .unwrap();
        txn.commit::<String>().await.unwrap();
        let error = g
            .remove_keyspaces_from_group(&CancellationToken::new(), 1, &m, None, &[1])
            .await
            .unwrap_err();
        assert!(error.to_string().contains(if split {
            "ErrKeyspaceGroupInSplit"
        } else {
            "ErrKeyspaceGroupInMerging"
        }));
        assert!(m.load_by_id(1).await.is_ok());
        assert!(m.cache.get(1).is_some());
    }
}

#[tokio::test]
async fn mcs_042_revision_conflict_refreshes_membership_without_partial_deletes() {
    let f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let (g, m) = setup(store.clone(), 3, false, 0).await;
    let cancel = CancellationToken::new();
    let mut job = g
        .removal_job(&cancel, 1, &m, None, &[1, 2, 3])
        .await
        .unwrap();
    let prepared = job.prepare_next_batch().await.unwrap().unwrap();
    let mut txn = Base::new(store.as_ref()).begin_txn();
    let latest = KeyspaceGroup {
        id: 1,
        user_kind: "standard".into(),
        keyspaces: Some(vec![1, 2, 3, 9000]),
        ..Default::default()
    };
    StorageEndpoint::new(store.clone())
        .save_keyspace_group(&mut txn, &ClusterPaths::new(42), &latest)
        .unwrap();
    txn.commit::<String>().await.unwrap();
    assert!(matches!(
        prepared.commit().await,
        Err(GroupRemovalError::Conflict)
    ));
    assert_eq!(g.groups.read().await[&UserKind::STANDARD.0][&1], latest);
    for id in 1..=3 {
        assert!(m.cache.get(id).is_none());
        assert!(m.load_by_id(id).await.is_ok());
    }
    assert_eq!(
        counts(store, &BTreeMap::from([("g".into(), String::new())])).await["g"],
        3
    );
}
