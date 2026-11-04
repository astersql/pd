// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{BasicCluster, StoreInfo, StoreMeta, StoreState};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage::{MemoryStore, MetaStore};
use astersql_pd_storage_endpoint::StorageEndpoint;
use std::sync::Arc;
struct Cluster {
    cluster: BasicCluster,
    endpoint: StorageEndpoint,
    halt: bool,
}
#[async_trait::async_trait]
impl RecoveryCluster for Cluster {
    fn stores(&self) -> Vec<Arc<StoreInfo>> {
        self.cluster.stores.get_stores()
    }
    async fn bury_store(&mut self, id: u64) -> Result<(), RecoveryError> {
        let old = self
            .cluster
            .stores
            .get_store(id)
            .ok_or(RecoveryError::StoreNotFound(id))?;
        let mut next = (*old).clone();
        next.set_store_state(StoreState::Tombstone, None);
        self.endpoint
            .save_store_meta(&ClusterPaths::new(16), next.meta().unwrap())
            .await
            .map_err(|e| RecoveryError::Backend(e.to_string()))?;
        self.cluster.stores.put_store(Arc::new(next));
        Ok(())
    }
    fn set_recovery_halt(&mut self, halt: bool) {
        self.halt = halt;
    }
}
fn fixture() -> Cluster {
    let cluster = BasicCluster::default();
    for id in 1..=3 {
        let meta = StoreMeta {
            id,
            ..Default::default()
        };
        cluster.stores.put_store(Arc::new(StoreInfo::new(meta)));
    }
    Cluster {
        cluster,
        endpoint: StorageEndpoint::new(Arc::new(MemoryStore::default()) as Arc<dyn MetaStore>),
        halt: false,
    }
}
#[tokio::test]
async fn mcs_016_behavior() {
    let mut cluster = fixture();
    let mut controller = Controller::new(Arc::new(RecoverySteps::new(1600)));
    controller
        .remove_failed_stores_with_options(
            &mut cluster,
            StartRequest {
                failed_stores: [1].into(),
                timeout_seconds: 600,
                options: RemoveFailedStoresOptions {
                    plan_execution_timeout_nanos: 300_000_000_000,
                    disable_paranoid_check: true,
                },
                ..Default::default()
            },
            1_000_000_000_000,
        )
        .await
        .unwrap();
    assert_eq!(
        controller.stage,
        Stage::CollectReport,
        "accepted recovery must enter collect report"
    );
    assert!(cluster.halt);
    assert!(cluster.cluster.stores.get_store(1).unwrap().is_removed());
    assert_eq!(
        controller
            .expected_reports
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
    assert_eq!(controller.plan_execution_timeout_nanos, 300_000_000_000);
    assert!(controller.disable_paranoid_check);
    assert_eq!(controller.timeout_unix_nanos, 1_600_000_000_000);
    assert_eq!(controller.recovery_start_step, 1601);
    assert_eq!(controller.step, 1602);
    let durable = cluster
        .endpoint
        .load_store_meta(&ClusterPaths::new(16), 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(durable.state, StoreState::Tombstone);
}

#[tokio::test]
async fn mcs_016_rejections_are_ordered_and_do_not_bury_valid_ids() {
    let mut cluster = fixture();
    let mut connected = (*cluster.cluster.stores.get_store(2).unwrap()).clone();
    connected.set_last_heartbeat_unix_nanos(1_000_000_000_000);
    cluster.cluster.stores.put_store(Arc::new(connected));
    let mut controller = Controller::new(Arc::new(RecoverySteps::new(16)));
    for (ids, expected) in [
        (vec![], "no store specified"),
        (vec![1, 9], "store 9 doesn't exist"),
        (vec![1, 2], "store 2 is up and connected"),
    ] {
        let error = controller
            .remove_failed_stores_with_options(
                &mut cluster,
                StartRequest {
                    failed_stores: ids.into_iter().collect(),
                    timeout_seconds: 60,
                    ..Default::default()
                },
                1_000_000_000_000,
            )
            .await
            .unwrap_err();
        assert!(error.to_string().ends_with(expected));
        assert_eq!(controller.stage, Stage::Idle);
        assert!(!cluster.halt);
        assert!(!cluster.cluster.stores.get_store(1).unwrap().is_removed());
        assert!(
            cluster
                .endpoint
                .load_store_meta(&ClusterPaths::new(16), 1)
                .await
                .unwrap()
                .is_none()
        );
    }
    // Exactly 20 seconds is still connected; only strictly greater is disconnected.
    assert!(
        controller
            .remove_failed_stores_with_options(
                &mut cluster,
                StartRequest {
                    failed_stores: [2].into(),
                    ..Default::default()
                },
                1_020_000_000_000
            )
            .await
            .is_err()
    );
    controller
        .remove_failed_stores_with_options(
            &mut cluster,
            StartRequest {
                failed_stores: [2].into(),
                ..Default::default()
            },
            1_020_000_000_001,
        )
        .await
        .unwrap();
    assert_eq!(
        controller
            .remove_failed_stores_with_options(&mut cluster, StartRequest::default(), 0)
            .await,
        Err(RecoveryError::Running)
    );
}
#[tokio::test]
async fn mcs_016_auto_detect_filters_destroyed_and_keeps_manual_options() {
    let mut cluster = fixture();
    cluster
        .cluster
        .stores
        .patch_store(3, |s| s.set_store_state(StoreState::Offline, Some(true)));
    let steps = Arc::new(RecoverySteps::new(100));
    let mut controller = Controller::new(steps.clone());
    controller
        .remove_failed_stores_with_options(
            &mut cluster,
            StartRequest {
                auto_detect: true,
                failed_stores: [1].into(),
                timeout_seconds: u64::MAX,
                options: RemoveFailedStoresOptions {
                    plan_execution_timeout_nanos: -1,
                    disable_paranoid_check: false,
                },
            },
            500,
        )
        .await
        .unwrap();
    assert!(!cluster.cluster.stores.get_store(1).unwrap().is_removed());
    assert_eq!(
        controller
            .expected_reports
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![2]
    );
    assert_eq!(controller.plan_execution_timeout_nanos, 60_000_000_000);
    assert_eq!(controller.timeout_unix_nanos, 500 - 1_000_000_000);
    let mut successor = Controller::new(steps);
    let previous = controller.step;
    successor
        .remove_failed_stores_with_options(
            &mut cluster,
            StartRequest {
                auto_detect: true,
                ..Default::default()
            },
            500,
        )
        .await
        .unwrap();
    assert!(successor.step > previous);
}

#[tokio::test]
async fn mcs_016_real_persistence_failure_does_not_start_or_publish_tombstone() {
    let mut native = crate::etcd_fixture::Fixture::start();
    let backend = Arc::new(native.store().await);
    let mut cluster = fixture();
    cluster.endpoint = StorageEndpoint::new(backend.clone());
    cluster
        .endpoint
        .save_store_meta(
            &ClusterPaths::new(16),
            cluster.cluster.stores.get_store(1).unwrap().meta().unwrap(),
        )
        .await
        .unwrap();
    native.command("stop", "stopped");
    let mut controller = Controller::new(Arc::new(RecoverySteps::new(1000)));
    let request = StartRequest {
        failed_stores: [1].into(),
        timeout_seconds: 600,
        ..Default::default()
    };
    assert!(matches!(
        controller
            .remove_failed_stores_with_options(&mut cluster, request.clone(), 1_000_000_000_000)
            .await,
        Err(RecoveryError::Backend(_))
    ));
    assert_eq!(controller.stage, Stage::Idle);
    assert!(!cluster.halt);
    assert_eq!(controller.step, 0);
    assert!(!cluster.cluster.stores.get_store(1).unwrap().is_removed());
    native.command("start", "restarted");
    tokio::time::timeout(std::time::Duration::from_secs(8), async {
        loop {
            if cluster
                .endpoint
                .load_store_meta(&ClusterPaths::new(16), 1)
                .await
                .is_ok()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_ne!(
        cluster
            .endpoint
            .load_store_meta(&ClusterPaths::new(16), 1)
            .await
            .unwrap()
            .unwrap()
            .state,
        StoreState::Tombstone
    );
    controller
        .remove_failed_stores_with_options(&mut cluster, request, 1_000_000_000_000)
        .await
        .unwrap();
    assert!(cluster.halt);
    assert_eq!(controller.recovery_start_step, 1001);
    assert_eq!(controller.step, 1002);
    assert_eq!(
        cluster
            .endpoint
            .load_store_meta(&ClusterPaths::new(16), 1)
            .await
            .unwrap()
            .unwrap()
            .state,
        StoreState::Tombstone
    );
}
