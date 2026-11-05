// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{KeyspaceGroup, MetaStore};
use std::sync::Arc;
#[tokio::test]
async fn mcs_032_merge_all_cancellation_retains_committed_batch() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = KeyspaceGroupManager::new(store, ClusterPaths::new(811));
    m.bootstrap().await.unwrap();
    m.create_groups(&[KeyspaceGroup {
        id: 1,
        user_kind: "basic".into(),
        keyspaces: Some(vec![11]),
        ..Default::default()
    }])
    .await
    .unwrap();
    let c = tokio_util::sync::CancellationToken::new();
    c.cancel();
    m.merge_all_into_default(&c).await.unwrap();
    assert!(m.get_group(1).await.unwrap().is_none());
    assert!(m.get_group(0).await.unwrap().unwrap().is_merge_target());
    m.finish_merge(0).await.unwrap();
    assert!(!m.get_group(0).await.unwrap().unwrap().is_merging());
}
#[tokio::test]
async fn mcs_031_legacy_patrol_atomic_assignment_retry_and_once() {
    use astersql_pd_storage_domain::{
        Base, Consistency, Keyspace, KeyspaceIdentity, KeyspaceStorage, Read, StorageEndpoint,
    };
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let p = ClusterPaths::new(831);
    let groups = Arc::new(KeyspaceGroupManager::new(store.clone(), p.clone()));
    groups.bootstrap().await.unwrap();
    let storage = KeyspaceStorage::new(
        store.clone(),
        p.clone(),
        Arc::new(astersql_pd_storage_endpoint::ProtobufKeyspaceCodec),
    );
    let manager = KeyspaceManager::with_resources(
        storage,
        KeyspaceCreateResources {
            groups: Some(groups.clone()),
            ..Default::default()
        },
    );
    manager
        .save_new(&Keyspace {
            identity: Some(KeyspaceIdentity::Id(101)),
            name: "legacy101".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let mut txn = Base::new(store.as_ref()).begin_txn();
    let ep = StorageEndpoint::new(store.clone());
    let mut g = groups.get_group(0).await.unwrap().unwrap();
    g.split_state = Some(astersql_pd_storage_domain::SplitState { split_source: 0 });
    ep.save_keyspace_group(&mut txn, &p, &g).unwrap();
    txn.commit::<std::convert::Infallible>().await.unwrap();
    let c = tokio_util::sync::CancellationToken::new();
    assert!(manager.patrol_group_assignment(0, 0, &c).await.is_err());
    assert!(
        !manager
            .load_by_id(101)
            .await
            .unwrap()
            .config
            .contains_key("tso_keyspace_group_id")
    );
    g.split_state = None;
    let mut txn = Base::new(store.as_ref()).begin_txn();
    ep.save_keyspace_group(&mut txn, &p, &g).unwrap();
    txn.commit::<std::convert::Infallible>().await.unwrap();
    manager.patrol_group_assignment(0, 0, &c).await.unwrap();
    assert_eq!(
        manager.load_by_id(101).await.unwrap().config["tso_keyspace_group_id"],
        "0"
    );
    assert_eq!(
        groups.get_group(0).await.unwrap().unwrap().keyspaces,
        Some(vec![0, 101])
    );
    let row = store
        .read(Read::prefix(
            p.keyspace_meta_path(101).as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .entries
        .remove(0);
    let group = store
        .read(Read::prefix(
            p.keyspace_group_id_path(0).as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .entries
        .remove(0);
    assert_eq!(row.mod_revision, group.mod_revision);
    manager
        .save_new(&Keyspace {
            identity: Some(KeyspaceIdentity::Id(102)),
            name: "legacy102".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    manager.patrol_group_assignment(0, 0, &c).await.unwrap();
    assert!(
        !manager
            .load_by_id(102)
            .await
            .unwrap()
            .config
            .contains_key("tso_keyspace_group_id")
    );
}
#[tokio::test]
async fn mcs_031_patrol_sparse_upper_bound_finishes() {
    use astersql_pd_storage_domain::{Keyspace, KeyspaceIdentity, KeyspaceStorage};
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let p = ClusterPaths::new(832);
    let groups = Arc::new(KeyspaceGroupManager::new(store.clone(), p.clone()));
    groups.bootstrap().await.unwrap();
    let storage = KeyspaceStorage::new(
        store,
        p,
        Arc::new(astersql_pd_storage_endpoint::ProtobufKeyspaceCodec),
    );
    let manager = KeyspaceManager::with_resources(
        storage,
        KeyspaceCreateResources {
            groups: Some(groups.clone()),
            ..Default::default()
        },
    );
    for id in 200..330 {
        manager
            .save_new(&Keyspace {
                identity: Some(KeyspaceIdentity::Id(id)),
                name: format!("sparse{id}"),
                ..Default::default()
            })
            .await
            .unwrap();
    }
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        manager.patrol_group_assignment(1, 100, &tokio_util::sync::CancellationToken::new()),
    )
    .await
    .expect("a full page beyond the requested range must stop the patrol")
    .unwrap();
    assert_eq!(
        groups.get_group(0).await.unwrap().unwrap().keyspaces,
        Some(vec![0])
    );
}
