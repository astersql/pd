// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{StoreInfo, StoreMeta};
use astersql_pd_core_constant::SchedulePolicy;
use astersql_pd_schedule_operator::OpInfluence;
use std::sync::Arc;
fn store(id: u64, count: i64) -> Arc<StoreInfo> {
    let mut s = StoreInfo::new(StoreMeta {
        id,
        ..Default::default()
    });
    s.set_leader_count(count);
    s.set_last_heartbeat_unix_nanos(1_000_000_000_000);
    Arc::new(s)
}
#[test]
fn task_070_candidates() {
    let stores = [104, 0, 0, 100, 100, 0]
        .into_iter()
        .enumerate()
        .map(|(i, n)| store(i as u64 + 1, n))
        .collect();
    let influence = OpInfluence::new();
    let mut c = LeaderCandidates::new(stores, false, SchedulePolicy::BY_COUNT, &influence);
    assert_eq!(c.ids(), vec![1, 5, 4, 6, 3, 2]);
    assert_eq!(c.binary_search(4, &influence), 2);
    influence.update_store(1, |i| i.leader_count = -4);
    c.resort(0, &influence);
    assert_eq!(c.ids(), vec![5, 4, 1, 6, 3, 2]);
    influence.update_store(5, |i| i.leader_count = -1);
    c.resort(0, &influence);
    assert_eq!(c.ids(), vec![4, 1, 5, 6, 3, 2]);
    influence.update_store(2, |i| i.leader_count = 4);
    c.resort(5, &influence);
    assert_eq!(c.ids(), vec![4, 1, 5, 2, 6, 3]);
}
#[test]
fn task_070_config_and_allowed() {
    assert!(BalanceLeaderConfig::new(vec![], 0).is_err());
    assert!(BalanceLeaderConfig::new(vec![], 101).is_err());
    let cfg = BalanceLeaderConfig::default();
    assert_eq!(cfg.batch(), 4);
    let json = encode_config(&cfg).unwrap();
    assert_eq!(
        decode_config::<BalanceLeaderConfig>(&json).unwrap().batch(),
        4
    );
    assert!(decode_config::<BalanceLeaderConfig>(br#"{"ranges":[],"batch":0}"#).is_err());

    let mut clone = cfg.clone();
    clone.set_batch(100).unwrap();
    assert_eq!(cfg.batch(), 4);
    assert!(BalanceLeaderScheduler::is_schedule_allowed(3, 4));
    assert!(!BalanceLeaderScheduler::is_schedule_allowed(4, 4));
    assert!(!BalanceLeaderScheduler::is_schedule_allowed(0, 0));
}

#[tokio::test]
async fn task_070_behavior_cases() {
    use astersql_pd_core::{
        BasicCluster as CoreCluster, KeyRange, Peer, RegionInfo, RegionMeta, RegionOption,
    };
    use astersql_pd_core_constant::Direction;
    use astersql_pd_keypath::ClusterPaths;
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_core::BasicCluster;
    use astersql_pd_schedule_filter::AffinityFilter;
    use astersql_pd_schedule_operator::OpStep;
    use astersql_pd_schedule_placement::RuleManager;
    use astersql_pd_statistics::{HotPeerStat, StatisticsSnapshot};
    use astersql_pd_storage_domain::MemoryStore;
    use astersql_pd_storage_domain::StorageEndpoint;
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_070.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let counts = case["counts"]
            .as_array()
            .unwrap_or(fixture["defaults"]["counts"].as_array().unwrap());
        let sizes = case["sizes"]
            .as_array()
            .unwrap_or(fixture["defaults"]["sizes"].as_array().unwrap());
        let mut core = CoreCluster::new();
        for id in 1..=3 {
            if id == 1 && case["missing_source"] == true {
                continue;
            }
            let mut st = (*store(id, counts[id as usize - 1].as_i64().unwrap())).clone();
            st.set_leader_size(sizes[id as usize - 1].as_i64().unwrap());
            st.set_leader_weight(if id == 1 {
                case["weight"].as_f64().unwrap_or(1.0)
            } else {
                1.0
            });
            if id != 1
                && let Some(weights) = case["target_weights"].as_array()
            {
                st.set_leader_weight(weights[id as usize - 2].as_f64().unwrap());
            }
            if (id == 1 && case["removing_source"] == true)
                || (id != 1 && case["removing_targets"] == true)
            {
                st.set_store_state(astersql_pd_core::StoreState::Offline, Some(false));
            }
            if id != 1 && case["special"] == true {
                st.set_labels(vec![astersql_pd_core::StoreLabel {
                    key: "specialUse".into(),
                    value: "reserved".into(),
                }]);
            }
            core.stores.put_store(Arc::new(st));
        }
        let n = case["regions"].as_u64().unwrap_or(8);
        for id in 1..=n {
            let peers: Vec<_> = (1..=case["peers"].as_u64().unwrap_or(3))
                .map(|s| Peer {
                    id: id * 10 + s,
                    store_id: s,
                    is_witness: s != 1 && case["witness"] == true,
                    role: if s != 1 && case["learner"] == true {
                        astersql_pd_core::PeerRole::Learner
                    } else {
                        astersql_pd_core::PeerRole::Voter
                    },
                    ..Default::default()
                })
                .collect();
            let pending = if case["pending"] == true {
                vec![peers[0].clone()]
            } else {
                vec![]
            };
            let mut region = RegionInfo::new(
                RegionMeta {
                    id,
                    key_range: KeyRange {
                        start_key: vec![b'a' + id as u8 - 1],
                        end_key: vec![b'a' + id as u8],
                    },
                    peers: peers.clone(),
                    ..Default::default()
                },
                Some(peers[0].clone()),
                if case["down"] == true {
                    vec![astersql_pd_core::DownPeer {
                        peer: peers[0].clone(),
                        down_seconds: 0,
                    }]
                } else {
                    vec![]
                },
                pending,
            );
            region.stats_mut().approximate_size = 10;
            let region = Arc::new(region);
            let (o, v, b) = core.regions.set_region(region.clone());
            core.regions.update_sub_tree(region, o.as_deref(), &v, b);
        }
        if case["pause"] == true {
            for s in 2..=3 {
                core.pause_leader_transfer(s, Direction::IN).unwrap();
            }
        }
        if case["source_pause"] == true {
            core.pause_leader_transfer(1, Direction::OUT).unwrap();
        }
        let mut config = ProviderSnapshot::<()>::default();
        config.schedule.tolerant_size_ratio = 1.0;
        config.schedule.leader_schedule_policy = case["policy"].as_str().unwrap_or("count").into();
        config.replication.enable_placement_rules = case["placement"] == true;
        let storage = Arc::new(MemoryStore::default());
        let mut rules = RuleManager::new(
            Arc::new(StorageEndpoint::new(storage.clone())),
            ClusterPaths::new(70),
        );
        rules
            .initialize(3, &[], "", false, &config, &[])
            .await
            .unwrap();
        let labels = astersql_pd_schedule_labeler::RegionLabeler::load(
            StorageEndpoint::new(storage),
            ClusterPaths::new(70),
            Default::default(),
        )
        .await
        .unwrap();
        let key_ranges = astersql_pd_schedule_keyrange::Manager::new();
        if case["exclude"] == true {
            key_ranges.append(&[astersql_pd_keyutil::KeyRange::new(b"a", b"i")]);
        }
        let influence = OpInfluence::new();
        if let Some(values) = case["influence"].as_array() {
            for (j, v) in values.iter().enumerate() {
                influence.update_store(j as u64 + 1, |i| i.leader_count = v.as_i64().unwrap());
            }
        }
        let before = influence.snapshot();
        let mut statistics = StatisticsSnapshot::default();
        if case["hot"] == true {
            statistics.read.insert(
                1,
                (1..=n)
                    .map(|id| HotPeerStat {
                        store_id: 1,
                        region_id: id,
                        hot_degree: 10,
                        ..Default::default()
                    })
                    .collect(),
            );
        }
        let mut next = 0usize;
        let mut index = |n: usize| {
            let result = (next / 20) % n;
            next += 1;
            result
        };
        let mut permutations = 0usize;
        let mut shuffle = |order: &mut [usize]| {
            let n = order.len();
            order.rotate_left(permutations % n);
            permutations += 1;
        };
        let mut scheduler = BalanceLeaderScheduler::default();
        if case["custom"] == true {
            scheduler.name = "custom".into();
        }
        let ranges = case["ranges"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|r| KeyRange {
                        start_key: r[0].as_str().unwrap().as_bytes().to_vec(),
                        end_key: r[1].as_str().unwrap().as_bytes().to_vec(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        scheduler.config =
            BalanceLeaderConfig::new(ranges, case["batch"].as_u64().unwrap_or(4) as usize).unwrap();
        let mut ctx = LeaderBalanceContext {
            cluster: &core,
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
            random_index: &mut index,
            shuffle: &mut shuffle,
        };
        let result = scheduler.schedule(&mut ctx, true).await;
        assert_eq!(
            result.operators.len(),
            case["expected"].as_u64().unwrap() as usize,
            "{name}, errors={:?}",
            result.errors
        );
        assert_eq!(
            influence.snapshot(),
            before,
            "{name}: input influence immutable"
        );
        if name == "down-peer" || name == "pending-peers" {
            assert!(result.plans.iter().any(|p| p.step == 1
                && p.status.status_code
                    == astersql_pd_schedule_plan::StatusCode::REGION_UNHEALTHY));
        }
        if name == "witness-targets" {
            assert!(!result.errors.is_empty());
            assert!(result.plans.iter().any(|p| p.step == 4
                && p.status.status_code
                    == astersql_pd_schedule_plan::StatusCode::CREATE_OPERATOR_FAILED));
        }
        let mut used = std::collections::HashSet::new();
        for op in &result.operators {
            assert!(used.insert(op.region_id()), "{name}: duplicate region");
            assert_eq!(op.kind(), astersql_pd_schedule_operator::OpKind::LEADER);
            assert!(op.need_store_health_check());
            assert_eq!(op.len(), 1);
            let source_score: f64 = op
                .additional_info()
                .get("sourceScore")
                .unwrap()
                .parse()
                .unwrap();
            let target_score: f64 = op
                .additional_info()
                .get("targetScore")
                .unwrap()
                .parse()
                .unwrap();
            assert!(source_score > target_score);
            let OpStep::TransferLeader(step) = op.step(0).unwrap() else {
                panic!("expected transfer");
            };
            assert_eq!(step.from_store, 1);
            assert!([2, 3].contains(&step.to_store));
            if let Some(expected) = case["target"].as_u64() {
                assert_eq!(step.to_store, expected, "{name}");
            }
            assert!(step.to_stores.is_empty());
            let region = core.get_region(op.region_id()).unwrap();
            // Apply the actual command to the canonical region and role indexes.
            let applied = Arc::new(region.clone_with([RegionOption::Leader(
                region.get_store_peer(step.to_store).cloned(),
            )]));
            assert!(op.start());
            assert!(op.check(&applied).is_none());
            assert!(op.check_success());
            let (o, v, b) = core.regions.set_region(applied.clone());
            core.regions
                .update_sub_tree(applied.clone(), o.as_deref(), &v, b);
            assert_eq!(core.get_leader_store(&applied).unwrap().id(), step.to_store);
            let policy = if case["policy"] == "size" {
                SchedulePolicy::BY_SIZE
            } else {
                SchedulePolicy::BY_COUNT
            };
            let src = core.get_store(1).unwrap();
            let dst = core.get_store(step.to_store).unwrap();
            let gap_before = src.leader_score(policy, 0) - dst.leader_score(policy, 0);
            let mut src = (*src).clone();
            let mut dst = (*dst).clone();
            src.set_leader_count(src.leader_count() - 1);
            dst.set_leader_count(dst.leader_count() + 1);
            src.set_leader_size(src.leader_size() - region.stats().approximate_size);
            dst.set_leader_size(dst.leader_size() + region.stats().approximate_size);
            core.stores.put_store(Arc::new(src));
            core.stores.put_store(Arc::new(dst));
            let gap_after = core.get_store(1).unwrap().leader_score(policy, 0)
                - core
                    .get_store(step.to_store)
                    .unwrap()
                    .leader_score(policy, 0);
            assert!(
                gap_after.abs() < gap_before.abs(),
                "{name}: score must improve after applying transfer"
            );
        }
        if name == "balanced" || name == "tolerance-equal" {
            assert!(result.plans.iter().any(|p| p.step == 3
                && p.status.status_code
                    == astersql_pd_schedule_plan::StatusCode::STORE_SCORE_DISALLOWED));
        }
        println!(
            "task_070 case {name}: {} operators, {} factory errors",
            result.operators.len(),
            result.errors.len()
        );
    }
    println!("task_070: {} frozen behavior cases passed", cases.len());
}
