// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_storage_domain::{MetaStore, StorageEndpoint};
use std::{
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::sync::Mutex;
async fn manager(
    store: Arc<dyn MetaStore>,
    kernel: Kernel,
    groups: Option<Arc<KeyspaceGroupManager>>,
) -> KeyspaceManager {
    let p = ClusterPaths::new(42);
    let labeler = Arc::new(Mutex::new(
        RegionLabeler::load(StorageEndpoint::new(store.clone()), p.clone(), 0)
            .await
            .unwrap(),
    ));
    let split = Arc::new(KeyspaceRegionSplit::new(
        Some(labeler),
        None,
        Arc::new(RwLock::new(KeyspaceSplitConfig {
            interval: Duration::from_millis(1),
            timeout: Duration::from_millis(2),
        })),
    ));
    KeyspaceManager::with_resources(
        astersql_pd_storage_endpoint::StorageEndpoint::new(store).keyspace_storage(p),
        KeyspaceCreateResources {
            kernel,
            splitter: Some(split),
            groups,
            ..Default::default()
        },
    )
}
#[tokio::test]
async fn mcs_041_behavior() {
    let f = crate::etcd_fixture::Fixture::start();
    let m = manager(Arc::new(f.store().await), Kernel::Classic, None).await;
    m.bootstrap(100).await.unwrap();
    let reserved = m
        .load_by_id(0)
        .await
        .expect("bootstrap must persist the protected keyspace and its name index");
    assert_eq!(reserved.name, "DEFAULT");
    assert_eq!(reserved.state, 0);
    assert_eq!(reserved.created_at, 100);
    assert_eq!(m.load_by_name("DEFAULT").await.unwrap(), reserved);
}
use astersql_pd_storage_domain::{Base, Consistency, Read, SplitState};
use tokio_util::sync::CancellationToken;
#[tokio::test]
async fn mcs_041_both_kernels_repeated_bootstrap_preserves_metadata_and_refreshes_rule() {
    for kernel in [Kernel::Classic, Kernel::NextGeneration] {
        let f = crate::etcd_fixture::Fixture::start();
        let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
        let groups = Arc::new(KeyspaceGroupManager::new(
            store.clone(),
            ClusterPaths::new(42),
        ));
        groups.bootstrap_for_kernel(kernel).await.unwrap();
        let m = manager(store.clone(), kernel, Some(groups.clone())).await;
        let (id, name) = kernel.bootstrap_identity();
        m.bootstrap(100).await.unwrap();
        let original = m.load_by_id(id).await.unwrap();
        assert_eq!(original.name, name);
        assert_eq!(original.config["tso_keyspace_group_id"], "0");
        assert_eq!(original.config["user_kind"], "basic");
        assert_eq!(original.config["region_bound_type"], "txn");
        assert_eq!(
            original
                .config
                .get("gc_management_type")
                .map(String::as_str),
            if kernel == Kernel::NextGeneration {
                Some("keyspace_level")
            } else {
                None
            }
        );
        assert_eq!(
            groups.get_group(0).await.unwrap().unwrap().keyspaces,
            Some(vec![id])
        );
        let p = ClusterPaths::new(42);
        let before = store
            .read(Read::range(
                p.keyspace_meta_path(id).as_bytes(),
                b"",
                Consistency::Linearizable,
            ))
            .await
            .unwrap();
        m.resources.config.write().unwrap().bound = BoundType::Raw;
        m.bootstrap(999).await.unwrap();
        assert_eq!(m.load_by_id(id).await.unwrap(), original);
        assert_eq!(
            before.entries,
            store
                .read(Read::range(
                    p.keyspace_meta_path(id).as_bytes(),
                    b"",
                    Consistency::Linearizable
                ))
                .await
                .unwrap()
                .entries
        );
        let rule = store
            .read(Read::range(
                p.region_label_key_path(&format!("keyspaces/{id}"))
                    .as_bytes(),
                b"",
                Consistency::Linearizable,
            ))
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&rule.entries[0].value).unwrap(),
            serde_json::to_value(build_label_rule(id, BoundType::Raw)).unwrap()
        );
        let cold = manager(store, kernel, Some(groups)).await;
        cold.bootstrap(2000).await.unwrap();
        assert!(
            cold.cache.get(id).is_none(),
            "Go ignores Exists without eagerly warming existing metadata cache"
        );
        assert_eq!(cold.load_by_id(id).await.unwrap(), original);
    }
}
#[tokio::test]
async fn mcs_041_real_failure_after_rule_before_indexes_and_retry_same_wal() {
    let mut f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let groups = Arc::new(KeyspaceGroupManager::new(
        store.clone(),
        ClusterPaths::new(42),
    ));
    groups.bootstrap().await.unwrap();
    let m = Arc::new(manager(store.clone(), Kernel::Classic, Some(groups.clone())).await);
    let held = groups.groups.write().await;
    let worker = m.clone();
    let task = tokio::spawn(async move { worker.bootstrap(100).await });
    let p = ClusterPaths::new(42);
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if !store
                .read(Read::range(
                    p.region_label_key_path("keyspaces/0").as_bytes(),
                    b"",
                    Consistency::Linearizable,
                ))
                .await
                .unwrap()
                .entries
                .is_empty()
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    f.command("stop", "stopped");
    drop(held);
    assert!(task.await.unwrap().is_err());
    assert!(m.cache.get(0).is_none());
    f.command("start", "restarted");
    let fresh: Arc<dyn MetaStore> = Arc::new(f.store().await);
    for key in [p.keyspace_id_path("DEFAULT"), p.keyspace_meta_path(0)] {
        assert!(
            fresh
                .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                .await
                .unwrap()
                .entries
                .is_empty()
        );
    }
    let groups = Arc::new(KeyspaceGroupManager::new(fresh.clone(), p.clone()));
    groups.bootstrap().await.unwrap();
    let retry = manager(fresh.clone(), Kernel::Classic, Some(groups)).await;
    retry.bootstrap(200).await.unwrap();
    assert_eq!(retry.load_by_name("DEFAULT").await.unwrap().id(), 0);
    let name = fresh
        .read(Read::range(
            p.keyspace_id_path("DEFAULT").as_bytes(),
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    let meta = fresh
        .read(Read::range(
            p.keyspace_meta_path(0).as_bytes(),
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert_eq!(
        name.entries[0].create_revision,
        meta.entries[0].create_revision
    );
}
#[tokio::test]
async fn mcs_041_preallocations_are_host_owned_real_allocator_jobs() {
    use astersql_pd_id::{Allocator, AllocatorNamespace};
    for kernel in [Kernel::Classic, Kernel::NextGeneration] {
        let f = crate::etcd_fixture::Fixture::start();
        let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
        let member = astersql_pd_member::Member::bootstrap(
            store.clone(),
            CancellationToken::new(),
            42,
            1,
            b"prealloc41".to_vec(),
        )
        .await
        .unwrap();
        let leader = Arc::new(member.campaign(120).await.unwrap());
        let namespace = if kernel == Kernel::Classic {
            AllocatorNamespace::Keyspace
        } else {
            AllocatorNamespace::KeyspaceNextGen
        };
        let allocator = Arc::new(
            Allocator::with_namespace(
                store.clone(),
                leader.clone(),
                CancellationToken::new(),
                100,
                namespace,
            )
            .unwrap(),
        );
        let groups = Arc::new(KeyspaceGroupManager::new(
            store.clone(),
            ClusterPaths::new(42),
        ));
        groups.bootstrap_for_kernel(kernel).await.unwrap();
        let mut m = manager(store, kernel, Some(groups.clone())).await;
        m.resources.allocator = Some(allocator.clone());
        m.resources.config.write().unwrap().wait_region_split = false;
        let names = vec![
            "pre_one".into(),
            "pre_two".into(),
            "pre_one".into(),
            "bad.name".into(),
        ];
        let work = m.bootstrap_with_preallocations(100, &names).await.unwrap();
        assert_eq!(work.preallocations, names);
        assert!(matches!(
            m.load_by_name("pre_one").await,
            Err(KeyspaceError::NotFound)
        ));
        assert_eq!(allocator.alloc_count(0).await.unwrap().0, 0);
        let first = m
            .preallocate(&CancellationToken::new(), &work.preallocations[0], 101)
            .await
            .unwrap();
        assert!(matches!(first,PreallocationOutcome::Created(ref meta)if meta.id()==1));
        assert!(
            matches!(m.preallocate(&CancellationToken::new(),&work.preallocations[1],102).await.unwrap(),PreallocationOutcome::Created(ref meta)if meta.id()==2)
        );
        assert!(matches!(
            m.preallocate(&CancellationToken::new(), &work.preallocations[2], 103)
                .await
                .unwrap(),
            PreallocationOutcome::AlreadyExists
        ));
        assert!(
            m.preallocate(&CancellationToken::new(), &work.preallocations[3], 104)
                .await
                .is_err()
        );
        assert_eq!(allocator.alloc_count(0).await.unwrap().0, 2);
        let mut expected = vec![kernel.bootstrap_identity().0, 1, 2];
        if kernel == Kernel::NextGeneration {
            expected.push(0);
            assert!(matches!(
                m.load_by_id(0).await,
                Err(KeyspaceError::NotFound)
            ));
        }
        assert_eq!(
            groups.get_group(0).await.unwrap().unwrap().keyspaces,
            Some(expected)
        );
        leader.resign().await.unwrap();
    }
}
#[tokio::test]
async fn mcs_041_group_failure_after_save_keeps_whole_indexes_and_can_retry() {
    let f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let p = ClusterPaths::new(42);
    let groups = Arc::new(KeyspaceGroupManager::new(store.clone(), p.clone()));
    groups.bootstrap().await.unwrap();
    let m = Arc::new(manager(store.clone(), Kernel::Classic, Some(groups.clone())).await);
    let held = groups.membership_mutex.lock().await;
    let worker = m.clone();
    let task = tokio::spawn(async move { worker.bootstrap(100).await });
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if m.load_by_id(0).await.is_ok() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let mut group = groups.get_group(0).await.unwrap().unwrap();
    group.split_state = Some(SplitState { split_source: 0 });
    let mut txn = Base::new(store.as_ref()).begin_txn();
    StorageEndpoint::new(store.clone())
        .save_keyspace_group(&mut txn, &p, &group)
        .unwrap();
    txn.commit::<String>().await.unwrap();
    drop(held);
    assert!(
        task.await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("ErrKeyspaceGroupInSplit")
    );
    assert_eq!(m.load_by_name("DEFAULT").await.unwrap().created_at, 100);
    group.split_state = None;
    let mut txn = Base::new(store.as_ref()).begin_txn();
    StorageEndpoint::new(store.clone())
        .save_keyspace_group(&mut txn, &p, &group)
        .unwrap();
    txn.commit::<String>().await.unwrap();
    m.bootstrap(999).await.unwrap();
    assert_eq!(m.load_by_name("DEFAULT").await.unwrap().created_at, 100);
}
