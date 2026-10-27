// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    BasicCluster as CoreCluster, KeyRange, Peer, PeerRole, RegionInfo, RegionMeta, RegionOption,
    StoreInfo, StoreMeta,
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_operator::{OpInfluence, OpKind, OpStep};
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_storage_domain::MemoryStore;
use astersql_pd_storage_domain::StorageEndpoint;
use std::sync::Arc;

#[tokio::test]
async fn task_073_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_073.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut cluster = CoreCluster::new();
        for (id, count) in [(1, 7), (2, 8), (3, 9), (4, 14)] {
            let mut s = StoreInfo::new(StoreMeta {
                id,
                ..Default::default()
            });
            if id == 4 {
                s.set_labels(vec![astersql_pd_core::StoreLabel {
                    key: "witness".into(),
                    value: "yes".into(),
                }]);
            }
            s.set_witness_count(if case["balanced"] == true { 8 } else { count });
            s.set_last_heartbeat_unix_nanos(1_000_000_000_000);
            if case["offline"] == true && id <= 3 {
                s.set_store_state(astersql_pd_core::StoreState::Offline, Some(false));
            }
            if case["source_offline"] == true && id == 4 {
                s.set_store_state(astersql_pd_core::StoreState::Offline, Some(false));
            }
            cluster.stores.put_store(Arc::new(s));
        }
        let peers: Vec<_> = (1..=4)
            .map(|id| Peer {
                id: id + 10,
                store_id: id,
                is_witness: (id == 4 && case["no_witness"] != true)
                    || (id <= 2 && case["all_witness"] == true),
                role: if id <= 2 && case["learner"] == true {
                    PeerRole::Learner
                } else {
                    PeerRole::Voter
                },
            })
            .collect();
        let down = if case["down"] == true {
            vec![astersql_pd_core::DownPeer {
                peer: peers[0].clone(),
                down_seconds: 1,
            }]
        } else {
            vec![]
        };
        let pending = if case["pending"] == true {
            vec![peers[0].clone()]
        } else {
            vec![]
        };
        let region = Arc::new(RegionInfo::new(
            RegionMeta {
                id: 1,
                key_range: KeyRange {
                    start_key: b"a".to_vec(),
                    end_key: b"b".to_vec(),
                },
                peers: peers.clone(),
                ..Default::default()
            },
            if case["no_leader"] == true {
                None
            } else {
                Some(peers[2].clone())
            },
            down,
            pending,
        ));
        let (o, v, b) = cluster.regions.set_region(region.clone());
        cluster
            .regions
            .update_sub_tree(region.clone(), o.as_deref(), &v, b);
        for id in 2..=case["regions"].as_u64().unwrap_or(1) {
            let r = Arc::new(region.clone_with([
                RegionOption::RegionId(id),
                RegionOption::StartKey(format!("z{id}").into_bytes()),
                RegionOption::EndKey(format!("z{}", id + 1).into_bytes()),
            ]));
            let (o, v, b) = cluster.regions.set_region(r.clone());
            cluster.regions.update_sub_tree(r, o.as_deref(), &v, b);
        }
        let mut config = ProviderSnapshot::<()>::default();
        config.schedule.tolerant_size_ratio = 2.5;
        config.replication.max_replicas = 4;
        config.replication.enable_placement_rules = case["placement"] == true;
        let mut rules = RuleManager::new(
            Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
            ClusterPaths::new(73),
        );
        rules
            .initialize(4, &[], "", false, &config, &[])
            .await
            .unwrap();
        if case["placement_block"] == true {
            use astersql_pd_schedule_placement::{
                LabelConstraint, LabelConstraintOp, PeerRoleType, Rule,
            };
            rules
                .set_rule(
                    Rule {
                        group_id: "pd".into(),
                        id: "leader".into(),
                        role: PeerRoleType::Leader,
                        count: 1,
                        ..Default::default()
                    },
                    1000,
                    &config,
                    &[],
                )
                .await
                .unwrap();
            rules
                .set_rule(
                    Rule {
                        group_id: "pd".into(),
                        id: "default".into(),
                        role: PeerRoleType::Follower,
                        count: 2,
                        ..Default::default()
                    },
                    1000,
                    &config,
                    &[],
                )
                .await
                .unwrap();
            rules
                .set_rule(
                    Rule {
                        group_id: "pd".into(),
                        id: "witness".into(),
                        role: PeerRoleType::Voter,
                        count: 1,
                        is_witness: true,
                        label_constraints: vec![LabelConstraint {
                            key: "witness".into(),
                            op: LabelConstraintOp::In,
                            values: vec!["yes".into()],
                        }],
                        ..Default::default()
                    },
                    1000,
                    &config,
                    &[],
                )
                .await
                .unwrap();
        }
        let influence = OpInfluence::new();
        if case["influence"] == true {
            influence.update_store(4, |s| s.witness_count = -2);
            influence.update_store(1, |s| s.witness_count = 2);
            influence.update_store(2, |s| s.witness_count = 2);
        }
        let before = influence.snapshot();
        let mut random = |_: usize| 0;
        let mut shuffle = |_: &mut [usize]| {};
        let mut scheduler = BalanceWitnessScheduler::default();
        scheduler.config = BalanceWitnessConfig::new(
            if case["outside"] == true {
                vec![KeyRange {
                    start_key: b"c".to_vec(),
                    end_key: b"d".to_vec(),
                }]
            } else {
                vec![]
            },
            case["batch"].as_u64().unwrap_or(4) as usize,
        )
        .unwrap();
        let mut ctx = WitnessBalanceContext {
            cluster: &cluster,
            config: &config,
            rules: &mut rules,
            influence: &influence,
            now_unix_nanos: 1_000_000_000_000,
            random_index: &mut random,
            shuffle: &mut shuffle,
            supports_conf_change_v2: case["simple"] != true,
        };
        let result = scheduler.schedule(&mut ctx, true).await;
        assert_eq!(
            result.operators.len(),
            case["expected"].as_u64().unwrap() as usize,
            "{name}: {:?}",
            result.errors
        );
        assert_eq!(influence.snapshot(), before, "{name}: immutable influence");
        if case["no_leader"] == true {
            assert!(
                result
                    .errors
                    .iter()
                    .any(|e| matches!(e, astersql_pd_schedule_operator::BuilderError::NoLeader)),
                "{name}: {:?}",
                result.errors
            );
            assert!(result.plans.iter().any(|p| p.step == 4
                && p.status.status_code
                    == astersql_pd_schedule_plan::StatusCode::CREATE_OPERATOR_FAILED));
        }
        if case["learner"] == true || case["all_witness"] == true {
            assert!(result.errors.is_empty());
        }
        for op in &result.operators {
            assert_eq!(op.region_id(), 1);
            assert_ne!(op.kind().0 & OpKind::WITNESS.0, 0);
            assert_eq!(
                op.priority_level(),
                astersql_pd_core_constant::PriorityLevel::MEDIUM
            );
            assert!(op.start());
            let mut applied = region.clone();
            for i in 0..op.len() {
                let mut peers = applied.meta().peers.clone();
                match op.step(i).unwrap() {
                    OpStep::ChangePeerV2Enter(s) => {
                        for p in &mut peers {
                            if s.demote_voters.iter().any(|n| n.to_store == p.store_id) {
                                p.role = PeerRole::Learner;
                            }
                            if s.promote_learners.iter().any(|n| n.to_store == p.store_id) {
                                p.role = PeerRole::Voter;
                            }
                        }
                    }
                    OpStep::BatchSwitchWitness(s) => {
                        for p in &mut peers {
                            if s.to_witnesses.iter().any(|n| n.store_id == p.store_id) {
                                assert_eq!(p.store_id, 1, "{name}: target");
                                p.is_witness = true;
                            }
                            if s.to_non_witnesses.iter().any(|n| n.store_id == p.store_id) {
                                assert_eq!(p.store_id, 4);
                                p.is_witness = false;
                            }
                        }
                    }
                    OpStep::BecomeWitness(s) => {
                        peers
                            .iter_mut()
                            .find(|p| p.store_id == s.store_id)
                            .unwrap()
                            .is_witness = true
                    }
                    OpStep::BecomeNonWitness(s) => {
                        peers
                            .iter_mut()
                            .find(|p| p.store_id == s.store_id)
                            .unwrap()
                            .is_witness = false
                    }
                    OpStep::PromoteLearner(s) => {
                        peers
                            .iter_mut()
                            .find(|p| p.store_id == s.to_store)
                            .unwrap()
                            .role = PeerRole::Voter
                    }
                    OpStep::RemovePeer(s) => peers.retain(|p| p.store_id != s.from_store),
                    OpStep::AddLearner(s) => peers.push(Peer {
                        id: s.peer_id,
                        store_id: s.to_store,
                        role: PeerRole::Learner,
                        is_witness: s.is_witness,
                    }),
                    s => panic!("{name}: unexpected {s:?}"),
                }
                applied = Arc::new(applied.clone_with([
                    RegionOption::Peers(peers),
                    RegionOption::PendingPeers(vec![]),
                ]));
                op.check(&applied);
            }
            assert!(op.check_success(), "{name}: applied steps complete");
            assert_eq!(
                op.conf_ver_changed(&applied),
                4,
                "{name}: demote/switch/promote confver"
            );
            assert_eq!(applied.leader(), region.leader());
            assert_eq!(
                applied
                    .meta()
                    .peers
                    .iter()
                    .map(|p| p.id)
                    .collect::<Vec<_>>(),
                region.meta().peers.iter().map(|p| p.id).collect::<Vec<_>>()
            );
            assert!(!applied.get_store_peer(4).unwrap().is_witness);
            assert!(applied.get_store_peer(1).unwrap().is_witness);
            assert_eq!(applied.meta().peers.len(), 4);
            assert!(
                applied
                    .meta()
                    .peers
                    .iter()
                    .all(|p| p.role == PeerRole::Voter)
            );
            let delta = OpInfluence::new();
            op.total_influence(&delta, Some(&region));
            assert_eq!(delta.get_store_influence(4).witness_count, -1);
            assert_eq!(delta.get_store_influence(1).witness_count, 1);
            let (o, v, b) = cluster.regions.set_region(applied.clone());
            cluster
                .regions
                .update_sub_tree(applied, o.as_deref(), &v, b);
            assert_eq!(
                cluster
                    .regions
                    .store_role_count(4, astersql_pd_core::RegionRole::Witness),
                case["regions"].as_u64().unwrap_or(1) as usize - 1
            );
            assert_eq!(
                cluster
                    .regions
                    .store_role_count(1, astersql_pd_core::RegionRole::Witness),
                1
            );
        }
        println!(
            "task_073 case {name}: {} operators, {} errors",
            result.operators.len(),
            result.errors.len()
        );
    }
}
#[test]
fn task_073_config_and_limits() {
    assert_eq!(BalanceWitnessConfig::default().batch(), 4);
    for batch in [0, 11, usize::MAX] {
        assert!(BalanceWitnessConfig::new(vec![], batch).is_err());
    }
    for batch in [1, 10] {
        assert_eq!(
            BalanceWitnessConfig::new(vec![], batch).unwrap().batch(),
            batch
        );
    }
    let c = BalanceWitnessConfig::new(
        vec![KeyRange {
            start_key: vec![0, 255],
            end_key: vec![255],
        }],
        10,
    )
    .unwrap();
    let decoded: BalanceWitnessConfig = decode_config(&encode_config(&c).unwrap()).unwrap();
    assert_eq!(decoded.ranges(), c.ranges());
    assert_eq!(decoded.batch(), 10);
    assert!(decode_config::<BalanceWitnessConfig>(br#"{"ranges":[],"batch":0}"#).is_err());
    assert!(BalanceWitnessScheduler::is_schedule_allowed(3, 4));
    assert!(!BalanceWitnessScheduler::is_schedule_allowed(4, 4));
    assert!(!BalanceWitnessScheduler::is_schedule_allowed(0, 0));
}
