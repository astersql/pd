// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    BasicCluster as CoreCluster, KeyRange, Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta,
    StoreStats,
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_core::BasicCluster;
use astersql_pd_schedule_filter::AffinityFilter;
use astersql_pd_schedule_operator::{BuilderError, OpInfluence, OpKind, OpStep};
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_statistics::StatisticsSnapshot;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use std::sync::Arc;

#[tokio::test]
async fn task_072_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_072.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let map = |id: u64| {
            if case["reverse"] == true {
                match id {
                    1 => 4,
                    4 => 1,
                    _ => id,
                }
            } else {
                id
            }
        };
        let mut cluster = CoreCluster::new();
        let sizes = case["sizes"]
            .as_array()
            .unwrap_or(fixture["sizes"].as_array().unwrap());
        for id in 1..=4 {
            let mut store = StoreInfo::new(StoreMeta {
                id: map(id),
                ..Default::default()
            });
            store.set_last_heartbeat_unix_nanos(1_000_000_000_000);
            store.set_region_size(sizes[id as usize - 1].as_i64().unwrap());
            store.set_region_count(10);
            let used = store.region_size().max(0) as u64 * (1 << 20);
            store.update_raw_stats(StoreStats {
                capacity: 6 * (1u64 << 40),
                used_size: used,
                available: 6 * (1u64 << 40) - used,
                ..Default::default()
            });
            if case["blocked_target"] == true && id == 4 {
                store.set_store_state(astersql_pd_core::StoreState::Offline, Some(false));
            }
            if case["placement_block"] == true {
                store.set_labels(vec![astersql_pd_core::StoreLabel {
                    key: "zone".into(),
                    value: if id == 4 { "2".into() } else { id.to_string() },
                }]);
            }
            cluster.stores.put_store(Arc::new(store));
        }
        let peers: Vec<_> = (1..=case["peers"].as_u64().unwrap_or(3))
            .map(|id| Peer {
                id: id + 10,
                store_id: map(id),
                is_witness: id == 1 && case["witness"] == true,
                ..Default::default()
            })
            .collect();
        let pending = if case["pending"] == true {
            vec![peers[0].clone()]
        } else {
            vec![]
        };
        let down = if case["down"] == true {
            vec![astersql_pd_core::DownPeer {
                peer: peers[0].clone(),
                down_seconds: 0,
            }]
        } else {
            vec![]
        };
        let leader = if case["no_leader"] == true {
            None
        } else {
            Some(peers[case["leader"].as_u64().unwrap_or(2) as usize - 1].clone())
        };
        let mut region = RegionInfo::new(
            RegionMeta {
                id: 1,
                key_range: KeyRange {
                    start_key: b"a".to_vec(),
                    end_key: b"b".to_vec(),
                },
                peers,
                ..Default::default()
            },
            leader,
            down,
            pending,
        );
        region.stats_mut().approximate_size = case["size"].as_i64().unwrap_or(96);
        let region = Arc::new(region);
        let (o, v, b) = cluster.regions.set_region(region.clone());
        cluster
            .regions
            .update_sub_tree(region.clone(), o.as_deref(), &v, b);
        for id in 2..=case["total"].as_u64().unwrap_or(1) {
            let peer = Peer {
                id: id * 10,
                store_id: map(4),
                ..Default::default()
            };
            let mut r = RegionInfo::new(
                RegionMeta {
                    id,
                    key_range: KeyRange {
                        start_key: format!("z{id:04}").into_bytes(),
                        end_key: format!("z{:04}", id + 1).into_bytes(),
                    },
                    peers: vec![peer.clone()],
                    ..Default::default()
                },
                Some(peer),
                vec![],
                vec![],
            );
            r.stats_mut().approximate_size = 0;
            let r = Arc::new(r);
            let (o, v, b) = cluster.regions.set_region(r.clone());
            cluster.regions.update_sub_tree(r, o.as_deref(), &v, b);
        }
        let mut config = ProviderSnapshot::<()>::default();
        config.schedule.tolerant_size_ratio = case["ratio"].as_f64().unwrap_or(1.0);
        config.schedule.region_score_formula_version = "v1".into();
        config.replication.max_replicas = 3;
        config.replication.enable_placement_rules = case["placement"] == true;
        if case["placement_block"] == true {
            config.replication.location_labels = vec!["zone".into()].into();
        }
        let storage = Arc::new(MemoryStore::default());
        let mut rules = RuleManager::new(
            Arc::new(StorageEndpoint::new(storage.clone())),
            ClusterPaths::new(72),
        );
        rules
            .initialize(
                3,
                config.replication.location_labels.as_slice(),
                "",
                false,
                &config,
                &[],
            )
            .await
            .unwrap();
        let labels = astersql_pd_schedule_labeler::RegionLabeler::load(
            StorageEndpoint::new(storage),
            ClusterPaths::new(72),
            Default::default(),
        )
        .await
        .unwrap();
        let influence = OpInfluence::new();
        if case["influence"] == true {
            influence.update_store(map(1), |i| i.region_size = 96);
            influence.update_store(map(4), |i| i.region_size = -96);
        }
        let before = influence.snapshot();
        let mut statistics = StatisticsSnapshot::default();
        if case["hot"] == true {
            statistics.read.insert(
                1,
                vec![astersql_pd_statistics::HotPeerStat {
                    region_id: 1,
                    store_id: 1,
                    hot_degree: 10,
                    ..Default::default()
                }],
            );
        }
        let key_ranges = astersql_pd_schedule_keyrange::Manager::new();
        if case["exclude"] == true {
            key_ranges.append(&[astersql_pd_keyutil::KeyRange::new(b"a", b"b")]);
        }
        let mut random = |_: usize| 0;
        let mut shuffle = |_: &mut [usize]| {};
        let mut allocate = || {
            if case["allocation_fail"] == true {
                Err(BuilderError::Allocation("injected".into()))
            } else {
                Ok(1000)
            }
        };
        let mut ctx = RegionBalanceContext {
            cluster: &cluster,
            config: &config,
            rules: &mut rules,
            affinity: AffinityFilter {
                manager: None,
                labels: &labels,
                now: 1000,
            },
            statistics: &statistics,
            hot_degree: 1,
            key_ranges: &key_ranges,
            influence: &influence,
            now_unix_nanos: 1_000_000_000_000,
            random_index: &mut random,
            shuffle: &mut shuffle,
            allocate: &mut allocate,
            supports_conf_change_v2: false,
            allow_empty_regions: case["range_view"] == true,
        };
        let mut scheduler = BalanceRegionScheduler::default();
        if case["outside"] == true {
            scheduler.config = BalanceRegionConfig::new(vec![KeyRange {
                start_key: b"c".to_vec(),
                end_key: b"d".to_vec(),
            }]);
        }
        let result = scheduler.schedule(&mut ctx, true).await;
        for _ in 1..case["repeat"].as_u64().unwrap_or(1) {
            assert_eq!(
                scheduler.schedule(&mut ctx, false).await.operators.len(),
                case["expected"].as_u64().unwrap() as usize,
                "{name}: repeated scheduling"
            );
        }

        assert_eq!(
            result.operators.len(),
            case["expected"].as_u64().unwrap() as usize,
            "{name}: {:?}",
            result.errors
        );
        assert_eq!(influence.snapshot(), before, "{name}");
        if case["allocation_fail"] == true {
            assert!(result.plans.iter().any(|p| p.step == 4
                && p.status.status_code
                    == astersql_pd_schedule_plan::StatusCode::CREATE_OPERATOR_FAILED));
            assert!(
                result
                    .errors
                    .iter()
                    .any(|e| matches!(e, BuilderError::Allocation(_)))
            );
        }
        for op in &result.operators {
            assert_eq!(op.region_id(), 1, "{name}");
            assert!(op.kind().0 & OpKind::REGION.0 != 0);
            assert!(op.need_store_health_check());
            assert!(
                matches!(op.step(0),Some(OpStep::AddLearner(s)) if s.to_store==map(4) && s.peer_id==1000)
            );
            assert!(
                matches!(op.step(op.len()-1),Some(OpStep::RemovePeer(s)) if s.from_store==map(1))
            );
            assert_eq!(
                op.priority_level(),
                astersql_pd_core_constant::PriorityLevel::MEDIUM
            );
            assert!(op.start());
            let mut applied = region.clone();
            for i in 0..op.len() {
                let mut peers = applied.meta().peers.clone();
                let mut leader = applied.leader().cloned();
                match op.step(i).unwrap() {
                    OpStep::AddLearner(step) => peers.push(Peer {
                        id: step.peer_id,
                        store_id: step.to_store,
                        role: astersql_pd_core::PeerRole::Learner,
                        ..Default::default()
                    }),
                    OpStep::PromoteLearner(step) => {
                        peers
                            .iter_mut()
                            .find(|p| p.store_id == step.to_store)
                            .unwrap()
                            .role = astersql_pd_core::PeerRole::Voter
                    }
                    OpStep::TransferLeader(step) => {
                        leader = peers.iter().find(|p| p.store_id == step.to_store).cloned()
                    }
                    OpStep::RemovePeer(step) => peers.retain(|p| p.store_id != step.from_store),
                    step => panic!("{name}: unexpected step {step:?}"),
                }
                applied = Arc::new(applied.clone_with([
                    astersql_pd_core::RegionOption::Peers(peers),
                    astersql_pd_core::RegionOption::Leader(leader),
                    astersql_pd_core::RegionOption::PendingPeers(vec![]),
                ]));
                op.check(&applied);
            }
            assert!(op.check_success(), "{name}: actual steps finish");
            assert_eq!(
                op.conf_ver_changed(&applied),
                3,
                "{name}: add/promote/remove"
            );
            assert!(applied.get_store_peer(map(1)).is_none());
            assert_eq!(
                applied.get_store_peer(map(4)).unwrap().role,
                astersql_pd_core::PeerRole::Voter
            );
            let (o, v, b) = cluster.regions.set_region(applied.clone());
            cluster
                .regions
                .update_sub_tree(applied.clone(), o.as_deref(), &v, b);
            assert_eq!(cluster.store_region_count(map(1)), 0);
            assert!(cluster.store_region_count(map(4)) > 0);
            let delta = OpInfluence::new();
            op.total_influence(&delta, Some(&region));
            assert_eq!(
                delta.get_store_influence(map(1)).region_size,
                -region.stats().approximate_size
            );
            assert_eq!(
                delta.get_store_influence(map(4)).region_size,
                region.stats().approximate_size
            );
            let mut src = (*cluster.get_store(map(1)).unwrap()).clone();
            let mut dst = (*cluster.get_store(map(4)).unwrap()).clone();
            let gap = src.region_size() - dst.region_size();
            src.set_region_size(src.region_size() - region.stats().approximate_size);
            dst.set_region_size(dst.region_size() + region.stats().approximate_size);
            assert!(
                (src.region_size() - dst.region_size()).abs() < gap.abs()
                    || region.stats().approximate_size == 0,
                "{name}: improves distribution"
            );
        }
        println!(
            "task_072 case {name}: {} operators, {} errors",
            result.operators.len(),
            result.errors.len()
        );
    }
}
#[test]
fn task_072_config_and_limits() {
    assert!(BalanceRegionScheduler::is_schedule_allowed(3, 4));
    assert!(!BalanceRegionScheduler::is_schedule_allowed(4, 4));
    assert!(!BalanceRegionScheduler::is_schedule_allowed(0, 0));
    let config = BalanceRegionConfig::new(vec![KeyRange {
        start_key: vec![0, 255],
        end_key: vec![255],
    }]);
    let encoded = encode_config(&config).unwrap();
    let decoded: BalanceRegionConfig = decode_config(&encoded).unwrap();
    assert_eq!(decoded.ranges(), config.ranges());
    assert!(
        decode_config::<BalanceRegionConfig>(br#"{"ranges":[{"start-key":"xx","end-key":""}]}"#)
            .is_err()
    );
    assert_eq!(BalanceRegionConfig::new(vec![]).ranges().len(), 1);
}

#[test]
fn task_072_influence_amp_and_candidate_margin() {
    use astersql_pd_core_constant::{ResourceKind, ScheduleKind, SchedulePolicy};
    use astersql_pd_schedule_plan::BalanceSchedulerPlan;
    let kind = ScheduleKind::new(ResourceKind::REGION, SchedulePolicy::BY_SIZE);
    // Frozen from TestInfluenceAmp and candidate-size regression tests; no Go execution.
    for (source, target, candidate, average, ratio, pending, expected) in [
        (108 * 96, 95 * 96, 96, 96, 1.0, true, true),
        (107 * 96, 95 * 96, 96, 96, 1.0, true, false),
        (288, 0, 96, 96, 1.0, false, true),
        (150, 0, 100, 100, 0.1, false, false),
        (10, 0, 10, 0, 1.0, false, false),
    ] {
        let mut config = ProviderSnapshot::<()>::default();
        config.schedule.tolerant_size_ratio = ratio;
        config.schedule.region_score_formula_version = "v1".into();
        let mut stores = Vec::new();
        for (id, size) in [(1, source), (2, target)] {
            let mut s = StoreInfo::new(StoreMeta {
                id,
                ..Default::default()
            });
            s.set_region_size(size);
            s.update_raw_stats(StoreStats {
                capacity: 6 * (1u64 << 40),
                available: 6 * (1u64 << 40) - size as u64 * (1 << 20),
                used_size: size as u64 * (1 << 20),
                ..Default::default()
            });
            stores.push(Arc::new(s));
        }
        let mut r = RegionInfo::new(RegionMeta::default(), None, vec![], vec![]);
        r.stats_mut().approximate_size = candidate;
        let plan = BalanceSchedulerPlan {
            source: Some(stores[0].clone()),
            target: Some(stores[1].clone()),
            region: Some(Arc::new(r)),
            ..Default::default()
        };
        let influence = OpInfluence::new();
        if pending {
            influence.update_store(1, |s| s.region_size = 96);
            influence.update_store(2, |s| s.region_size = -96);
        }
        assert_eq!(
            BalanceSolver::new(kind, &config.schedule, average, 10)
                .evaluate(&plan, &influence)
                .unwrap()
                .should_balance(),
            expected,
            "source={source} target={target} candidate={candidate}"
        );
    }
}
