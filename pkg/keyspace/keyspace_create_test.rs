// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{Base, Keyspace, KeyspaceIdentity};
use std::sync::Arc;
#[tokio::test]
async fn mcs_037_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let adapter = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
    let paths = ClusterPaths::new(42);
    let storage = adapter.keyspace_storage(paths.clone());
    let manager = KeyspaceManager::new(storage.clone());
    let meta = Keyspace {
        identity: Some(KeyspaceIdentity::Id(37)),
        name: "space37".into(),
        state: 1,
        created_at: 100,
        state_changed_at: 100,
        ..Default::default()
    };
    manager.save_new(&meta).await.unwrap();
    let mut txn = Base::new(store.as_ref()).begin_txn();
    assert_eq!(
        storage
            .load_keyspace_id(&mut txn, &paths, "space37")
            .await
            .unwrap(),
        Some(37),
        "name and ID indexes must commit together"
    );
    assert_eq!(
        storage
            .load_keyspace_meta(&mut txn, &paths, 37)
            .await
            .unwrap(),
        Some(meta)
    );
    assert!(manager.cache.keyspace_exist(37));
}

use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_storage_domain::{Consistency, MetaStore, Read, StorageEndpoint};
use std::{
    collections::{BTreeMap, HashMap},
    sync::RwLock,
    time::Duration,
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
fn meta(id: u32, name: &str) -> Keyspace {
    Keyspace {
        identity: Some(KeyspaceIdentity::Id(id)),
        name: name.into(),
        state: 1,
        created_at: 100,
        state_changed_at: 100,
        ..Default::default()
    }
}
async fn configured(
    store: Arc<dyn MetaStore>,
    kernel: Kernel,
    wait: bool,
    groups: Option<Arc<KeyspaceGroupManager>>,
    meta_groups: Option<Arc<MetaServiceGroupManager>>,
) -> (
    Arc<KeyspaceManager>,
    Arc<Mutex<RegionLabeler>>,
    astersql_pd_core::RegionsInfo,
) {
    let paths = ClusterPaths::new(42);
    let labeler = Arc::new(Mutex::new(
        RegionLabeler::load(StorageEndpoint::new(store.clone()), paths.clone(), 0)
            .await
            .unwrap(),
    ));
    let regions = astersql_pd_core::RegionsInfo::new();
    let splitter = Arc::new(KeyspaceRegionSplit::new(
        Some(labeler.clone()),
        Some(regions.clone()),
        Arc::new(RwLock::new(KeyspaceSplitConfig {
            interval: Duration::from_millis(10),
            timeout: Duration::from_millis(150),
        })),
    ));
    let storage = astersql_pd_storage_endpoint::StorageEndpoint::new(store).keyspace_storage(paths);
    (
        Arc::new(KeyspaceManager::with_resources(
            storage,
            KeyspaceCreateResources {
                kernel,
                splitter: Some(splitter),
                groups,
                meta_groups,
                config: Arc::new(RwLock::new(KeyspaceCreateConfig {
                    wait_region_split: wait,
                    bound: BoundType::Txn,
                })),
                ..Default::default()
            },
        )),
        labeler,
        regions,
    )
}
fn request(id: u32, name: &str) -> CreateKeyspaceByIdRequest {
    CreateKeyspaceByIdRequest {
        id: Some(id),
        name: name.into(),
        create_time: 100,
        ..Default::default()
    }
}
#[tokio::test]
async fn mcs_037_indexes_conflicts_pagination_and_corrupt_reads() {
    let f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let storage = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone())
        .keyspace_storage(ClusterPaths::new(42));
    let m = KeyspaceManager::new(storage.clone());
    for id in [37, 40, 42] {
        m.save_new(&meta(id, &format!("space{id}"))).await.unwrap();
    }
    for item in [meta(50, "space37"), meta(37, "different")] {
        assert!(matches!(
            m.save_new(&item).await,
            Err(KeyspaceError::Exists)
        ));
    }
    assert!(matches!(
        m.load_by_name("different").await,
        Err(KeyspaceError::NotFound)
    ));
    assert!(matches!(
        m.load_by_id(50).await,
        Err(KeyspaceError::NotFound)
    ));
    assert_eq!(
        m.load_by_name("space37").await.unwrap(),
        m.load_by_id(37).await.unwrap()
    );
    assert_eq!(
        m.load_range(38, 1)
            .await
            .unwrap()
            .iter()
            .map(Keyspace::id)
            .collect::<Vec<_>>(),
        [40]
    );
    assert_eq!(
        m.load_range(40, 0)
            .await
            .unwrap()
            .iter()
            .map(Keyspace::id)
            .collect::<Vec<_>>(),
        [40, 42]
    );
    assert!(m.load_range(43, 0).await.unwrap().is_empty());
    assert_eq!(
        m.load_range(0x1000000, 0).await.unwrap_err().to_string(),
        "startID of the scan 16777216 exceeds spaceID Max 16777215"
    );
    let p = ClusterPaths::new(42);
    Base::new(store.as_ref())
        .save(p.keyspace_id_path("orphan").as_bytes(), b"99")
        .await
        .unwrap();
    assert!(matches!(
        m.load_by_name("orphan").await,
        Err(KeyspaceError::NotFound)
    ));
    Base::new(store.as_ref())
        .save(p.keyspace_meta_path(99).as_bytes(), b"\xff")
        .await
        .unwrap();
    assert!(matches!(
        m.load_by_id(99).await,
        Err(KeyspaceError::Storage(_))
    ));
    let mut revisions = vec![];
    for key in [p.keyspace_id_path("space37"), p.keyspace_meta_path(37)] {
        let s = store
            .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
            .await
            .unwrap();
        revisions.push(s.entries[0].create_revision);
    }
    assert_eq!(revisions[0], revisions[1]);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_037_independent_managers_cannot_publish_partial_indexes() {
    let f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let storage = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone())
        .keyspace_storage(ClusterPaths::new(42));
    for same_name in [true, false] {
        let a = Arc::new(KeyspaceManager::new(storage.clone()));
        let b = Arc::new(KeyspaceManager::new(storage.clone()));
        let first = if same_name {
            meta(100, "race")
        } else {
            meta(200, "race_a")
        };
        let second = if same_name {
            meta(101, "race")
        } else {
            meta(200, "race_b")
        };
        let c = a.clone();
        let d = b.clone();
        let l = first.clone();
        let r = second.clone();
        let (x, y) = tokio::join!(
            tokio::spawn(async move { c.save_new(&l).await }),
            tokio::spawn(async move { d.save_new(&r).await })
        );
        let x = x.unwrap();
        let y = y.unwrap();
        assert_ne!(x.is_ok(), y.is_ok());
        let winner = if x.is_ok() { first } else { second };
        assert_eq!(a.load_by_name(&winner.name).await.unwrap(), winner);
        assert_eq!(a.load_by_id(winner.id()).await.unwrap(), winner);
        assert_eq!(a.cache.keyspace_exist(winner.id()), x.is_ok());
        assert_eq!(b.cache.keyspace_exist(winner.id()), y.is_ok());
    }
}
#[tokio::test]
async fn mcs_037_creation_config_group_and_nextgen_defaults() {
    let f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let groups = Arc::new(KeyspaceGroupManager::new(
        store.clone(),
        ClusterPaths::new(42),
    ));
    groups.bootstrap().await.unwrap();
    enable_group(store.clone(), "g").await;
    let mgm = Arc::new(MetaServiceGroupManager::new(
        store.clone(),
        ClusterPaths::new(42),
        BTreeMap::from([("g".into(), "127.0.0.1:11000".into())]),
    ));
    let (m, labeler, _) = configured(
        store.clone(),
        Kernel::NextGeneration,
        false,
        Some(groups.clone()),
        Some(mgm.clone()),
    )
    .await;
    let mut req = request(50, "created");
    req.config = HashMap::from([
        ("meta_service_group_addrs".into(), "forged".into()),
        ("meta_service_group_id".into(), "forged".into()),
        ("tso_keyspace_group_id".into(), "999".into()),
        ("keep".into(), "value".into()),
    ]);
    let result = m
        .create_by_id(&CancellationToken::new(), req)
        .await
        .unwrap();
    assert_eq!(result.state, 0);
    assert_eq!(result.config["keep"], "value");
    assert_eq!(result.config["gc_management_type"], "keyspace_level");
    assert_eq!(result.config["tso_keyspace_group_id"], "0");
    assert_eq!(result.config["user_kind"], "basic");
    assert_eq!(result.config["region_bound_type"], "txn");
    assert_eq!(result.config["meta_service_group_id"], "g");
    assert_eq!(result.config["meta_service_group_addrs"], "127.0.0.1:11000");
    let mut txn = Base::new(store.as_ref()).begin_txn();
    let raw = m
        .storage
        .load_keyspace_meta(&mut txn, &m.storage.paths, 50)
        .await
        .unwrap()
        .unwrap();
    assert!(!raw.config.contains_key("meta_service_group_addrs"));
    assert_eq!(m.load_by_name("created").await.unwrap(), result);
    assert_eq!(
        groups.get_group(0).await.unwrap().unwrap().keyspaces,
        Some(vec![0, 50])
    );
    assert!(labeler.lock().await.index().rule("keyspaces/50").is_some());
    let statuses = StorageEndpoint::new(store.clone())
        .load_meta_service_group_status(&mut txn, &m.storage.paths, &mgm.get_groups().await)
        .await
        .unwrap();
    assert_eq!(statuses["g"].assignment_count, 1);
    let (classic, _, _) = configured(store.clone(), Kernel::Classic, false, None, None).await;
    let mut req = request(51, "classic");
    req.config
        .insert("meta_service_group_addrs".into(), "forged".into());
    let value = classic
        .create_by_id(&CancellationToken::new(), req)
        .await
        .unwrap();
    assert!(value.config.is_empty());
}
#[tokio::test]
async fn mcs_037_split_failure_rolls_back_indexes_and_assignment() {
    let f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    enable_group(store.clone(), "g").await;
    let mgm = Arc::new(MetaServiceGroupManager::new(
        store.clone(),
        ClusterPaths::new(42),
        BTreeMap::from([("g".into(), "127.0.0.1:11000".into())]),
    ));
    let (m, labeler, _) = configured(
        store.clone(),
        Kernel::Classic,
        true,
        None,
        Some(mgm.clone()),
    )
    .await;
    let error = m
        .create_by_id(&CancellationToken::new(), request(60, "timeout"))
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "[PD:keyspace:ErrRegionSplitTimeout]region split timeout"
    );
    assert!(matches!(
        error,
        KeyspaceError::Split {
            cleanup_error: None,
            ..
        }
    ));
    assert!(matches!(
        m.load_by_name("timeout").await,
        Err(KeyspaceError::NotFound)
    ));
    assert!(matches!(
        m.load_by_id(60).await,
        Err(KeyspaceError::NotFound)
    ));
    assert!(labeler.lock().await.index().rule("keyspaces/60").is_none());
    // Go's creation failure removes storage but leaves the previously published disabled cache item.
    assert_eq!(m.cache.get(60).unwrap().state, KeyspaceState::Disabled);
    let mut txn = Base::new(store.as_ref()).begin_txn();
    let statuses = StorageEndpoint::new(store.clone())
        .load_meta_service_group_status(&mut txn, &m.storage.paths, &mgm.get_groups().await)
        .await
        .unwrap();
    assert_eq!(statuses["g"].assignment_count, 0);
}
#[tokio::test]
async fn mcs_037_actual_save_failure_and_same_wal_recovery() {
    let mut f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let (m, labeler, _) = configured(store.clone(), Kernel::Classic, false, None, None).await;
    let mut req = request(70, "large");
    req.config
        .insert("large".into(), "x".repeat(2 * 1024 * 1024));
    assert!(
        m.create_by_id(&CancellationToken::new(), req)
            .await
            .is_err()
    );
    assert!(m.cache.get(70).is_none());
    assert!(labeler.lock().await.index().rule("keyspaces/70").is_none());
    assert!(matches!(
        m.load_by_name("large").await,
        Err(KeyspaceError::NotFound)
    ));
    let saved = m
        .create_by_id(&CancellationToken::new(), request(70, "large"))
        .await
        .unwrap();
    f.command("stop", "stopped");
    assert!(m.load_by_id(70).await.is_err());
    f.command("start", "restarted");
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let (restored, labeler, _) = configured(store, Kernel::Classic, false, None, None).await;
    assert_eq!(restored.load_by_name("large").await.unwrap(), saved);
    assert!(labeler.lock().await.index().rule("keyspaces/70").is_some());
}

async fn enable_group(store: Arc<dyn MetaStore>, id: &str) {
    let mut txn = Base::new(store.as_ref()).begin_txn();
    StorageEndpoint::new(store.clone())
        .save_meta_service_group_status(
            &mut txn,
            &ClusterPaths::new(42),
            id,
            Some(&astersql_pd_storage_domain::MetaServiceGroupStatus {
                enabled: true,
                assignment_count: 0,
            }),
        )
        .unwrap();
    txn.commit::<String>().await.unwrap();
}
#[tokio::test]
async fn mcs_037_real_allocator_duplicate_name_does_not_consume_id() {
    use astersql_pd_id::{Allocator, AllocatorNamespace};
    let f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let member = astersql_pd_member::Member::bootstrap(
        store.clone(),
        CancellationToken::new(),
        42,
        1,
        b"allocator37".to_vec(),
    )
    .await
    .unwrap();
    let leader = Arc::new(member.campaign(5).await.unwrap());
    let allocator = Arc::new(
        Allocator::with_namespace(
            store.clone(),
            leader.clone(),
            CancellationToken::new(),
            100,
            AllocatorNamespace::Keyspace,
        )
        .unwrap(),
    );
    let (m, _, _) = configured(store, Kernel::Classic, false, None, None).await;
    let mut m = Arc::try_unwrap(m).ok().unwrap();
    m.resources.allocator = Some(allocator.clone());
    let a = m
        .create(
            &CancellationToken::new(),
            CreateKeyspaceRequest {
                name: "allocated".into(),
                create_time: 100,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(a.id(), 1);
    assert!(matches!(
        m.create(
            &CancellationToken::new(),
            CreateKeyspaceRequest {
                name: "allocated".into(),
                ..Default::default()
            }
        )
        .await,
        Err(KeyspaceError::Exists)
    ));
    assert_eq!(allocator.alloc_count(0).await.unwrap().0, 1);
    let b = m
        .create(
            &CancellationToken::new(),
            CreateKeyspaceRequest {
                name: "second".into(),
                create_time: 101,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(b.id(), 2);
    let error = m
        .create_by_id(
            &CancellationToken::new(),
            CreateKeyspaceByIdRequest::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "keyspace id is empty");
    let error = m
        .create_by_id(&CancellationToken::new(), request(99, ""))
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "keyspace name is empty");
    leader.resign().await.unwrap();
}
#[tokio::test]
async fn mcs_037_disabled_metadata_visible_during_real_boundary_wait() {
    use astersql_pd_core::{KeyRange, RegionInfo, RegionMeta};
    let f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let (m, labeler, mut regions) = configured(store, Kernel::Classic, true, None, None).await;
    let worker = m.clone();
    let task = tokio::spawn(async move {
        worker
            .create_by_id(&CancellationToken::new(), request(90, "waiting"))
            .await
    });
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if labeler.lock().await.index().rule("keyspaces/90").is_some() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(m.load_by_name("waiting").await.unwrap().state, 1);
    let b = make_region_bound(90);
    let (left, right) = BoundType::Txn.bounds(&b);
    for (id, start, end) in [
        (1, left.to_vec(), right.to_vec()),
        (2, right.to_vec(), vec![]),
    ] {
        regions.put_region(Arc::new(RegionInfo::new(
            RegionMeta {
                id,
                key_range: KeyRange {
                    start_key: start,
                    end_key: end,
                },
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        )));
    }
    let result = task.await.unwrap().unwrap();
    assert_eq!(result.state, 0);
    assert_eq!(m.load_by_id(90).await.unwrap(), result);
    assert_eq!(m.cache.get(90).unwrap().state, KeyspaceState::Enabled);
}
#[tokio::test]
async fn mcs_037_group_disappearing_after_split_leaves_enabled_metadata() {
    use astersql_pd_core::{KeyRange, RegionInfo, RegionMeta};
    let f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let groups = Arc::new(KeyspaceGroupManager::new(
        store.clone(),
        ClusterPaths::new(42),
    ));
    groups
        .create_groups(&[astersql_pd_storage_domain::KeyspaceGroup {
            id: 1,
            user_kind: "standard".into(),
            ..Default::default()
        }])
        .await
        .unwrap();
    let (m, labeler, mut regions) =
        configured(store, Kernel::Classic, true, Some(groups.clone()), None).await;
    let mut req = request(91, "group_race");
    req.config.insert("user_kind".into(), "standard".into());
    let worker = m.clone();
    let task =
        tokio::spawn(async move { worker.create_by_id(&CancellationToken::new(), req).await });
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if labeler.lock().await.index().rule("keyspaces/91").is_some() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    groups.delete_group(1).await.unwrap();
    let b = make_region_bound(91);
    let (left, right) = BoundType::Txn.bounds(&b);
    for (id, start, end) in [
        (1, left.to_vec(), right.to_vec()),
        (2, right.to_vec(), vec![]),
    ] {
        regions.put_region(Arc::new(RegionInfo::new(
            RegionMeta {
                id,
                key_range: KeyRange {
                    start_key: start,
                    end_key: end,
                },
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        )));
    }
    let error = task.await.unwrap().unwrap_err();
    assert_eq!(
        error.to_string(),
        "[PD:keyspace:ErrKeyspaceGroupNotExists]keyspace group 1 does not exist"
    );
    assert_eq!(m.load_by_name("group_race").await.unwrap().state, 0);
    assert!(labeler.lock().await.index().rule("keyspaces/91").is_some());
}
