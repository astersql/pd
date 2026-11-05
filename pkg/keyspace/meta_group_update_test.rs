// Copyright 2026 AsterSQL.
use super::*;
use crate::etcd_fixture as fixture;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{
    Base, Consistency, MetaServiceGroupStatus, MetaStore, Read, StorageEndpoint,
};
use astersql_pd_storage_kv::EtcdMetaStore;
use async_trait::async_trait;
use std::{collections::BTreeMap, sync::Arc, time::Duration};

struct EtcdHealth;
#[async_trait]
impl GroupEndpointHealth for EtcdHealth {
    async fn check(&self, endpoint: &str) -> Result<(), String> {
        let store = EtcdMetaStore::connect(&[endpoint.into()], Duration::from_millis(500))
            .await
            .map_err(|e| e.to_string())?;
        store
            .read(Read::range(b"health", b"", Consistency::Linearizable))
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
async fn persist(
    store: Arc<dyn MetaStore>,
    groups: BTreeMap<String, String>,
) -> Result<(), String> {
    let bytes = serde_json::to_vec(&serde_json::json!({"keyspace":{"meta-service-groups":groups}}))
        .unwrap();
    Base::new(store.as_ref())
        .save(ClusterPaths::new(42).config_path().as_bytes(), &bytes)
        .await
        .map_err(|e| e.to_string())
}

#[tokio::test]
async fn mcs_028_behavior() {
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let manager =
        MetaServiceGroupManager::new(store.clone(), ClusterPaths::new(42), BTreeMap::new());
    let candidate = BTreeMap::from([(" healthy ".into(), format!(" {} ", fixture.endpoint))]);
    manager
        .update_groups_safely(
            candidate,
            &[],
            &EtcdHealth,
            |groups| persist(store.clone(), groups),
            || {},
        )
        .await
        .unwrap();
    assert_eq!(
        manager.get_groups().await.get("healthy"),
        Some(&fixture.endpoint),
        "persisted configuration must be published"
    );
    let config = BTreeMap::from([
        (META_SERVICE_GROUP_ID_KEY.into(), "healthy".into()),
        ("other".into(), "retained".into()),
    ]);
    let decorated = manager.attach_endpoints(&config).await;
    assert_eq!(
        decorated[META_SERVICE_GROUP_ADDRESSES_KEY],
        fixture.endpoint
    );
    assert!(!config.contains_key(META_SERVICE_GROUP_ADDRESSES_KEY));
    let mut snapshot = manager.get_groups().await;
    snapshot.clear();
    assert_eq!(manager.get_groups().await.len(), 1);
    let bytes = Base::new(store.as_ref())
        .load(ClusterPaths::new(42).config_path().as_bytes())
        .await
        .unwrap();
    let persisted: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        persisted["keyspace"]["meta-service-groups"]["healthy"],
        fixture.endpoint
    );
}

#[tokio::test]
async fn mcs_028_health_validation_and_persist_failure() {
    let mut fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let original = BTreeMap::from([("original".into(), fixture.endpoint.clone())]);
    let manager =
        MetaServiceGroupManager::new(store.clone(), ClusterPaths::new(42), original.clone());
    persist(store.clone(), original.clone()).await.unwrap();
    let before = Base::new(store.as_ref())
        .load(ClusterPaths::new(42).config_path().as_bytes())
        .await
        .unwrap();
    for candidate in [
        BTreeMap::from([(
            "new".into(),
            format!("{},http://127.0.0.1:1", fixture.endpoint),
        )]),
        BTreeMap::from([(
            "original".into(),
            format!("{},http://127.0.0.1:1", fixture.endpoint),
        )]),
    ] {
        assert!(matches!(
            manager
                .update_groups_safely(
                    candidate,
                    &[],
                    &EtcdHealth,
                    |groups| persist(store.clone(), groups),
                    || {}
                )
                .await,
            Err(GroupUpdateError::Unhealthy { .. })
        ));
        assert_eq!(manager.get_groups().await, original);
        assert_eq!(
            Base::new(store.as_ref())
                .load(ClusterPaths::new(42).config_path().as_bytes())
                .await
                .unwrap(),
            before
        );
    }
    for candidate in [
        BTreeMap::from([(" ".into(), fixture.endpoint.clone())]),
        BTreeMap::from([("bad/id".into(), fixture.endpoint.clone())]),
        BTreeMap::from([("a".into(), " ".into())]),
        BTreeMap::from([
            ("a".into(), fixture.endpoint.clone()),
            (" a ".into(), fixture.endpoint.clone()),
        ]),
    ] {
        assert!(matches!(
            manager
                .update_groups_safely(
                    candidate,
                    &[],
                    &EtcdHealth,
                    |groups| persist(store.clone(), groups),
                    || {}
                )
                .await,
            Err(GroupUpdateError::InvalidConfiguration(_))
        ));
    }
    fixture.command("stop", "stopped");
    // Unchanged endpoints are not health-checked; the actual configuration write fails.
    assert!(matches!(
        manager
            .update_groups_safely(
                original.clone(),
                &[],
                &EtcdHealth,
                |groups| persist(store.clone(), groups),
                || {}
            )
            .await,
        Err(GroupUpdateError::Persistence(_))
    ));
    assert_eq!(manager.get_groups().await, original);
    fixture.command("start", "restarted");
    assert_eq!(
        Base::new(store.as_ref())
            .load(ClusterPaths::new(42).config_path().as_bytes())
            .await
            .unwrap(),
        before
    );
}

struct Catalog {
    endpoint: astersql_pd_storage_endpoint::StorageEndpoint,
    store: Arc<dyn MetaStore>,
}
#[async_trait]
impl GroupAssignmentCounter for Catalog {
    async fn count(&self, ids: &[String]) -> Result<BTreeMap<String, i64>, String> {
        let mut counts: BTreeMap<String, i64> = ids.iter().map(|id| (id.clone(), 0)).collect();
        let mut txn = Base::new(self.store.as_ref()).begin_txn();
        let rows = self
            .endpoint
            .load_range_keyspace(&mut txn, &ClusterPaths::new(42), 0, 0)
            .await
            .map_err(|e| e.to_string())?;
        for row in rows {
            if let Some(id) = row.config.get(META_SERVICE_GROUP_ID_KEY) {
                if let Some(count) = counts.get_mut(id) {
                    *count += 1;
                }
            }
        }
        txn.commit::<String>().await.map_err(|e| e.to_string())?;
        Ok(counts)
    }
}

#[tokio::test]
async fn mcs_028_authoritative_delete_guard_and_cleanup() {
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let endpoint = StorageEndpoint::new(store.clone());
    let mut txn = Base::new(store.as_ref()).begin_txn();
    endpoint
        .save_meta_service_group_status(
            &mut txn,
            &paths,
            "stale",
            Some(&MetaServiceGroupStatus {
                assignment_count: 3,
                enabled: true,
            }),
        )
        .unwrap();
    txn.commit::<String>().await.unwrap();
    let adapter = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
    let manager = MetaServiceGroupManager::new(
        store.clone(),
        paths.clone(),
        BTreeMap::from([
            ("stale".into(), fixture.endpoint.clone()),
            ("live".into(), fixture.endpoint.clone()),
        ]),
    )
    .with_assignment_counter(Arc::new(Catalog {
        endpoint: astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone()),
        store: store.clone(),
    }));
    manager
        .update_groups_safely(
            BTreeMap::from([("live".into(), fixture.endpoint.clone())]),
            &["stale".into()],
            &EtcdHealth,
            |groups| persist(store.clone(), groups),
            || {},
        )
        .await
        .unwrap();
    assert!(
        Base::new(store.as_ref())
            .load(paths.meta_service_group_status_path("stale").as_bytes())
            .await
            .unwrap()
            .is_empty()
    );
    let meta = astersql_pd_storage_endpoint::Keyspace {
        identity: Some(astersql_pd_storage_endpoint::KeyspaceIdentity::Id(1)),
        name: "live-space".into(),
        config: [(META_SERVICE_GROUP_ID_KEY.into(), "live".into())].into(),
        ..Default::default()
    };
    let mut txn = Base::new(store.as_ref()).begin_txn();
    adapter.save_keyspace_meta(&mut txn, &paths, &meta).unwrap();
    txn.commit::<String>().await.unwrap();
    assert!(
        matches!(manager.update_groups_safely(BTreeMap::new(),&["live".into()],&EtcdHealth,|groups|persist(store.clone(),groups),||{}).await,Err(GroupUpdateError::AssignedKeyspaces(id)) if id=="live")
    );
    assert!(manager.get_groups().await.contains_key("live"));
}

#[tokio::test]
async fn mcs_028_reservation_excludes_directory_deletion() {
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let endpoint = StorageEndpoint::new(store.clone());
    let mut txn = Base::new(store.as_ref()).begin_txn();
    endpoint
        .save_meta_service_group_status(
            &mut txn,
            &paths,
            "live",
            Some(&MetaServiceGroupStatus {
                assignment_count: 0,
                enabled: true,
            }),
        )
        .unwrap();
    txn.commit::<String>().await.unwrap();
    let manager = MetaServiceGroupManager::new(
        store.clone(),
        paths.clone(),
        BTreeMap::from([("live".into(), fixture.endpoint.clone())]),
    );
    let mut txn = Base::new(store.as_ref()).begin_txn();
    let reservation = manager.assign_in_txn(&mut txn, 1).await.unwrap();
    let deleted = vec!["live".into()];
    let update = manager.update_groups_safely(
        BTreeMap::new(),
        &deleted,
        &EtcdHealth,
        |groups| persist(store.clone(), groups),
        || {},
    );
    tokio::pin!(update);
    assert!(
        tokio::time::timeout(Duration::from_millis(30), &mut update)
            .await
            .is_err()
    );
    txn.commit::<String>().await.unwrap();
    drop(reservation);
    assert!(matches!(update.await,Err(GroupUpdateError::AssignedKeyspaces(id)) if id=="live"));
    assert!(manager.get_groups().await.contains_key("live"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mcs_028_concurrent_updates_publish_in_persistence_order() {
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let manager = Arc::new(MetaServiceGroupManager::new(
        store.clone(),
        ClusterPaths::new(42),
        BTreeMap::new(),
    ));
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let first_manager = manager.clone();
    let first_store = store.clone();
    let first = tokio::spawn(async move {
        first_manager
            .update_groups_safely(
                BTreeMap::new(),
                &[],
                &EtcdHealth,
                |groups| async move {
                    started_tx.send(()).unwrap();
                    release_rx.await.unwrap();
                    persist(first_store, groups).await
                },
                || {},
            )
            .await
    });
    started_rx.await.unwrap();
    let next = BTreeMap::from([("next".into(), fixture.endpoint.clone())]);
    let second_manager = manager.clone();
    let second_store = store.clone();
    let second_groups = next.clone();
    let mut second = tokio::spawn(async move {
        second_manager
            .update_groups_safely(
                second_groups,
                &[],
                &EtcdHealth,
                |groups| persist(second_store, groups),
                || {},
            )
            .await
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(30), &mut second)
            .await
            .is_err()
    );
    release_tx.send(()).unwrap();
    first.await.unwrap().unwrap();
    second.await.unwrap().unwrap();
    assert_eq!(manager.get_groups().await, next);
    let bytes = Base::new(store.as_ref())
        .load(ClusterPaths::new(42).config_path().as_bytes())
        .await
        .unwrap();
    let config: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        config["keyspace"]["meta-service-groups"]["next"],
        next["next"]
    );
}

#[tokio::test]
async fn mcs_028_cleanup_failure_preserves_committed_directory() {
    let mut fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let endpoint = StorageEndpoint::new(store.clone());
    let mut txn = Base::new(store.as_ref()).begin_txn();
    endpoint
        .save_meta_service_group_status(
            &mut txn,
            &paths,
            "old",
            Some(&MetaServiceGroupStatus {
                assignment_count: 0,
                enabled: true,
            }),
        )
        .unwrap();
    txn.commit::<String>().await.unwrap();
    let manager = MetaServiceGroupManager::new(
        store.clone(),
        paths.clone(),
        BTreeMap::from([("old".into(), fixture.endpoint.clone())]),
    );
    let outcome = manager
        .update_groups_safely(
            BTreeMap::new(),
            &["old".into()],
            &EtcdHealth,
            |groups| async {
                persist(store.clone(), groups).await?;
                fixture.command("stop", "stopped");
                Ok(())
            },
            || {},
        )
        .await
        .unwrap();
    assert!(outcome.cleanup_error.is_some());
    assert!(manager.get_groups().await.is_empty());
    fixture.command("start", "restarted");
    let bytes = Base::new(store.as_ref())
        .load(paths.config_path().as_bytes())
        .await
        .unwrap();
    let config: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        config["keyspace"]["meta-service-groups"],
        serde_json::json!({})
    );
}

#[test]
fn mcs_028_replays_configuration_normalization() {
    for (input, expected) in [
        (
            vec![(" group-1 ", " http://127.0.0.1:2379 ")],
            vec![("group-1", "http://127.0.0.1:2379")],
        ),
        (
            vec![
                ("group-1", "http://127.0.0.1:2379"),
                (" group-2 ", " http://127.0.0.1:2380 "),
            ],
            vec![
                ("group-1", "http://127.0.0.1:2379"),
                ("group-2", "http://127.0.0.1:2380"),
            ],
        ),
        (
            vec![("group.1:8080", "http://127.0.0.1:2379")],
            vec![("group.1:8080", "http://127.0.0.1:2379")],
        ),
    ] {
        let input: BTreeMap<_, _> = input
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        let expected: BTreeMap<_, _> = expected
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        assert_eq!(normalize_meta_service_groups(input).unwrap(), expected);
    }
    for (input, message) in [
        (
            vec![(" ", "http://127.0.0.1:2379")],
            "[keyspace] meta-service group ID cannot be empty",
        ),
        (
            vec![("group-1", " ")],
            "[keyspace] meta-service group addresses cannot be empty",
        ),
        (
            vec![
                ("group-1", "http://127.0.0.1:2379"),
                (" group-1 ", "http://127.0.0.1:2380"),
            ],
            "[keyspace] meta-service group ID cannot be duplicated: group-1",
        ),
        (
            vec![("group/1", "http://127.0.0.1:2379")],
            "[keyspace] meta-service group ID cannot contain '/': group/1",
        ),
    ] {
        let input = input
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        assert_eq!(
            normalize_meta_service_groups(input)
                .unwrap_err()
                .to_string(),
            message
        );
    }
}
