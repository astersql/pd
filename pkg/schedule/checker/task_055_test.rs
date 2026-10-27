// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_cache::{TTLUint64, TtlDuration};
use astersql_pd_core::{
    BasicCluster, KeyRange, Peer, PeerRole, RegionEpoch, RegionInfo, RegionMeta, StoreInfo,
    StoreMeta, StoreStats,
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_affinity::{group::Group, manager::Manager, ranges::GroupKeyRanges};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_labeler::{LabelRule, RegionLabeler};
use astersql_pd_schedule_operator::{BuilderError, ControllerAdmission, OpStep};
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_storage_domain::MemoryStore;
use astersql_pd_storage_domain::StorageEndpoint;
use serde_json::{Value, json};
use std::{collections::BTreeSet, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
const NOW: i128 = 100_000_000_000;
fn pending() -> Arc<TTLUint64<()>> {
    Arc::new(TTLUint64::new(
        tokio::runtime::Handle::current(),
        CancellationToken::new(),
        Duration::from_secs(60),
        TtlDuration::from_nanos(180_000_000_000),
    ))
}
fn region(id: u64, start: u8, end: u8, c: &Value) -> Arc<RegionInfo> {
    let peers: Vec<_> = (1..=c["peers"].as_u64().unwrap_or(3))
        .map(|s| Peer {
            id: id * 10 + s,
            store_id: s,
            role: if s == 3 && c["joint"] == true {
                PeerRole::DemotingVoter
            } else if s == 3 && c["learner"] == true {
                PeerRole::Learner
            } else {
                PeerRole::Voter
            },
            ..Default::default()
        })
        .collect();
    let mut r = RegionInfo::new(
        RegionMeta {
            id,
            key_range: KeyRange {
                start_key: vec![start],
                end_key: vec![end],
            },
            peers: peers.clone(),
            epoch: Some(RegionEpoch {
                version: 9,
                conf_ver: 7,
            }),
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![],
    );
    r.stats_mut().approximate_size = 1;
    r.stats_mut().approximate_keys = 1;
    Arc::new(r)
}
async fn evaluate(
    c: &Value,
) -> (
    CheckerDispatchOutcome,
    CheckerDispatcher,
    Arc<TTLUint64<()>>,
) {
    let mut conf = ProviderSnapshot::<()>::default();
    conf.replication.max_replicas = 3;
    conf.replication.enable_placement_rules = c["rules"] == true;
    conf.replication.enable_placement_rules_cache = c["cache"] == true;
    conf.schedule.enable_make_up_replica = true;
    conf.schedule.enable_remove_extra_replica = true;
    conf.schedule.replica_schedule_limit = c["replica_limit"].as_u64().unwrap_or(10);
    conf.schedule.affinity_schedule_limit = c["affinity_limit"].as_u64().unwrap_or(10);
    conf.schedule.merge_schedule_limit = c["merge_limit"].as_u64().unwrap_or(10);
    conf.schedule.max_merge_region_size = 10;
    conf.schedule.max_merge_region_keys = 10000;
    conf.schedule.max_affinity_merge_region_size = 10;
    conf.schedule.split_merge_interval = "0s".parse().unwrap();
    conf.schedule.max_store_down_time = "30m".parse().unwrap();
    conf.schedule.max_snapshot_count = 64;
    conf.schedule.high_space_ratio = 0.7;
    conf.schedule.low_space_ratio = 0.8;
    conf.key_type = astersql_pd_core_constant::KeyType::RAW;
    let mut cluster = BasicCluster::new();
    for id in 1..=c["stores"].as_u64().unwrap_or(3) {
        let mut s = StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        });
        s.set_last_heartbeat_unix_nanos(NOW as i64);
        s.update_raw_stats(StoreStats {
            capacity: 1_000_000_000_000,
            available: 900_000_000_000,
            used_size: 100_000_000_000,
            is_busy: c["busy"] == true && id > 2,
            ..Default::default()
        });
        cluster.stores.put_store(Arc::new(s));
    }
    let storage = Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default())));
    let paths = ClusterPaths::new(55);
    let mut labels = RegionLabeler::load(
        StorageEndpoint::new(Arc::new(MemoryStore::default())),
        paths.clone(),
        0,
    )
    .await
    .unwrap();
    let mut affinity = Manager::default();
    if c["affinity"] == true {
        affinity
            .create_affinity_groups(
                &storage,
                &paths,
                &mut labels,
                &[GroupKeyRanges {
                    group_id: "g".into(),
                    key_ranges: vec![astersql_pd_keyutil::KeyRange::new(vec![1], vec![9])],
                }],
                0,
            )
            .await
            .unwrap();
        affinity.sync_group_from_etcd(
            &Group {
                id: "g".into(),
                leader_store_id: 2,
                voter_store_ids: vec![1, 2, 3],
                ..Default::default()
            },
            &labels,
            0,
        );
        affinity.restore_affinity_group("g", 0);
    }
    if c["deny"] == true || c["split"] == true {
        let label = LabelRule::from_json(&serde_json::to_vec(&json!({"id":"label","labels":[{"key":"schedule","value":if c["deny"]==true {"deny"}else{"allow"}}],"rule_type":"key-range","data":[{"start_key":if c["split"]==true {"02"}else{"01"},"end_key":"09"}]})).unwrap(),0).unwrap();
        labels.set_label_rule(label, 0).await.unwrap();
    }
    let mut rules = RuleManager::new(storage.clone(), paths.clone());
    rules
        .initialize(3, &[], "", false, &conf, &[])
        .await
        .unwrap();
    let mut r = region(1, 1, 3, c);
    cluster.regions.set_region(r.clone());
    if c["neighbor"] == true {
        cluster.regions.set_region(region(2, 3, 5, &json!({})));
    }
    if c["cache"] == true {
        for _ in 0..11 {
            let fit = rules.fit_region(&cluster.stores.get_stores(), &r, &conf, NOW);
            rules.set_region_fit_cache(&cluster.stores.get_stores(), &r, fit, NOW);
        }
        assert!(rules.is_region_fit_cached(&cluster.stores.get_stores(), &r, NOW));
        if c["invalidate"] == true {
            rules.invalid_cache(1);
        }
    }
    if c["stale_epoch"] == true {
        let mut meta = r.meta().clone();
        meta.epoch.as_mut().unwrap().conf_ver += 1;
        r = Arc::new(RegionInfo::new(meta, r.leader().cloned(), vec![], vec![]));
        cluster.regions.set_region(r.clone());
    }
    let ops = ControllerAdmission::new(c["queue_limit"].as_u64().unwrap_or(10));
    if let Some(kind) = c["running_kind"].as_str() {
        use astersql_pd_schedule_operator::{OpKind, Operator, TransferLeader};
        let other = region(3, 5, 9, &json!({}));
        cluster.regions.set_region(other.clone());
        let kind = match kind {
            "replica" => OpKind::REPLICA,
            "affinity" => OpKind::AFFINITY,
            _ => OpKind::MERGE,
        };
        let op = Arc::new(Operator::new(
            "running",
            "",
            3,
            other.meta().epoch,
            kind,
            1,
            vec![
                TransferLeader {
                    from_store: 1,
                    to_store: 2,
                    ..Default::default()
                }
                .into(),
            ],
        ));
        ops.add_operators(&cluster, &[op]).unwrap();
    }
    let hot = BTreeSet::new();
    let p = pending();
    let mut ctl = CheckerDispatcher::new(p.clone(), 0, 0);
    if let Some(paused) = c["paused"].as_str() {
        ctl.pause_controller(paused)
            .unwrap()
            .pause_or_resume_at(200, 0);
    }
    let ctx = CheckerDispatchContext {
        affinity: AffinityContext {
            rules: RuleMissingContext {
                replica: ReplicaFaultContext {
                    config: &conf,
                    stores: &cluster.stores,
                    rules: &[],
                    now_unix_nanos: NOW,
                    supports_conf_change_v2: false,
                },
                witness_enabled: false,
            },
            labels: &labels,
            storage: &storage,
            paths: &paths,
            region_exists: true,
        },
        cluster: &cluster,
        operators: &ops,
        hot_regions: &hot,
    };
    if c["queue_noop"] == true {
        ctl.add_pending_processed_regions(true, &[1]);
        let result = ctl
            .try_add_operators(&ctx, &mut rules, &mut affinity, Some(&r), &mut || Ok(1000))
            .await;
        assert!(result.queued.is_empty());
        assert_eq!(ctl.pending_processed_regions(), vec![1]);
        return (CheckerDispatchOutcome::default(), ctl, p);
    }
    if c["queue"] == true {
        ctl.add_pending_processed_regions(true, &[1]);
        assert!(
            ctl.try_add_operators(&ctx, &mut rules, &mut affinity, None, &mut || Ok(1000))
                .await
                .queued
                .is_empty()
        );
        assert_eq!(ctl.pending_processed_regions(), vec![1]);
        let queued = ctl
            .try_add_operators(&ctx, &mut rules, &mut affinity, Some(&r), &mut || Ok(1000))
            .await;
        assert_eq!(queued.admission_error.is_some(), c["queue_limit"] == 0);
        assert!(!queued.store_limited);
        assert!(ctl.pending_processed_regions().is_empty());
        if c["queue_limit"] != 0 {
            assert_eq!(queued.queued.len(), 1);
            let admitted = ops.promote_waiting(&cluster, 0.0).unwrap().unwrap();
            assert_eq!(admitted.admitted[0].desc(), "promote-learner");
            ctl.add_pending_processed_regions(true, &[1]);
            assert!(
                ctl.try_add_operators(&ctx, &mut rules, &mut affinity, Some(&r), &mut || panic!(
                    "existing operator checked"
                ))
                .await
                .queued
                .is_empty()
            );
            assert!(ctl.pending_processed_regions().is_empty());
        }
        return (CheckerDispatchOutcome::default(), ctl, p);
    }
    let mut out = ctl
        .check_region(&ctx, &mut rules, &mut affinity, &r, &mut || {
            if c["allocation_error"] == true {
                Err(BuilderError::Allocation("injected".into()))
            } else {
                Ok(1000)
            }
        })
        .await;
    if c["cache"] == true && c["split"] != true {
        assert_eq!(
            out.rule_cache_hit,
            c["invalidate"] != true && c["stale_epoch"] != true
        );
    }
    if c["temporary_transition"] == true {
        assert!(ctl.is_pending_region(1));
        let mut store = StoreInfo::new(StoreMeta {
            id: 3,
            ..Default::default()
        });
        store.set_last_heartbeat_unix_nanos(NOW as i64);
        store.update_raw_stats(StoreStats {
            capacity: 1_000_000_000_000,
            available: 900_000_000_000,
            used_size: 100_000_000_000,
            is_busy: true,
            ..Default::default()
        });
        cluster.stores.put_store(Arc::new(store));
        out = ctl
            .check_region(&ctx, &mut rules, &mut affinity, &r, &mut || Ok(1000))
            .await;
    }
    if c["recover"] == true {
        assert!(ctl.is_pending_region(1));
        let mut store = StoreInfo::new(StoreMeta {
            id: 3,
            ..Default::default()
        });
        store.set_last_heartbeat_unix_nanos(NOW as i64);
        store.update_raw_stats(StoreStats {
            capacity: 1_000_000_000_000,
            available: 900_000_000_000,
            used_size: 100_000_000_000,
            ..Default::default()
        });
        cluster.stores.put_store(Arc::new(store));
        out = ctl
            .check_region(&ctx, &mut rules, &mut affinity, &r, &mut || Ok(1000))
            .await;
    }
    (out, ctl, p)
}
#[tokio::test]
async fn task_055_behavior_cases() {
    let f: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_055.json"
    ))
    .unwrap();
    for c in f["cases"].as_array().unwrap() {
        let (out, ctl, p) = evaluate(c).await;
        assert_eq!(
            out.operators.first().map(|o| o.desc()),
            c["expected"].as_str(),
            "{}",
            c["name"]
        );
        assert_eq!(
            out.operators.len(),
            c["count"]
                .as_u64()
                .unwrap_or(if c["expected"].is_string() { 1 } else { 0 }) as usize,
            "{} count",
            c["name"]
        );
        assert_eq!(
            ctl.pending_processed_regions().contains(&1),
            c["pending"] == true,
            "{} pending",
            c["name"]
        );
        assert_eq!(
            ctl.is_pending_region(1),
            c["rule_pending"] == true,
            "{} rule pending",
            c["name"]
        );
        assert_eq!(
            !out.errors.is_empty(),
            c["error"] == true,
            "{} errors: {:?}",
            c["name"],
            out.errors
        );
        if c["expected"] == "make-up-replica" || c["expected"] == "add-rule-peer" {
            assert!(
                matches!(out.operators[0].step(0),Some(OpStep::AddLearner(step)) if step.to_store==3 && step.peer_id==1000)
            );
            assert_eq!(
                out.operators[0].priority_level(),
                astersql_pd_core_constant::PriorityLevel::HIGH
            );
        }
        if c["count"] == 2 {
            assert_eq!(out.operators[0].region_id(), 1);
            assert_eq!(out.operators[1].region_id(), 2);
        }
        if c["allocation_error"] == true && c["error"] == true {
            let error = BuilderError::Allocation("injected".into());
            assert_eq!(
                out.errors,
                vec![if c["rules"] == true {
                    DispatchError::Missing(RuleMissingError::Builder(error))
                } else {
                    DispatchError::Builder(error)
                }]
            );
        }
        for op in &out.operators {
            use astersql_pd_schedule_operator::OpKind;
            let mask = match op.desc() {
                "labeler-split-region" => OpKind::SPLIT,
                "make-up-replica" | "add-rule-peer" => OpKind::REPLICA,
                "affinity-move-region" => OpKind::AFFINITY,
                "merge-region" => OpKind::MERGE,
                _ => OpKind(0),
            };
            assert_eq!(op.kind().0 & mask.0, mask.0);
            assert_eq!(
                op.region_epoch(),
                Some(&RegionEpoch {
                    version: 9,
                    conf_ver: if c["stale_epoch"] == true { 8 } else { 7 }
                })
            );
        }
        if c["expected"] == "promote-learner" {
            assert!(matches!(
                out.operators[0].step(0),
                Some(OpStep::PromoteLearner(_))
            ));
        }
        p.shutdown().await;
    }
    println!(
        "task_055: {} frozen dispatch cases",
        f["cases"].as_array().unwrap().len()
    );
}

#[tokio::test(start_paused = true)]
async fn task_055_pending_capacity_ttl_and_pause() {
    let p = pending();
    let ctl = CheckerDispatcher::new(p.clone(), 0, 0);
    for name in [
        "joint-state",
        "split",
        "learner",
        "replica",
        "rule",
        "affinity",
        "merge",
    ] {
        let pause = ctl.pause_controller(name).unwrap();
        pause.pause_or_resume_at(2, 0);
        assert!(pause.is_paused_at(1));
        assert!(!pause.is_paused_at(2));
        pause.pause_or_resume_at(0, 0);
        assert!(!pause.is_paused_at(0));
    }
    assert!(ctl.pause_controller("unknown").is_none());
    let ids: Vec<_> = (0..100_002).collect();
    ctl.add_pending_processed_regions(true, &ids);
    assert_eq!(p.len(), 100_001);
    assert!(!p.exists(&100_001));
    // Even replacement is rejected when physical length already exceeds the limit.
    ctl.add_pending_processed_regions(true, &[0]);
    ctl.remove_pending_processed_region(0);
    assert_eq!(p.len(), 100_000);
    ctl.add_pending_processed_regions(true, &[100_001]);
    assert_eq!(p.len(), 100_001);
    ctl.add_pending_processed_regions(false, &[100_002, 100_003]);
    assert_eq!(p.len(), 100_003);
    ctl.clear_pending_processed_regions();
    assert!(p.is_empty());
    ctl.add_pending_processed_regions(true, &[7]);
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(179)).await;
    assert_eq!(ctl.pending_processed_regions(), vec![7]);
    tokio::time::advance(Duration::from_secs(1)).await;
    assert!(ctl.pending_processed_regions().is_empty());
    // TTL Get accepts equality even though enumeration does not.
    assert!(p.exists(&7));
    p.shutdown().await;
    println!(
        "task_055: capacity=100001 strict boundary, unchecked insertion, remove/clear, exact 180s TTL, 7 pause names"
    );
}
#[tokio::test]
async fn task_055_queue_rejection_existing_and_absent() {
    for limit in [0, 10] {
        let (_, _, p) = evaluate(&json!({"learner":true,"queue":true,"queue_limit":limit})).await;
        p.shutdown().await;
    }
    println!(
        "task_055: real waiting enqueue/reject, promotion, existing/absent-region pending transitions"
    );
}
#[tokio::test]
async fn task_055_success_callback() {
    use astersql_pd_schedule_operator::{ControllerRecords, OpKind, Operator, TransferLeader};
    use std::{sync::Mutex, time::Instant};
    let p = pending();
    let owner = Arc::new(Mutex::new(CheckerDispatcher::new(p.clone(), 0, 0)));
    let records = ControllerRecords::default().with_success_callbacks(vec![Some(
        CheckerDispatcher::success_callback(Arc::downgrade(&owner), || (10, 100)),
    )]);
    let op = Operator::new(
        "affinity-merge-region",
        "",
        1,
        None,
        OpKind::AFFINITY,
        1,
        vec![
            TransferLeader {
                from_store: 1,
                to_store: 2,
                ..Default::default()
            }
            .into(),
        ],
    );
    op.additional_info()
        .set(astersql_pd_schedule_operator::RELATED_MERGE_REGION, "2");
    let op = Arc::new(op);
    // Planned and canceled operators must not populate the recent-merge cache.
    owner.lock().unwrap().record_op_success(&op, 10, 100);
    assert!(!owner.lock().unwrap().recently_affinity_merged(1, 100));
    assert!(op.start());
    let r = region(1, 1, 3, &json!({}));
    let heartbeat = RegionInfo::new(
        r.meta().clone(),
        Some(r.meta().peers[1].clone()),
        vec![],
        vec![],
    );
    assert!(op.check(&heartbeat).is_none());
    assert!(records.bury_operator(op.clone(), Instant::now()).is_some());
    assert!(records.bury_operator(op, Instant::now()).is_none());
    for id in [1, 2] {
        assert!(owner.lock().unwrap().recently_affinity_merged(id, 160));
        assert!(!owner.lock().unwrap().recently_affinity_merged(id, 161));
    }
    let canceled = Arc::new(Operator::new(
        "cancel",
        "",
        3,
        None,
        OpKind::AFFINITY,
        1,
        vec![],
    ));
    canceled
        .additional_info()
        .set(astersql_pd_schedule_operator::RELATED_MERGE_REGION, "4");
    canceled.cancel();
    records.bury_operator(canceled, Instant::now());
    assert!(!owner.lock().unwrap().recently_affinity_merged(3, 100));
    p.shutdown().await;
    println!(
        "task_055: canonical success callback, both IDs, duplicate burial, canceled exclusion, 60s inclusive TTL"
    );
}

#[tokio::test]
async fn task_055_store_budget_after_planning() {
    use astersql_pd_schedule_operator::{AddPeer, OpKind, Operator};
    let mut cluster = BasicCluster::new();
    for id in [1, 2] {
        let mut r = (*region(id, id as u8, id as u8 + 1, &json!({}))).clone();
        r.stats_mut().approximate_size = 96;
        cluster.regions.set_region(Arc::new(r));
    }
    cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 8,
        ..Default::default()
    })));
    let mut conf = ProviderSnapshot::<()>::default();
    conf.schedule.default_store_limit.add_peer = 0.000001;
    let operation = |id| {
        Operator::new(
            "planned-add",
            "",
            id,
            Some(RegionEpoch {
                version: 9,
                conf_ver: 7,
            }),
            OpKind::REGION,
            96,
            vec![
                AddPeer {
                    to_store: 8,
                    peer_id: 99,
                    ..Default::default()
                }
                .into(),
            ],
        )
    };
    let admission = ControllerAdmission::new(10);
    // Another region consumes the real v1 bucket between planning and enqueue.
    admission
        .add_limited_operators(&cluster, &conf, &[Arc::new(operation(1))])
        .await
        .unwrap();
    let storage = Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default())));
    let paths = ClusterPaths::new(55);
    let labels = RegionLabeler::load(
        StorageEndpoint::new(Arc::new(MemoryStore::default())),
        paths.clone(),
        0,
    )
    .await
    .unwrap();
    let hot = BTreeSet::new();
    let ctx = CheckerDispatchContext {
        affinity: AffinityContext {
            rules: RuleMissingContext {
                replica: ReplicaFaultContext {
                    config: &conf,
                    stores: &cluster.stores,
                    rules: &[],
                    now_unix_nanos: NOW,
                    supports_conf_change_v2: false,
                },
                witness_enabled: false,
            },
            labels: &labels,
            storage: &storage,
            paths: &paths,
            region_exists: true,
        },
        cluster: &cluster,
        operators: &admission,
        hot_regions: &hot,
    };
    let p = pending();
    let ctl = CheckerDispatcher::new(p.clone(), 0, 0);
    let result = ctl
        .enqueue_checked_operators(&ctx, 2, vec![operation(2)], vec![])
        .await;
    assert!(result.store_limited);
    assert!(result.queued.is_empty());
    assert!(result.admission_error.is_none());
    assert_eq!(ctl.pending_processed_regions(), vec![2]);
    // Changing the host config resets/refills the actual limiter on the next check.
    conf.schedule.default_store_limit.add_peer = 120.0;
    let ctx = CheckerDispatchContext {
        affinity: AffinityContext {
            rules: RuleMissingContext {
                replica: ReplicaFaultContext {
                    config: &conf,
                    stores: &cluster.stores,
                    rules: &[],
                    now_unix_nanos: NOW,
                    supports_conf_change_v2: false,
                },
                witness_enabled: false,
            },
            labels: &labels,
            storage: &storage,
            paths: &paths,
            region_exists: true,
        },
        cluster: &cluster,
        operators: &admission,
        hot_regions: &hot,
    };
    let result = ctl
        .enqueue_checked_operators(&ctx, 2, vec![operation(2)], vec![])
        .await;
    assert!(!result.store_limited);
    assert_eq!(result.queued.len(), 1);
    assert!(ctl.pending_processed_regions().is_empty());
    assert_eq!(admission.waiting_count("planned-add"), 1);
    p.shutdown().await;
    println!(
        "task_055: real store budget consumption rejects planned operator, pending insertion, rate reset queues/removes pending"
    );
}
