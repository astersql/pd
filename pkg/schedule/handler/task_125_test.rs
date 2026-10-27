// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_cache::{TTLUint64, TtlDuration};
use astersql_pd_core::{
    BasicCluster, KeyRange, Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta, StoreStats,
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule::{Coordinator, CoordinatorInputs};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_hbstream::routing::StreamRouter;
use astersql_pd_schedule_scatter::ScatterBatchMetrics;
use astersql_pd_schedule_schedulers::{FactoryContext, JobTime};
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
fn setup(
    config: serde_json::Value,
    backend: Arc<dyn astersql_pd_storage_kv::MetaStore>,
) -> (Coordinator<ProviderSnapshot<()>>, Arc<TTLUint64<()>>) {
    let mut cluster = BasicCluster::new();
    let peers: Vec<_> = (1..=3)
        .map(|id| Peer {
            id,
            store_id: id,
            role: if id == 3 && config["learner"].as_bool().unwrap_or(false) {
                astersql_pd_core::PeerRole::Learner
            } else {
                astersql_pd_core::PeerRole::Voter
            },
            is_witness: id == 3 && config["witness"].as_bool().unwrap_or(false),
            ..Default::default()
        })
        .collect();
    let region = Arc::new(RegionInfo::new(
        RegionMeta {
            id: 1,
            peers: peers.clone(),
            key_range: KeyRange {
                start_key: b"a".to_vec(),
                end_key: b"b".to_vec(),
            },
            epoch: Some(astersql_pd_core::RegionEpoch {
                conf_ver: 1,
                version: 2,
            }),
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![],
    ));
    let mut region = (*region).clone();
    region.stats_mut().approximate_size = 96;
    let region = altered(&region, config["source"].as_str().unwrap_or(""));
    let (origin, overlaps, changed) = cluster.regions.set_region(region.clone());
    cluster
        .regions
        .update_sub_tree(region, origin.as_deref(), &overlaps, changed);
    for id in 2..=3 {
        let mut meta = cluster.regions.get_region(1).unwrap().meta().clone();
        meta.id = id;
        meta.key_range.start_key = vec![b'a' + id as u8 - 1];
        meta.key_range.end_key = if id == 3 {
            vec![]
        } else {
            vec![b'a' + id as u8]
        };
        meta.epoch = Some(astersql_pd_core::RegionEpoch {
            conf_ver: id,
            version: id + 1,
        });
        for peer in &mut meta.peers {
            peer.id += id * 10;
        }
        let region = RegionInfo::new(meta.clone(), Some(meta.peers[0].clone()), vec![], vec![]);
        cluster.regions.set_region(altered(
            &region,
            if id == 2 {
                config["target"].as_str().unwrap_or("")
            } else {
                ""
            },
        ));
    }
    let cluster = Arc::new(cluster);
    for id in 1..=4 {
        let mut store = StoreInfo::new(StoreMeta {
            id,
            last_heartbeat: 1_000_000_000,
            ..Default::default()
        });
        store.update_raw_stats(StoreStats {
            capacity: 1_000_000_000_000,
            available: 900_000_000_000,
            used_size: 100_000_000_000,
            ..Default::default()
        });
        cluster.stores.put_store(Arc::new(store));
    }
    let (heartbeat, _router) = StreamRouter::new(Arc::new(astersql_pd_core::StoresInfo::new()));
    let pending = Arc::new(TTLUint64::new(
        tokio::runtime::Handle::current(),
        CancellationToken::new(),
        Duration::from_secs(60),
        TtlDuration::from_nanos(180_000_000_000),
    ));
    let rules = config["rules"].as_bool().unwrap_or(false);
    let c = Coordinator::new(CoordinatorInputs {
        cluster,
        config: Arc::new({
            let mut config = ProviderSnapshot::<()>::default();
            config.schedule.scheduler_max_waiting_operator = 10;
            config.replication.enable_placement_rules = rules;
            config.replication.max_replicas = 3;
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
        scheduler_configuration: config,
        scatter_metrics: ScatterBatchMetrics::new(&prometheus::Registry::new()).unwrap(),
        start: Duration::ZERO,
        now_unix_nanos: 1_000_000_000,
        split_merge_interval_nanos: 0,
        total_regions: Box::new(|| Ok(0)),
        unix_seconds: Arc::new(|| 0),
    });
    (c, pending)
}

use astersql_pd_schedule_placement::RuleManager;
use std::time::Instant;
fn manager() -> RuleManager {
    RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        ClusterPaths::new(120),
    )
}
fn context(manager: &mut RuleManager) -> AdminRangeContext<'_> {
    AdminRangeContext {
        manager,
        now_unix_nanos: 1_000_000_000,
        supports_conf_change_v2: false,
        now: Instant::now(),
    }
}
fn altered(r: &RegionInfo, mode: &str) -> Arc<RegionInfo> {
    let mut meta = r.meta().clone();
    let mut down = vec![];
    let mut pending = vec![];
    match mode {
        "pending" => pending.push(meta.peers[1].clone()),
        "down" => down.push(astersql_pd_core::DownPeer {
            peer: meta.peers[1].clone(),
            down_seconds: 0,
        }),
        "under" => {
            meta.peers.pop();
        }
        "over" => meta.peers.push(Peer {
            id: 100,
            store_id: 4,
            ..Default::default()
        }),
        "learner" => meta.peers[2].role = astersql_pd_core::PeerRole::Learner,
        "joint" => meta.peers[2].role = astersql_pd_core::PeerRole::IncomingVoter,
        "mismatch" => meta.peers[2].store_id = 4,
        "first" => meta.key_range.start_key.clear(),
        _ => {}
    }
    Arc::new(RegionInfo::new(
        meta.clone(),
        if mode == "leaderless" {
            None
        } else {
            Some(meta.peers[0].clone())
        },
        down,
        pending,
    ))
}

fn scatter_context<'a>(
    m: &'a mut RuleManager,
    hot: &'a std::collections::BTreeSet<u64>,
    cancel: &'a CancellationToken,
) -> CreationContext<'a> {
    CreationContext {
        range: context(m),
        hot,
        affinity: None,
        cancellation: cancel,
    }
}
#[test]
fn task_125_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_125.json"
    ))
    .unwrap();
    for row in fixture["percentages"].as_array().unwrap() {
        let failures = (0..row[1].as_u64().unwrap())
            .map(|id| {
                (
                    id,
                    astersql_pd_schedule_scatter::ScatterBatchFailure::MissingRegion,
                )
            })
            .collect();
        assert_eq!(
            build_scatter_regions_resp(row[0].as_u64().unwrap() as usize, &failures)
                .processed_percentage,
            row[2].as_u64().unwrap() as usize
        );
    }
}
#[tokio::test(start_paused = true)]
async fn task_125_batch_selection_partial_and_retry() {
    for (ids, start, end, retry, processed, failed, percent) in [
        (vec![1, 99, 99], "", "", 0, 1, vec![99], 50),
        (vec![99], "61", "63", 0, 2, vec![], 100),
        (vec![1], "invalid", "", 0, 1, vec![], 100),
        (vec![1, 1], "", "invalid", 0, 1, vec![], 100),
        (vec![1], "", "", -1, 0, vec![], 100),
        (vec![1, 2], "", "", 1, 1, vec![2], 50),
    ] {
        let (mut c, pending) = setup(serde_json::json!({}), Arc::new(MemoryStore::default()));
        let mut m = manager();
        m.initialize(3, &[], "", false, &**c.get_config(), &[])
            .await
            .unwrap();
        let hot = if retry == 1 {
            [2].into()
        } else {
            Default::default()
        };
        let cancel = CancellationToken::new();
        let outcome = add_scatter_regions_operators(
            Some(&mut c),
            &ids,
            start,
            end,
            "g",
            retry,
            scatter_context(&mut m, &hot, &cancel),
            || Ok(900),
        )
        .await
        .unwrap();
        assert_eq!(outcome.processed, processed);
        assert_eq!(outcome.failures.keys().copied().collect::<Vec<_>>(), failed);
        assert_eq!(
            build_scatter_regions_resp(outcome.processed, &outcome.failures).processed_percentage,
            percent
        );
        if retry == 1 {
            assert_eq!(
                outcome.failures[&2],
                astersql_pd_schedule_scatter::ScatterBatchFailure::Hot
            );
        }
        pending.shutdown().await;
    }
}
#[tokio::test(start_paused = true)]
async fn task_125_errors_single_and_bootstrap() {
    let (mut c, pending) = setup(serde_json::json!({}), Arc::new(MemoryStore::default()));
    let mut m = manager();
    m.initialize(3, &[], "", false, &**c.get_config(), &[])
        .await
        .unwrap();
    let hot = [1].into();
    let cancel = CancellationToken::new();
    for id in [1, 99] {
        let e = add_scatter_region_operator(
            Some(&mut c),
            id,
            "g",
            scatter_context(&mut m, &hot, &cancel),
            || panic!(),
        )
        .await
        .unwrap_err();
        assert_eq!(
            e.to_string(),
            if id == 1 {
                "region 1 is a hot region"
            } else {
                "region 99 not found"
            }
        );
    }
    for (ids, start, end, expected) in [
        (vec![], "", "", "scatter batch error: EmptyRegion"),
        (vec![99], "", "", "scatter batch error: RegionNotFound(99)"),
        (vec![98, 99], "", "", "scatter batch error: EmptyRegion"),
        (
            vec![1],
            "zz",
            "62",
            "encoding/hex: invalid byte: U+007A 'z'",
        ),
        (vec![1], "61", "0", "encoding/hex: odd length hex string"),
        (vec![1], "00", "01", "scatter batch error: EmptyRegion"),
    ] {
        assert_eq!(
            add_scatter_regions_operators(
                Some(&mut c),
                &ids,
                start,
                end,
                "g",
                0,
                scatter_context(&mut m, &hot, &cancel),
                || panic!()
            )
            .await
            .unwrap_err()
            .to_string(),
            expected
        );
    }
    assert!(matches!(
        add_scatter_regions_operators::<ProviderSnapshot<()>, _>(
            None,
            &[],
            "zz",
            "zz",
            "",
            0,
            scatter_context(&mut m, &hot, &cancel),
            || panic!()
        )
        .await,
        Err(CreationError::Handler(HandlerError::NotBootstrapped))
    ));
    assert!(matches!(
        scatter_regions_by_range::<ProviderSnapshot<()>, _>(
            None,
            "z",
            "",
            "",
            0,
            scatter_context(&mut m, &hot, &cancel),
            || panic!()
        )
        .await,
        Err(CreationError::Scatter(_))
    ));
    assert!(matches!(
        scatter_regions_by_id::<ProviderSnapshot<()>, _>(
            None,
            &[],
            "",
            0,
            scatter_context(&mut m, &hot, &cancel),
            || panic!()
        )
        .await,
        Err(CreationError::Handler(HandlerError::NotBootstrapped))
    ));
    pending.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn task_125_allocation_faults_and_single_admission() {
    use astersql_pd_core_constant::PriorityLevel;
    use astersql_pd_schedule_operator::{BuilderError, OpStep};
    for (single, fail_count) in [(false, 1), (false, 9), (true, 0), (true, 9)] {
        let (mut c, pending) = setup(serde_json::json!({}), Arc::new(MemoryStore::default()));
        let mut m = manager();
        m.initialize(3, &[], "", false, &**c.get_config(), &[])
            .await
            .unwrap();
        let hot = Default::default();
        let cancel = CancellationToken::new();
        {
            let scatter = c.get_region_scatterer(&mut m, &hot, None, 1_000_000_000, false);
            for id in 1..=3 {
                scatter.batch.counts.set("g", "tikv", id, false, 10);
            }
        }
        let mut attempts = 0;
        let mut allocate = || {
            attempts += 1;
            if attempts <= fail_count {
                Err(BuilderError::Allocation("allocator unavailable".into()))
            } else {
                Ok(900)
            }
        };
        if single {
            let result = add_scatter_region_operator(
                Some(&mut c),
                1,
                "g",
                scatter_context(&mut m, &hot, &cancel),
                &mut allocate,
            )
            .await;
            if fail_count > 0 {
                assert!(matches!(
                    result,
                    Err(CreationError::ScatterFailure(
                        astersql_pd_schedule_scatter::ScatterBatchFailure::Operator(_)
                    ))
                ));
            } else {
                result.unwrap();
            }
            assert_eq!(attempts, 1);
        } else {
            let start = tokio::time::Instant::now();
            let outcome = scatter_regions_by_id(
                Some(&mut c),
                &[1],
                "g",
                2,
                scatter_context(&mut m, &hot, &cancel),
                &mut allocate,
            )
            .await
            .unwrap();
            assert_eq!(attempts, if fail_count == 1 { 2 } else { 3 });
            assert_eq!(outcome.processed, if fail_count == 1 { 1 } else { 0 });
            assert_eq!(outcome.failures.len(), if fail_count == 1 { 0 } else { 1 });
            assert_eq!(
                build_scatter_regions_resp(outcome.processed, &outcome.failures)
                    .processed_percentage,
                if fail_count == 1 { 100 } else { 0 }
            );
            if fail_count == 9 {
                assert_eq!(outcome.pending, [1].into());
                assert!(matches!(
                    outcome.failures[&1],
                    astersql_pd_schedule_scatter::ScatterBatchFailure::Operator(_)
                ));
                assert_eq!(
                    tokio::time::Instant::now() - start,
                    Duration::from_millis(700)
                );
            }
        }
        if fail_count < 2 {
            let op = c.get_operator_controller().get_operator(1).unwrap();
            assert_eq!(op.additional_info().get("group").as_deref(), Some("g"));
            assert_eq!(op.priority_level(), PriorityLevel::HIGH);
            assert_eq!(
                op.region_epoch(),
                c.get_cluster()
                    .regions
                    .get_region(1)
                    .unwrap()
                    .meta()
                    .epoch
                    .as_ref()
            );
            assert!((0..op.len()).any(|i|matches!(op.step(i),Some(OpStep::AddLearner(s)) if s.peer_id==900 && s.to_store==4)));
            for i in 0..op.len() {
                if let Some(OpStep::RemovePeer(s)) = op.step(i) {
                    assert!(!s.is_light_weight);
                }
            }
            let repeated = add_scatter_region_operator(
                Some(&mut c),
                1,
                "g",
                scatter_context(&mut m, &hot, &cancel),
                || panic!("same group must be a no-op"),
            )
            .await;
            repeated.unwrap();
            assert!(Arc::ptr_eq(
                &op,
                &c.get_operator_controller().get_operator(1).unwrap()
            ));
        } else {
            assert!(c.get_operator_controller().operators().is_empty());
        }
        pending.shutdown().await;
    }
}

#[tokio::test(start_paused = true)]
async fn task_125_explicit_range_retry_cap_and_cancellation() {
    for retries in [i64::MIN, 31, i64::MAX] {
        let (mut c, pending) = setup(serde_json::json!({}), Arc::new(MemoryStore::default()));
        let mut m = manager();
        m.initialize(3, &[], "", false, &**c.get_config(), &[])
            .await
            .unwrap();
        let hot = [1, 2, 3].into();
        let cancel = CancellationToken::new();
        let outcome = scatter_regions_by_range(
            Some(&mut c),
            "",
            "",
            "g",
            retries,
            scatter_context(&mut m, &hot, &cancel),
            || panic!(),
        )
        .await
        .unwrap();
        assert_eq!(outcome.processed, 0);
        assert_eq!(outcome.failures.len(), if retries < 0 { 0 } else { 3 });
        assert_eq!(outcome.pending, [1, 2, 3].into());
        cancel.cancel();
        let cancelled = scatter_regions_by_id(
            Some(&mut c),
            &[1, 99],
            "g",
            0,
            scatter_context(&mut m, &hot, &cancel),
            || panic!(),
        )
        .await
        .unwrap();
        assert!(cancelled.cancelled);
        assert_eq!(cancelled.processed, 0);
        assert_eq!(
            cancelled.failures[&99],
            astersql_pd_schedule_scatter::ScatterBatchFailure::MissingRegion
        );
        assert_eq!(
            cancelled.failures[&1],
            astersql_pd_schedule_scatter::ScatterBatchFailure::Cancelled
        );
        pending.shutdown().await;
    }
}
