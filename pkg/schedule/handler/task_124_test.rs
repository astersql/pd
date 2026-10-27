// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_cache::{TTLUint64, TtlDuration};
use astersql_pd_core::{
    BasicCluster, KeyRange, Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta,
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule::{Coordinator, CoordinatorInputs};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_hbstream::routing::StreamRouter;
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_schedule_scatter::ScatterBatchMetrics;
use astersql_pd_schedule_schedulers::{FactoryContext, JobTime};
use astersql_pd_storage_domain::MemoryStore;
use astersql_pd_storage_domain::StorageEndpoint;
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
fn setup_admin(
    rules: bool,
    backend: Arc<dyn astersql_pd_storage_domain::MetaStore>,
) -> (Coordinator<ProviderSnapshot<()>>, Arc<TTLUint64<()>>) {
    let mut cluster = BasicCluster::new();
    for (id, start, end, count) in [(1, b"a", b"b", 3), (2, b"b", b"c", 2), (3, b"c", b"d", 3)] {
        let peers: Vec<_> = (1..=count)
            .map(|i| Peer {
                id: i,
                store_id: i,
                ..Default::default()
            })
            .collect();
        cluster.regions.set_region(Arc::new(RegionInfo::new(
            RegionMeta {
                id,
                key_range: KeyRange {
                    start_key: start.to_vec(),
                    end_key: end.to_vec(),
                },
                peers: peers.clone(),
                ..Default::default()
            },
            Some(peers[0].clone()),
            vec![],
            vec![],
        )));
    }
    let cluster = Arc::new(cluster);
    for id in 1..=3 {
        cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        })));
    }
    let (heartbeat, _router) = StreamRouter::new(Arc::new(astersql_pd_core::StoresInfo::new()));
    let pending = Arc::new(TTLUint64::new(
        tokio::runtime::Handle::current(),
        CancellationToken::new(),
        Duration::from_secs(60),
        TtlDuration::from_nanos(180_000_000_000),
    ));
    let c = Coordinator::new(CoordinatorInputs {
        cluster,
        config: Arc::new({
            let mut config = ProviderSnapshot::<()>::default();
            config.schedule.scheduler_max_waiting_operator = 10;
            config.schedule.enable_diagnostic = false;
            config.replication.enable_placement_rules = rules;
            config.replication.max_replicas = 3;
            config.schedule.replica_schedule_limit = 10;
            config.schedule.enable_make_up_replica = true;
            config.schedule.switch_witness_interval = TtlDuration::from_nanos(10);
            config
        }),
        heartbeat,
        pending: pending.clone(),
        factory: FactoryContext::new(
            Arc::new(StorageEndpoint::new(backend)),
            ClusterPaths::new(114),
            JobTime(0),
        ),
        scheduler_configuration: json!({}),
        scatter_metrics: ScatterBatchMetrics::new(&prometheus::Registry::new()).unwrap(),
        start: Duration::ZERO,
        now_unix_nanos: 0,
        split_merge_interval_nanos: 0,
        total_regions: Box::new(|| Ok(0)),
        unix_seconds: Arc::new(|| 0),
    });
    (c, pending)
}

#[test]
fn task_124_limit_cases() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_124.json"
    ))
    .unwrap();
    for row in f["limits"].as_array().unwrap() {
        assert_eq!(
            adjust_region_limit(row[0].as_str().unwrap(), &[]).unwrap(),
            row[1].as_i64().unwrap() as isize
        );
    }
    assert_eq!(adjust_region_limit("", &[99, 100]).unwrap(), 99);
    for s in [" ", "1 ", "0x10", "1_0", "9223372036854775808", "+", "--1"] {
        assert!(adjust_region_limit(s, &[]).is_err(), "{s}");
    }
}
#[tokio::test]
async fn task_124_behavior_cases() {
    let (c, p) = setup_admin(false, Arc::new(MemoryStore::default()));
    let h = Handler::new(Some(&c));
    h.accelerate_regions_schedule_in_range("61", "64", 1)
        .unwrap();
    assert_eq!(
        c.get_checker_controller()
            .lock()
            .unwrap()
            .pending_processed_regions(),
        vec![1]
    );
    h.accelerate_regions_schedule_in_ranges(
        &[b"b".to_vec(), b"b".to_vec()],
        &[b"d".to_vec(), b"c".to_vec()],
        1,
    )
    .unwrap();
    let mut ids = c
        .get_checker_controller()
        .lock()
        .unwrap()
        .pending_processed_regions();
    ids.sort();
    assert_eq!(ids, vec![1, 2]);
    c.get_checker_controller()
        .lock()
        .unwrap()
        .clear_pending_processed_regions();
    h.accelerate_regions_schedule_in_range("61", "64", 0)
        .unwrap();
    let mut ids = c
        .get_checker_controller()
        .lock()
        .unwrap()
        .pending_processed_regions();
    ids.sort();
    assert_eq!(ids, vec![1, 2, 3]);
    for (a, b, limit, want) in [
        ("6161", "62", -1, vec![1]),
        ("64", "", 0, vec![]),
        ("63", "63", 0, vec![]),
        ("", "", -1, vec![1, 2, 3]),
    ] {
        c.get_checker_controller()
            .lock()
            .unwrap()
            .clear_pending_processed_regions();
        h.accelerate_regions_schedule_in_range(a, b, limit).unwrap();
        let mut ids = c
            .get_checker_controller()
            .lock()
            .unwrap()
            .pending_processed_regions();
        ids.sort();
        assert_eq!(ids, want);
    }
    p.shutdown().await;
}
#[tokio::test]
async fn task_124_replication_and_rules() {
    for enabled in [false, true] {
        let backend = Arc::new(MemoryStore::default());
        let (c, p) = setup_admin(enabled, backend.clone());
        let h = Handler::new(Some(&c));
        let mut manager = RuleManager::new(
            Arc::new(StorageEndpoint::new(backend)),
            ClusterPaths::new(124),
        );
        manager
            .initialize(3, &[], "", false, &**c.get_config(), &[])
            .await
            .unwrap();
        assert_eq!(
            h.check_regions_replicated("61", "62", &mut manager, 0)
                .unwrap(),
            "REPLICATED"
        );
        assert_eq!(
            h.check_regions_replicated("61", "64", &mut manager, 0)
                .unwrap(),
            "INPROGRESS"
        );
        c.get_checker_controller()
            .lock()
            .unwrap()
            .add_pending_processed_regions(false, &[1]);
        assert_eq!(
            h.check_regions_replicated("61", "64", &mut manager, 0)
                .unwrap(),
            "INPROGRESS"
        );
        c.get_checker_controller()
            .lock()
            .unwrap()
            .add_pending_processed_regions(false, &[2]);
        // Acceleration membership is not the rule checker's pending list.
        assert_eq!(
            h.check_regions_replicated("61", "64", &mut manager, 0)
                .unwrap(),
            "INPROGRESS"
        );
        assert_eq!(
            h.check_regions_replicated("64", "", &mut manager, 0)
                .unwrap(),
            "REPLICATED"
        );
        let r = h.pre_check_for_region("01").unwrap();
        assert!(Arc::ptr_eq(
            &r,
            &c.get_cluster().regions.get_region(1).unwrap()
        ));
        if enabled {
            assert!(
                h.check_region_placement_rule(&r, &mut manager, 0)
                    .unwrap()
                    .is_satisfied()
            );
            assert!(
                !h.check_region_placement_rule(
                    &h.pre_check_for_region("2").unwrap(),
                    &mut manager,
                    0
                )
                .unwrap()
                .is_satisfied()
            );
        } else {
            assert!(matches!(
                h.check_region_placement_rule(&r, &mut manager, 0),
                Err(RegionAdminError::PlacementDisabled)
            ));
        }
        if enabled {
            // Exhaust real placement candidates; dispatch must publish rule-pending.
            c.get_cluster().stores.delete_store(3);
            dispatch_missing(&c, &mut manager).await;
            assert!(c.is_pending_region(2));
            assert_eq!(
                h.check_regions_replicated("62", "63", &mut manager, 0)
                    .unwrap(),
                "PENDING"
            );
            let mut rule = manager.get_rule("pd", "default").unwrap();
            rule.count = 2;
            manager
                .set_rule(rule, 1, &**c.get_config(), &[])
                .await
                .unwrap();
            assert_eq!(
                h.check_regions_replicated("62", "63", &mut manager, 0)
                    .unwrap(),
                "REPLICATED"
            );
        }
        p.shutdown().await;
    }
}
async fn dispatch_missing(c: &Coordinator<ProviderSnapshot<()>>, manager: &mut RuleManager) {
    use astersql_pd_schedule_checker::{
        AffinityContext, CheckerDispatchContext, ReplicaFaultContext, RuleMissingContext,
    };
    let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let paths = ClusterPaths::new(124);
    let labels = astersql_pd_schedule_labeler::RegionLabeler::load(
        StorageEndpoint::new(Arc::new(MemoryStore::default())),
        paths.clone(),
        0,
    )
    .await
    .unwrap();
    let hot = std::collections::BTreeSet::new();
    let ctx = CheckerDispatchContext {
        affinity: AffinityContext {
            rules: RuleMissingContext {
                replica: ReplicaFaultContext {
                    config: &**c.get_config(),
                    stores: &c.get_cluster().stores,
                    rules: &[],
                    now_unix_nanos: 0,
                    supports_conf_change_v2: false,
                },
                witness_enabled: false,
            },
            labels: &labels,
            storage: &storage,
            paths: &paths,
            region_exists: true,
        },
        cluster: c.get_cluster(),
        operators: c.get_operator_controller(),
        hot_regions: &hot,
    };
    let mut affinity = astersql_pd_schedule_affinity::manager::Manager::default();
    let out = c
        .get_checker_controller()
        .lock()
        .unwrap()
        .check_region(
            &ctx,
            manager,
            &mut affinity,
            &c.get_cluster().regions.get_region(2).unwrap(),
            &mut || Ok(100),
        )
        .await;
    assert!(out.operators.is_empty());
}
#[tokio::test]
async fn task_124_errors_and_precedence() {
    let (c, p) = setup_admin(false, Arc::new(MemoryStore::default()));
    let h = Handler::new(Some(&c));
    for s in ["", "+1", "-1", " 1", "1 ", "0x1", "18446744073709551616"] {
        assert!(matches!(
            h.pre_check_for_region(s),
            Err(AdminPeerError::InvalidRegionId)
        ));
    }
    for s in ["0", "99", "18446744073709551615"] {
        assert!(matches!(
            h.pre_check_for_region(s),
            Err(AdminPeerError::RegionNotFound(_))
        ));
    }
    assert!(
        h.accelerate_regions_schedule_in_ranges(&[vec![]], &[], 0)
            .is_err()
    );
    assert!(
        c.get_checker_controller()
            .lock()
            .unwrap()
            .pending_processed_regions()
            .is_empty()
    );
    for (a, b) in [("0", ""), ("gg", ""), ("", "z0")] {
        assert!(matches!(
            h.accelerate_regions_schedule_in_range(a, b, 0),
            Err(RegionAdminError::Hex(_))
        ));
    }
    let none = Handler::<ProviderSnapshot<()>>::new(None);
    assert!(matches!(
        none.accelerate_regions_schedule_in_range("z", "", 0),
        Err(RegionAdminError::Hex(_))
    ));
    assert!(matches!(
        none.accelerate_regions_schedule_in_range("", "", 0),
        Err(RegionAdminError::Handler(HandlerError::NotBootstrapped))
    ));
    assert!(matches!(
        none.accelerate_regions_schedule_in_ranges(&[vec![]], &[], 0),
        Err(RegionAdminError::Handler(HandlerError::NotBootstrapped))
    ));
    assert!(matches!(
        none.pre_check_for_region("bad"),
        Err(AdminPeerError::Handler(HandlerError::NotBootstrapped))
    ));
    let mut manager = RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        ClusterPaths::new(124),
    );
    assert!(matches!(
        none.check_regions_replicated("z", "", &mut manager, 0),
        Err(RegionAdminError::Hex(_))
    ));
    assert!(matches!(
        none.check_regions_replicated("", "", &mut manager, 0),
        Err(RegionAdminError::Handler(HandlerError::NotBootstrapped))
    ));
    assert!(matches!(
        none.check_region_placement_rule(&h.pre_check_for_region("1").unwrap(), &mut manager, 0),
        Err(RegionAdminError::Handler(HandlerError::NotBootstrapped))
    ));
    assert_eq!(
        h.accelerate_regions_schedule_in_range("0", "", 0)
            .unwrap_err()
            .to_string(),
        "encoding/hex: odd length hex string"
    );
    assert_eq!(
        h.accelerate_regions_schedule_in_range("gg", "", 0)
            .unwrap_err()
            .to_string(),
        "encoding/hex: invalid byte: U+0067 'g'"
    );
    h.accelerate_regions_schedule_in_range("6A", "6B", 0)
        .unwrap();
    h.accelerate_regions_schedule_in_ranges(&[], &[], 0)
        .unwrap();
    assert!(
        c.get_checker_controller()
            .lock()
            .unwrap()
            .pending_processed_regions()
            .is_empty()
    );
    p.shutdown().await;
}

#[tokio::test]
async fn task_124_acceleration_bypasses_retry_cap() {
    let (c, p) = setup_admin(false, Arc::new(MemoryStore::default()));
    let ids: Vec<_> = (1000..101002).collect();
    c.get_checker_controller()
        .lock()
        .unwrap()
        .add_pending_processed_regions(false, &ids);
    assert_eq!(
        c.get_checker_controller()
            .lock()
            .unwrap()
            .pending_processed_regions()
            .len(),
        100002
    );
    let h = Handler::new(Some(&c));
    h.accelerate_regions_schedule_in_range("61", "62", 1)
        .unwrap();
    assert!(
        c.get_checker_controller()
            .lock()
            .unwrap()
            .pending_processed_regions()
            .contains(&1)
    );
    assert!(!c.is_pending_region(1));
    p.shutdown().await;
}
