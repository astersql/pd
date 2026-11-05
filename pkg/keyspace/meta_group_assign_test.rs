// Copyright 2026 AsterSQL.

use super::*;
use crate::etcd_fixture as fixture;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{
    Base, CallbackTxnError, MetaServiceGroupStatus, MetaStore, StorageEndpoint,
};
use std::{collections::BTreeMap, sync::Arc};

async fn setup(store: Arc<dyn MetaStore>) -> MetaServiceGroupManager {
    let endpoint = StorageEndpoint::new(store.clone());
    let paths = ClusterPaths::new(42);
    Base::new(store.as_ref())
        .run_in_txn(async |txn| {
            for (id, count, enabled) in [("a", 0, true), ("b", 5, true), ("disabled", -20, false)] {
                endpoint.save_meta_service_group_status(
                    txn,
                    &paths,
                    id,
                    Some(&MetaServiceGroupStatus {
                        assignment_count: count,
                        enabled,
                    }),
                )?;
            }
            Ok::<_, astersql_pd_storage_domain::EndpointError>(())
        })
        .await
        .unwrap();
    MetaServiceGroupManager::new(
        store,
        paths,
        ["a", "b", "disabled"]
            .map(|id| (id.to_owned(), String::new()))
            .into(),
    )
}

async fn count(store: &dyn MetaStore, id: &str) -> i64 {
    let bytes = Base::new(store)
        .load(
            ClusterPaths::new(42)
                .meta_service_group_status_path(id)
                .as_bytes(),
        )
        .await
        .unwrap();
    serde_json::from_slice::<MetaServiceGroupStatus>(&bytes)
        .unwrap()
        .assignment_count
}

#[tokio::test]
async fn mcs_027_behavior() {
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let manager = setup(store.clone()).await;
    fn assert_send<T: Send>(_: T) {}
    assert_send(manager.pick_group());
    assert_send(manager.rollback_assignment("a"));
    assert_eq!(manager.pick_group().await.unwrap(), "a");
    assert_eq!(
        count(store.as_ref(), "a").await,
        1,
        "PickGroup must persist its reservation"
    );
    assert_eq!(manager.assign_to_group(3).await.unwrap(), "a");
    assert_eq!(count(store.as_ref(), "a").await, 4);
    assert_eq!(count(store.as_ref(), "b").await, 5);
    assert_eq!(count(store.as_ref(), "disabled").await, -20);
    manager.rollback_assignment("a").await.unwrap();
    assert_eq!(count(store.as_ref(), "a").await, 3);
    manager.rollback_assignment("missing").await.unwrap();
    assert!(
        Base::new(store.as_ref())
            .load(
                ClusterPaths::new(42)
                    .meta_service_group_status_path("missing")
                    .as_bytes()
            )
            .await
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        manager.assign_to_group(-1).await,
        Err(CallbackTxnError::Callback(
            GroupAssignmentError::InvalidCount
        ))
    ));
    let empty = MetaServiceGroupManager::new(store.clone(), ClusterPaths::new(42), BTreeMap::new());
    assert!(matches!(
        empty.pick_group().await,
        Err(CallbackTxnError::Callback(
            GroupAssignmentError::NoAvailableGroups
        ))
    ));
    let before = count(store.as_ref(), "a").await;
    let failed = Base::new(store.as_ref())
        .run_in_txn(async |txn| {
            manager.assign_in_txn(txn, 1).await.unwrap();
            txn.save(b"/failed-keyspace", b"uncommitted").unwrap();
            Err::<(), _>("creation failed")
        })
        .await;
    assert_eq!(failed, Err(CallbackTxnError::Callback("creation failed")));
    assert_eq!(count(store.as_ref(), "a").await, before);
    assert!(
        Base::new(store.as_ref())
            .load(b"/failed-keyspace")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_027_concurrent_and_restart() {
    let mut fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let manager = Arc::new(setup(store.clone()).await);
    let mut jobs = Vec::with_capacity(8);
    for _ in 0..8 {
        let manager = manager.clone();
        jobs.push(tokio::spawn(async move { manager.pick_group().await }));
    }
    let mut succeeded = 0;
    for job in jobs {
        match job.await.unwrap() {
            Ok(_) => succeeded += 1,
            Err(CallbackTxnError::Conflict) => {}
            other => panic!("unexpected reservation result: {other:?}"),
        }
    }
    assert!(succeeded > 0);
    let total = count(store.as_ref(), "a").await + count(store.as_ref(), "b").await;
    assert_eq!(total, 5 + succeeded);
    fixture.command("stop", "stopped");
    assert!(manager.pick_group().await.is_err());
    fixture.command("start", "restarted");
    let recovered = fixture.store().await;
    assert_eq!(
        count(&recovered, "a").await + count(&recovered, "b").await,
        total
    );
}

#[tokio::test]
async fn mcs_027_conflict_discards_reservation_and_keyspace() {
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let manager = setup(store.clone()).await;
    let barrier = tokio::sync::Barrier::new(2);
    let base = Base::new(store.as_ref());
    let left = base.run_in_txn(async |txn| {
        manager.assign_in_txn(txn, 1).await?;
        txn.save(b"/keyspace-left", b"created").unwrap();
        barrier.wait().await;
        Ok::<_, GroupAssignmentError>(())
    });
    let right = base.run_in_txn(async |txn| {
        manager.assign_in_txn(txn, 1).await?;
        txn.save(b"/keyspace-right", b"created").unwrap();
        barrier.wait().await;
        Ok::<_, GroupAssignmentError>(())
    });
    let (left, right) = tokio::join!(left, right);
    assert!(matches!(
        (&left, &right),
        (Ok(()), Err(CallbackTxnError::Conflict)) | (Err(CallbackTxnError::Conflict), Ok(()))
    ));
    assert_eq!(count(store.as_ref(), "a").await, 1);
    for (key, succeeded) in [
        (b"/keyspace-left".as_slice(), left.is_ok()),
        (b"/keyspace-right".as_slice(), right.is_ok()),
    ] {
        assert_eq!(!base.load(key).await.unwrap().is_empty(), succeeded);
    }
}

#[tokio::test]
async fn mcs_027_assignment_count_five_and_disabled_groups() {
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let manager = setup(store.clone()).await;
    assert_eq!(manager.assign_to_group(5).await.unwrap(), "a");
    assert_eq!(count(store.as_ref(), "a").await, 5);
    assert_eq!(count(store.as_ref(), "b").await, 5);
    assert_eq!(count(store.as_ref(), "disabled").await, -20);
    let disabled = MetaServiceGroupManager::new(
        store.clone(),
        ClusterPaths::new(42),
        [("disabled".into(), String::new())].into(),
    );
    assert!(matches!(
        disabled.pick_group().await,
        Err(CallbackTxnError::Callback(
            GroupAssignmentError::NoAvailableGroups
        ))
    ));
    assert!(matches!(
        manager.assign_to_group(-1).await,
        Err(CallbackTxnError::Callback(
            GroupAssignmentError::InvalidCount
        ))
    ));
    assert_eq!(count(store.as_ref(), "a").await, 5);
    assert_eq!(count(store.as_ref(), "b").await, 5);
}

/// Replay the exact enabled group IDs and count inputs from the isolated Go suite.
#[tokio::test]
async fn mcs_027_replays_direct_go_cases() {
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let groups: BTreeMap<String, String> = (0..3)
        .map(|n| {
            (
                format!("etcd-group-{n}"),
                format!("etcd-group-{n}.tidb-serverless.cluster.svc.local"),
            )
        })
        .collect();
    let endpoint = StorageEndpoint::new(store.clone());
    let manager = MetaServiceGroupManager::new(store.clone(), paths.clone(), groups.clone());
    assert!(matches!(
        manager.assign_to_group(1).await,
        Err(CallbackTxnError::Callback(
            GroupAssignmentError::NoAvailableGroups
        ))
    ));
    assert!(matches!(
        manager.assign_to_group(-1).await,
        Err(CallbackTxnError::Callback(
            GroupAssignmentError::InvalidCount
        ))
    ));
    for increment in [5, 1] {
        Base::new(store.as_ref())
            .run_in_txn(async |txn| {
                for id in groups.keys() {
                    endpoint.save_meta_service_group_status(
                        txn,
                        &paths,
                        id,
                        Some(&MetaServiceGroupStatus {
                            assignment_count: 0,
                            enabled: true,
                        }),
                    )?;
                }
                Ok::<_, astersql_pd_storage_domain::EndpointError>(())
            })
            .await
            .unwrap();
        let selected = if increment == 5 {
            manager.assign_to_group(5).await.unwrap()
        } else {
            manager.pick_group().await.unwrap()
        };
        assert!(groups.contains_key(&selected));
        for id in groups.keys() {
            assert_eq!(
                count(store.as_ref(), id).await,
                if *id == selected { increment } else { 0 }
            );
        }
    }
}
