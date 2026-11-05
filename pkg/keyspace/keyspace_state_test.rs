// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{Keyspace, KeyspaceIdentity};
use std::sync::Arc;
#[tokio::test]
async fn mcs_039_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let manager = KeyspaceManager::new(
        astersql_pd_storage_endpoint::StorageEndpoint::new(store)
            .keyspace_storage(ClusterPaths::new(42)),
    );
    manager
        .save_new(&Keyspace {
            identity: Some(KeyspaceIdentity::Id(39)),
            name: "state39".into(),
            state: 0,
            created_at: 100,
            state_changed_at: 100,
            ..Default::default()
        })
        .await
        .unwrap();
    let updated = manager.update_state_by_id(39, 1, 200).await.unwrap();
    assert_eq!(
        updated.state, 1,
        "state transition must update persisted metadata"
    );
    assert_eq!(updated.state_changed_at, 200);
    assert_eq!(manager.load_by_name("state39").await.unwrap(), updated);
}
use astersql_pd_storage_domain::{
    Base, Consistency, KeyspaceCodec, MetaServiceGroupStatus, MetaStore, Mutation, Read,
    StorageEndpoint,
};
use std::collections::{BTreeMap, HashMap};
fn item(id: u32, state: i32) -> Keyspace {
    Keyspace {
        identity: Some(KeyspaceIdentity::Id(id)),
        name: format!("state{id}"),
        state,
        created_at: 100,
        state_changed_at: 100,
        ..Default::default()
    }
}
#[tokio::test]
async fn mcs_039_complete_transition_matrix_and_idempotent_times() {
    let f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = KeyspaceManager::new(
        astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone())
            .keyspace_storage(ClusterPaths::new(42)),
    );
    for (i, from) in [0, 1, 2, 3, 99].into_iter().enumerate() {
        for (j, to) in [0, 1, 2, 3, 99].into_iter().enumerate() {
            let id = 100 + (i * 5 + j) as u32;
            let original = item(id, from);
            m.save_new(&original).await.unwrap();
            let allowed = from == to || [(0, 1), (1, 0), (1, 2), (2, 3)].contains(&(from, to));
            let result = if j % 2 == 0 {
                m.update_state_by_id(id, to, 999).await
            } else {
                m.update_state_by_name(&original.name, to, 999).await
            };
            if allowed {
                let value = result.unwrap();
                assert_eq!(value.state, to);
                assert_eq!(value.state_changed_at, if from == to { 100 } else { 999 });
                assert_eq!(m.load_by_id(id).await.unwrap(), value);
                assert_eq!(m.cache.keyspace_exist(id), to != 3);
            } else {
                assert_eq!(
                    result.unwrap_err().to_string(),
                    format!(
                        "cannot change keyspace state from {} to {}",
                        crate::keyspace_config::state_name(from),
                        crate::keyspace_config::state_name(to)
                    )
                );
                assert_eq!(m.load_by_id(id).await.unwrap(), original);
            }
        }
    }
    assert!(matches!(
        m.update_state_by_id(500, 1, 0).await,
        Err(KeyspaceError::NotFound)
    ));
    assert!(matches!(
        m.update_state_by_name("missing", 1, 0).await,
        Err(KeyspaceError::NotFound)
    ));
}
#[tokio::test]
async fn mcs_039_kernel_protection_precedes_storage_and_preserves_reserved_records() {
    let mut f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let mut storage = astersql_pd_storage_endpoint::StorageEndpoint::new(store)
        .keyspace_storage(ClusterPaths::new(42));
    for (kernel, id, name) in [
        (Kernel::Classic, 0, "DEFAULT"),
        (
            Kernel::NextGeneration,
            astersql_pd_keyspace_constant::SYSTEM_KEYSPACE_ID,
            "SYSTEM",
        ),
    ] {
        let m = KeyspaceManager::with_resources(
            storage.clone(),
            KeyspaceCreateResources {
                kernel,
                ..Default::default()
            },
        );
        let mut reserved = item(id, 0);
        reserved.name = name.into();
        m.save_new(&reserved).await.unwrap();
        f.command("stop", "stopped");
        let expected = crate::keyspace_state::protected_state_error(kernel).to_string();
        assert_eq!(
            m.update_state_by_id(id, 1, 200)
                .await
                .unwrap_err()
                .to_string(),
            expected
        );
        assert_eq!(
            m.update_state_by_name(name, 1, 200)
                .await
                .unwrap_err()
                .to_string(),
            expected
        );
        f.command("start", "restarted");
        storage = astersql_pd_storage_endpoint::StorageEndpoint::new(Arc::new(f.store().await))
            .keyspace_storage(ClusterPaths::new(42));
        assert_eq!(
            KeyspaceManager::new(storage.clone())
                .load_by_id(id)
                .await
                .unwrap(),
            reserved
        );
    }
}
#[tokio::test]
async fn mcs_039_tombstone_unassigns_once_and_repairs_legacy_binding_on_same_state() {
    let mut f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let p = ClusterPaths::new(42);
    let mgm = Arc::new(MetaServiceGroupManager::new(
        store.clone(),
        p.clone(),
        BTreeMap::from([("g".into(), "127.0.0.1:10001".into())]),
    ));
    let m = KeyspaceManager::with_resources(
        astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone())
            .keyspace_storage(p.clone()),
        KeyspaceCreateResources {
            meta_groups: Some(mgm),
            ..Default::default()
        },
    );
    let mut original = item(39, 2);
    original
        .config
        .insert("meta_service_group_id".into(), "g".into());
    m.save_new(&original).await.unwrap();
    let mut txn = Base::new(store.as_ref()).begin_txn();
    let ep = StorageEndpoint::new(store.clone());
    ep.save_meta_service_group_status(
        &mut txn,
        &p,
        "g",
        Some(&MetaServiceGroupStatus {
            enabled: true,
            assignment_count: 1,
        }),
    )
    .unwrap();
    txn.commit::<String>().await.unwrap();
    let dead = m.update_state_by_name("state39", 3, 200).await.unwrap();
    assert!(!dead.config.contains_key("meta_service_group_id"));
    assert_eq!(dead.state_changed_at, 200);
    assert!(!m.cache.keyspace_exist(39));
    let mut all = vec![];
    m.cache.scan_all(|id, _| {
        all.push(id);
        true
    });
    assert_eq!(all, [39]);
    let mut repaired = dead.clone();
    repaired
        .config
        .insert("meta_service_group_id".into(), "g".into());
    let mut txn = Base::new(store.as_ref()).begin_txn();
    m.storage
        .save_keyspace_meta(&mut txn, &p, &repaired)
        .unwrap();
    ep.save_meta_service_group_status(
        &mut txn,
        &p,
        "g",
        Some(&MetaServiceGroupStatus {
            enabled: true,
            assignment_count: 1,
        }),
    )
    .unwrap();
    txn.commit::<String>().await.unwrap();
    assert_eq!(m.update_state_by_id(39, 3, 999).await.unwrap(), dead);
    assert_eq!(m.update_state_by_id(39, 3, 1000).await.unwrap(), dead);
    let mut txn = Base::new(store.as_ref()).begin_txn();
    let statuses = ep
        .load_meta_service_group_status(
            &mut txn,
            &p,
            &BTreeMap::from([("g".into(), String::new())]),
        )
        .await
        .unwrap();
    assert_eq!(statuses["g"].assignment_count, 0);
    f.command("stop", "stopped");
    f.command("start", "restarted");
    let recovered = KeyspaceManager::new(
        astersql_pd_storage_endpoint::StorageEndpoint::new(Arc::new(f.store().await))
            .keyspace_storage(p),
    );
    assert_eq!(recovered.load_by_id(39).await.unwrap(), dead);
}
#[tokio::test]
async fn mcs_039_actual_commit_size_rejection_retains_go_cache_publication_point() {
    let f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = KeyspaceManager::new(
        astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone())
            .keyspace_storage(ClusterPaths::new(42)),
    );
    let initial = item(39, 1);
    m.save_new(&initial).await.unwrap();
    let key = ClusterPaths::new(42).keyspace_meta_path(39);
    let codec = astersql_pd_storage_endpoint::ProtobufKeyspaceCodec;
    let mut low = 0;
    let mut high = 2 * 1024 * 1024;
    while low + 1 < high {
        let mid = (low + high) / 2;
        let mut meta = initial.clone();
        meta.config = HashMap::from([("filler".into(), "x".repeat(mid))]);
        let bytes = codec.encode(&meta);
        match store
            .transact(vec![], vec![Mutation::put(key.as_bytes(), &bytes)])
            .await
        {
            Ok(_) => low = mid,
            Err(error) => {
                assert!(matches!(
                    error,
                    astersql_pd_storage_domain::StoreError::RequestTooLarge
                        | astersql_pd_storage_domain::StoreError::Backend
                        | astersql_pd_storage_domain::StoreError::InvalidArgument
                ));
                high = mid;
            }
        }
    }
    assert!(
        low > 1024 * 1024,
        "must exercise the actual etcd request boundary"
    );
    let mut meta = initial.clone();
    meta.config.insert("filler".into(), "x".repeat(low));
    store
        .transact(
            vec![],
            vec![Mutation::put(key.as_bytes(), &codec.encode(&meta))],
        )
        .await
        .unwrap();
    let before = store
        .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
        .await
        .unwrap();
    let error = m.update_state_by_id(39, 0, i64::MAX).await.unwrap_err();
    println!("actual etcd state commit rejected at filler={low}: {error}");
    assert_eq!(
        before.entries,
        store
            .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
    );
    assert_eq!(m.load_by_id(39).await.unwrap().state, 1);
    assert_eq!(m.cache.get(39).unwrap().state, KeyspaceState::Enabled);
}
#[tokio::test]
async fn mcs_039_backend_failure_before_load_preserves_cache_and_wal_state() {
    let mut f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = KeyspaceManager::new(
        astersql_pd_storage_endpoint::StorageEndpoint::new(store)
            .keyspace_storage(ClusterPaths::new(42)),
    );
    let original = item(39, 1);
    m.save_new(&original).await.unwrap();
    f.command("stop", "stopped");
    assert!(m.update_state_by_id(39, 0, 200).await.is_err());
    assert_eq!(m.cache.get(39).unwrap().state, KeyspaceState::Disabled);
    f.command("start", "restarted");
    let recovered = KeyspaceManager::new(
        astersql_pd_storage_endpoint::StorageEndpoint::new(Arc::new(f.store().await))
            .keyspace_storage(ClusterPaths::new(42)),
    );
    assert_eq!(recovered.load_by_id(39).await.unwrap(), original);
}
