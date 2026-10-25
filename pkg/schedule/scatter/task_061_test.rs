// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    BasicCluster, KeyRange, Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta, StoreStats,
};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_operator::{
    BuilderError, ControllerAdmission, OpKind, Operator, TransferLeader,
};
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use std::{collections::BTreeSet, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

fn cluster(pending: bool) -> BasicCluster {
    let mut c = BasicCluster::new();
    for id in 1..=4 {
        let mut s = StoreInfo::new(StoreMeta {
            id,
            last_heartbeat: 1_000_000_000,
            ..Default::default()
        });
        s.update_raw_stats(StoreStats {
            capacity: 1_000_000_000_000,
            available: 900_000_000_000,
            used_size: 100_000_000_000,
            ..Default::default()
        });
        c.stores.put_store(Arc::new(s));
    }
    for id in 1..=2 {
        let peers: Vec<_> = (1..=3)
            .map(|store_id| Peer {
                id: id * 10 + store_id,
                store_id,
                ..Default::default()
            })
            .collect();
        c.regions.set_region(Arc::new(RegionInfo::new(
            RegionMeta {
                id,
                key_range: KeyRange {
                    start_key: vec![id as u8],
                    end_key: vec![id as u8 + 1],
                },
                peers: peers.clone(),
                ..Default::default()
            },
            Some(peers[0].clone()),
            vec![],
            if pending {
                vec![peers[1].clone()]
            } else {
                vec![]
            },
        )));
    }
    c
}
async fn manager(conf: &ProviderSnapshot<()>, c: &BasicCluster) -> RuleManager {
    let mut m = RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        astersql_pd_keypath::ClusterPaths::new(61),
    );
    m.initialize(
        3,
        &[],
        "",
        false,
        conf,
        &c.stores
            .get_stores()
            .iter()
            .map(|s| (**s).clone())
            .collect::<Vec<_>>(),
    )
    .await
    .unwrap();
    m
}
fn config() -> ProviderSnapshot<()> {
    let mut conf = ProviderSnapshot::default();
    conf.replication.enable_placement_rules = false;
    conf.replication.max_replicas = 3;
    conf
}
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_061.json"
    ))
    .unwrap()
}
#[tokio::test(start_paused = true)]
async fn task_061_behavior_cases() {
    for case in fixture()["cases"].as_array().unwrap() {
        let c = cluster(case["pending"] == true);
        let conf = config();
        let ctl = ControllerAdmission::new(10);
        let mut counts = ScatterCounts::default();
        let mut mgr = manager(&conf, &c).await;
        let hot: BTreeSet<_> = if case["hot"] == true {
            [1].into()
        } else {
            BTreeSet::new()
        };
        let token = CancellationToken::new();
        if case["cancel"] == true {
            token.cancel();
        }
        let metrics = ScatterBatchMetrics::new(&prometheus::Registry::new()).unwrap();
        let mut batch = ScatterBatch {
            cluster: &c,
            config: &conf,
            rules: &mut mgr,
            controller: &ctl,
            counts: &mut counts,
            affinity: None,
            hot_regions: &hot,
            now_unix_nanos: 1_000_000_000,
            supports_conf_change_v2: false,
            metrics: &metrics,
        };
        let request = if case["range"] == true {
            ScatterBatchRequest::Range {
                start: vec![1],
                end: vec![3],
            }
        } else {
            ScatterBatchRequest::Ids(
                case["ids"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|id| id.as_u64().unwrap())
                    .collect(),
            )
        };
        let result = batch
            .scatter(
                request,
                "g",
                case["retry"].as_i64().unwrap() as i32,
                false,
                &token,
                || Ok(900),
            )
            .await;
        if let Some(error) = case["error"].as_str() {
            assert_eq!(format!("{:?}", result.unwrap_err()), error, "{case}");
        } else {
            let result = result.unwrap();
            assert_eq!(
                result.processed,
                case["processed"].as_u64().unwrap() as usize,
                "{case}"
            );
            assert_eq!(
                result.failures.keys().copied().collect::<Vec<_>>(),
                case["failures"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_u64().unwrap())
                    .collect::<Vec<_>>(),
                "{case}"
            );
            assert_eq!(
                result.pending.iter().copied().collect::<Vec<_>>(),
                case["pending_ids"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_u64().unwrap())
                    .collect::<Vec<_>>(),
                "{case}"
            );
            assert_eq!(result.cancelled, case["cancel"] == true, "{case}");
            assert_eq!(
                metrics.value("unnecessary", ""),
                case["unnecessary"].as_u64().unwrap() as f64
            );
        }
        assert_eq!(
            metrics.value("skip", "no-region"),
            case["missing"].as_f64().unwrap_or(0.0),
            "{case}"
        );
        assert_eq!(
            metrics.value("skip", "hot"),
            case["hot_attempts"].as_f64().unwrap_or(0.0),
            "{case}"
        );
        assert_eq!(counts.get("other", "tikv", 1, false), 0);
        println!("case {} passed", case["name"]);
    }
}
#[tokio::test(start_paused = true)]
async fn task_061_retry_allocation_and_controller_rejection() {
    for (fail_count, reject, skip, expected_processed, expected_failures) in [
        (1, false, true, 1, 0),
        (1, false, false, 1, 0),
        (9, false, true, 0, 1),
        (0, true, true, 1, 1),
    ] {
        let c = cluster(false);
        let conf = config();
        let ctl = ControllerAdmission::new(10);
        let mut mgr = manager(&conf, &c).await;
        let mut counts = ScatterCounts::default();
        for id in 1..=3 {
            counts.set("g", "tikv", id, false, 10);
        }
        let metrics = ScatterBatchMetrics::new(&prometheus::Registry::new()).unwrap();
        let token = CancellationToken::new();
        let hot = BTreeSet::new();
        let mut batch = ScatterBatch {
            cluster: &c,
            config: &conf,
            rules: &mut mgr,
            controller: &ctl,
            counts: &mut counts,
            affinity: None,
            hot_regions: &hot,
            now_unix_nanos: 1_000_000_000,
            supports_conf_change_v2: false,
            metrics: &metrics,
        };
        let mut attempts = 0;
        let start = tokio::time::Instant::now();
        let result = batch
            .scatter(
                ScatterBatchRequest::Ids(vec![1]),
                "g",
                2,
                skip,
                &token,
                || {
                    attempts += 1;
                    if attempts <= fail_count {
                        return Err(BuilderError::Allocation("injected I/O failure".into()));
                    }
                    if reject {
                        let existing = Arc::new(Operator::new(
                            "concurrent-admin",
                            "",
                            1,
                            None,
                            OpKind::ADMIN,
                            96,
                            vec![
                                TransferLeader {
                                    from_store: 1,
                                    to_store: 2,
                                    to_stores: vec![],
                                }
                                .into(),
                            ],
                        ));
                        existing.set_priority_level(PriorityLevel::URGENT);
                        ctl.add_operators(&c, &[existing]).unwrap();
                    }
                    Ok(900)
                },
            )
            .await
            .unwrap();
        assert_eq!(
            (result.processed, result.failures.len()),
            (expected_processed, expected_failures)
        );
        assert_eq!(
            attempts,
            if reject {
                1
            } else if fail_count == 1 {
                2
            } else {
                3
            }
        );
        assert_eq!(
            metrics.value("success", ""),
            if fail_count == 9 { 0.0 } else { 1.0 }
        );
        if fail_count == 9 {
            assert_eq!(
                tokio::time::Instant::now() - start,
                Duration::from_millis(700)
            );
            assert!(matches!(
                result.failures[&1],
                ScatterBatchFailure::Operator(ScatterOperatorError::Build(
                    BuilderError::Allocation(_)
                ))
            ));
        }
        if reject {
            assert!(matches!(
                result.failures[&1],
                ScatterBatchFailure::Admission(_)
            ));
            assert!(result.pending.is_empty());
            assert_eq!(ctl.get_operator(1).unwrap().desc(), "concurrent-admin");
        }
        if !reject && fail_count == 1 {
            let op = ctl.get_operator(1).unwrap();
            assert_eq!(op.additional_info().get("group").as_deref(), Some("g"));
            assert_eq!(op.priority_level(), PriorityLevel::HIGH);
            for step in (0..op.len()).filter_map(|i| op.step(i)) {
                if let astersql_pd_schedule_operator::OpStep::RemovePeer(step) = step {
                    assert_eq!(step.is_light_weight, skip);
                }
            }
            assert!(
                (0..op.len())
                    .filter_map(|i| op.step(i))
                    .any(|s| matches!(s, astersql_pd_schedule_operator::OpStep::AddLearner(_)))
            );
        }
    }
}
#[tokio::test(start_paused = true)]
async fn task_061_cancellation_during_backoff_and_retry_cap() {
    for cancel in [true, false] {
        let c = cluster(false);
        let conf = config();
        let ctl = ControllerAdmission::new(10);
        let mut mgr = manager(&conf, &c).await;
        let mut counts = ScatterCounts::default();
        let metrics = ScatterBatchMetrics::new(&prometheus::Registry::new()).unwrap();
        let hot = [1].into();
        let token = CancellationToken::new();
        let mut batch = ScatterBatch {
            cluster: &c,
            config: &conf,
            rules: &mut mgr,
            controller: &ctl,
            counts: &mut counts,
            affinity: None,
            hot_regions: &hot,
            now_unix_nanos: 1_000_000_000,
            supports_conf_change_v2: false,
            metrics: &metrics,
        };
        let cancel_wait = async {
            if cancel {
                tokio::time::sleep(Duration::from_millis(50)).await;
                token.cancel();
            }
        };
        let (result, ()) = tokio::join!(
            batch.scatter(
                ScatterBatchRequest::Ids(vec![1, 2]),
                "g",
                99,
                false,
                &token,
                || Ok(900)
            ),
            cancel_wait
        );
        let result = result.unwrap();
        assert_eq!(result.processed, 1);
        assert_eq!(result.pending, [1].into());
        assert_eq!(result.cancelled, cancel);
        assert_eq!(
            metrics.value("skip", "hot"),
            if cancel { 1.0 } else { 31.0 }
        );
        assert!(matches!(result.failures[&1], ScatterBatchFailure::Hot));
    }
}

#[tokio::test(start_paused = true)]
async fn task_061_existing_group_and_region_faults() {
    let mut c = cluster(false);
    let conf = config();
    let ctl = ControllerAdmission::new(10);
    let mut mgr = manager(&conf, &c).await;
    let hot = BTreeSet::new();
    let token = CancellationToken::new();
    let registry = prometheus::Registry::new();
    let metrics = ScatterBatchMetrics::new(&registry).unwrap();
    assert!(ScatterBatchMetrics::new(&registry).is_err());
    let mut counts = ScatterCounts::default();
    for id in 1..=3 {
        counts.set("g", "tikv", id, false, 10);
    }
    {
        let mut batch = ScatterBatch {
            cluster: &c,
            config: &conf,
            rules: &mut mgr,
            controller: &ctl,
            counts: &mut counts,
            affinity: None,
            hot_regions: &hot,
            now_unix_nanos: 1_000_000_000,
            supports_conf_change_v2: false,
            metrics: &metrics,
        };
        let first = batch
            .scatter(
                ScatterBatchRequest::Ids(vec![1]),
                "g",
                0,
                true,
                &token,
                || Ok(900),
            )
            .await
            .unwrap();
        assert_eq!(first.processed, 1);
        assert!(first.failures.is_empty());
        let distribution = batch.counts.clone();
        let repeated = batch
            .scatter(
                ScatterBatchRequest::Ids(vec![1]),
                "g",
                2,
                true,
                &token,
                || panic!("running operator must skip allocation"),
            )
            .await
            .unwrap();
        assert_eq!(repeated.processed, 1);
        assert!(repeated.failures.is_empty());
        assert_eq!(*batch.counts, distribution);
        let other = batch
            .scatter(
                ScatterBatchRequest::Ids(vec![1]),
                "other",
                1,
                true,
                &token,
                || Ok(900),
            )
            .await
            .unwrap();
        assert_eq!(other.processed, 0);
        assert_eq!(other.failures[&1], ScatterBatchFailure::ExistingOperator);
        assert_eq!(metrics.value("skip", "running"), 1.0);
        assert_eq!(metrics.value("fail", "other-existed"), 2.0);
    }
    let region = c.regions.get_region(1).unwrap();
    let changed = region.clone_with([astersql_pd_core::RegionOption::Version(1)]);
    c.regions.set_region(Arc::new(changed));
    let mut batch = ScatterBatch {
        cluster: &c,
        config: &conf,
        rules: &mut mgr,
        controller: &ctl,
        counts: &mut counts,
        affinity: None,
        hot_regions: &hot,
        now_unix_nanos: 1_000_000_000,
        supports_conf_change_v2: false,
        metrics: &metrics,
    };
    let mismatch = batch
        .scatter(
            ScatterBatchRequest::Ids(vec![1]),
            "g",
            0,
            true,
            &token,
            || Ok(900),
        )
        .await
        .unwrap();
    assert_eq!(mismatch.failures[&1], ScatterBatchFailure::ExistingOperator);
    // A pending peer alone is not an admin replication failure. Missing replica
    // and leader faults instead retain pending IDs and produce suspect markers.
    drop(batch);
    for (no_leader, expected) in [
        (true, ScatterBatchFailure::NoLeader),
        (false, ScatterBatchFailure::NotReplicated),
    ] {
        let original = c.regions.get_region(2).unwrap();
        let options = if no_leader {
            vec![astersql_pd_core::RegionOption::Leader(None)]
        } else {
            vec![astersql_pd_core::RegionOption::Peers(
                original.meta().peers[..2].to_vec(),
            )]
        };
        c.regions.set_region(Arc::new(original.clone_with(options)));
        let mut batch = ScatterBatch {
            cluster: &c,
            config: &conf,
            rules: &mut mgr,
            controller: &ctl,
            counts: &mut counts,
            affinity: None,
            hot_regions: &hot,
            now_unix_nanos: 1_000_000_000,
            supports_conf_change_v2: false,
            metrics: &metrics,
        };
        let result = batch
            .scatter(
                ScatterBatchRequest::Ids(vec![2]),
                "g",
                0,
                false,
                &token,
                || Ok(900),
            )
            .await
            .unwrap();
        assert_eq!(result.failures[&2], expected);
        assert_eq!(result.pending, [2].into());
        assert_eq!(
            result.suspect,
            if no_leader {
                BTreeSet::new()
            } else {
                [2].into()
            }
        );
    }
    let families = registry.gather();
    assert_eq!(families.len(), 1);
    assert_eq!(families[0].name(), "pd_schedule_scatter_operators_count");
    assert_eq!(metrics.value("skip", "no-leader"), 1.0);
    assert_eq!(metrics.value("skip", "not-replicated"), 1.0);
}
