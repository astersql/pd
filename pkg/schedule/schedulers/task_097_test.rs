// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    BasicCluster, DownPeer, KeyRange, Peer, PeerRole, RegionInfo, RegionMeta, StoreInfo,
    StoreLabel, StoreMeta,
};
use astersql_pd_core_constant::{Direction, PriorityLevel};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::{ProviderSnapshot, REJECT_LEADER};
use astersql_pd_schedule_operator::{OpKind, OpStep};
use astersql_pd_schedule_placement::{
    LabelConstraint, LabelConstraintOp, PeerRoleType, Rule, RuleManager,
};
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use std::sync::Arc;
#[tokio::test]
async fn task_097_behavior_cases() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_097.json"
    ))
    .unwrap();
    for case in f["cases"].as_array().unwrap() {
        for v2 in [false, true] {
            let mut c = BasicCluster::new();
            let label = StoreLabel {
                key: "noleader".into(),
                value: "true".into(),
            };
            for id in 1..=3 {
                let mut s = StoreInfo::new(StoreMeta {
                    id,
                    labels: if id == 1 || (id == 2 && case["reject_target"] == true) {
                        vec![label.clone()]
                    } else {
                        vec![]
                    },
                    ..Default::default()
                });
                s.set_last_heartbeat_unix_nanos(if id == 2 && case["disconnected"] == true {
                    1
                } else {
                    1_000_000_000_000
                });
                if id == 2 && case["limit"] == true {
                    let limit = astersql_pd_storelimit::StoreRateLimit::new(0.000000001);
                    assert!(
                        limit
                            .take(
                                1000,
                                astersql_pd_storelimit::Type::TRANSFER_LEADER_IN,
                                PriorityLevel::MEDIUM
                            )
                            .await
                    );
                    s.set_store_limit(Some(astersql_pd_core::StoreLimiter::V1(limit)));
                }
                c.stores.put_store(Arc::new(s));
            }
            let peers: Vec<_> = (1..=3)
                .map(|id| Peer {
                    id,
                    store_id: id,
                    role: if id == 2 && case["learner"] == true {
                        PeerRole::Learner
                    } else {
                        PeerRole::Voter
                    },
                    is_witness: id == 2 && case["witness"] == true,
                    ..Default::default()
                })
                .collect();
            let ids = |s: &str| -> Vec<u64> {
                case[s]
                    .as_array()
                    .map(|a| a.iter().map(|v| v.as_u64().unwrap()).collect())
                    .unwrap_or_default()
            };
            let r = Arc::new(RegionInfo::new(
                RegionMeta {
                    id: 1,
                    key_range: KeyRange {
                        start_key: b"a".to_vec(),
                        end_key: b"b".to_vec(),
                    },
                    peers: peers.clone(),
                    ..Default::default()
                },
                Some(peers[0].clone()),
                ids("down")
                    .iter()
                    .map(|i| DownPeer {
                        peer: peers[*i as usize - 1].clone(),
                        down_seconds: 100,
                    })
                    .collect(),
                ids("pending")
                    .iter()
                    .map(|i| peers[*i as usize - 1].clone())
                    .collect(),
            ));
            if case["missing"] != true {
                let (o, v, b) = c.regions.set_region(r.clone());
                c.regions.update_sub_tree(r.clone(), o.as_deref(), &v, b);
            }
            for id in ids("pause") {
                c.stores.pause_leader_transfer(id, Direction::IN).unwrap();
            }
            if case["pause_out"] == true {
                c.stores.pause_leader_transfer(1, Direction::OUT).unwrap();
            }
            let mut config = ProviderSnapshot::<()>::default();
            if case["reject"] != false {
                config
                    .label_properties
                    .insert(REJECT_LEADER.into(), vec![label]);
            }
            let mut rules = RuleManager::new(
                Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
                ClusterPaths::new(97),
            );
            rules
                .initialize(3, &[], "", false, &config, &[])
                .await
                .unwrap();
            if case["placement"] == true || case["placement_allow"] == true {
                config.replication.enable_placement_rules = true;
                let rule = Rule {
                    group_id: "pd".into(),
                    id: "default".into(),
                    role: PeerRoleType::Voter,
                    count: 3,
                    label_constraints: vec![LabelConstraint {
                        key: "noleader".into(),
                        op: if case["placement"] == true {
                            LabelConstraintOp::In
                        } else {
                            LabelConstraintOp::NotIn
                        },
                        values: vec!["true".into()],
                    }],
                    ..Default::default()
                };
                rules
                    .set_rule(
                        rule,
                        0,
                        &config,
                        &c.stores
                            .get_stores()
                            .iter()
                            .map(|s| s.as_ref().clone())
                            .collect::<Vec<_>>(),
                    )
                    .await
                    .unwrap();
            }
            let ranges = case["ranges"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|p| KeyRange {
                            start_key: p[0].as_str().unwrap().as_bytes().to_vec(),
                            end_key: p[1].as_str().unwrap().as_bytes().to_vec(),
                        })
                        .collect()
                })
                .unwrap_or_else(|| vec![KeyRange::default()]);
            let scheduler = LabelScheduler::new(LabelConfig::new(ranges));
            let mut index = |n: usize| if case["last"] == true { n - 1 } else { 0 };
            let result = scheduler
                .schedule(&mut EvictLeaderContext {
                    cluster: &c,
                    config: &config,
                    rules: &mut rules,
                    now_unix_nanos: 1_000_000_000_000,
                    random_index: &mut index,
                    shuffle: &mut |_| {},
                })
                .await;
            let target = case["target"].as_u64().unwrap();
            assert_eq!(
                result.operators.len(),
                usize::from(target != 0),
                "{case}: {:?}",
                result.errors
            );
            assert_eq!(!result.errors.is_empty(), case["error"] == true, "{case}");
            if case["error"] == true {
                assert!(
                    matches!(
                        result.errors[0],
                        astersql_pd_schedule_operator::BuilderError::LeaderNotAllowed
                    ),
                    "{case}: {:?}",
                    result.errors
                );
            }
            for op in result.operators {
                assert_eq!(op.desc(), "label-reject-leader");
                assert_eq!(op.kind(), OpKind::LEADER);
                assert_eq!(op.priority_level(), PriorityLevel::MEDIUM);
                assert_eq!(op.len(), 1);
                let OpStep::TransferLeader(step) = op.step(0).unwrap() else {
                    panic!("expected transfer")
                };
                assert_eq!((step.from_store, step.to_store), (1, target));
                assert!(step.to_stores.is_empty());
                assert_eq!(step.conf_ver_changed(&r), 0);
                let influence = astersql_pd_schedule_operator::OpInfluence::new();
                step.influence(&influence, &r);
                assert_eq!(influence.get_store_influence(1).leader_count, -1);
                assert_eq!(influence.get_store_influence(target).leader_count, 1);
                let cmd = step.get_cmd(&r, v2).transfer_leader.unwrap();
                assert_eq!(cmd.peer.unwrap().store_id, target);
                assert!(cmd.peers.is_empty());
                assert!(op.start());
                assert!(op.check(&r).is_some());
                let applied = r.clone_with([astersql_pd_core::RegionOption::Leader(Some(
                    peers[target as usize - 1].clone(),
                ))]);
                assert!(op.check(&applied).is_none());
                assert!(op.check_success());
            }
        }
    }
    println!("task_097: 20 source-derived cases x 2 heartbeat modes = 40 passed");
}
#[test]
fn task_097_config_and_limits() {
    let config = LabelConfig::from_args(&["a%00", "b+", "ignored"]).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(
            &LabelScheduler::new(config.clone()).encode_config().unwrap()
        )
        .unwrap(),
        serde_json::json!({"ranges":[{"start-key":"6100","end-key":"6220"}]})
    );
    let restored: LabelConfig = decode_config(&encode_config(&config).unwrap()).unwrap();
    assert_eq!(restored.ranges(), config.ranges());
    assert!(LabelConfig::from_args(&["%q0", ""]).is_err());
    for bytes in [
        b"{".as_slice(),
        br#"{"ranges":[{"start-key":"xy"}]}"#,
        br#"{"ranges":2}"#,
    ] {
        assert!(decode_config::<LabelConfig>(bytes).is_err());
    }
    for (count, limit, allowed) in [
        (0, 0, false),
        (2, 3, true),
        (3, 3, false),
        (4, 3, false),
        (u64::MAX, u64::MAX, false),
    ] {
        assert_eq!(LabelScheduler::is_schedule_allowed(count, limit), allowed);
    }
    assert_eq!(
        LabelConfig::from_args(&[]).unwrap().ranges(),
        &[KeyRange::default()]
    );
}
