// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    NodeState, Peer, PeerRole, RegionInfo, RegionMeta, StoreInfo, StoreLabel, StoreMeta, StoreStats,
};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_placement::{
    LabelConstraint, LabelConstraintOp, PeerRoleType, Rule, fit_region,
};
use serde_json::Value;
use std::{collections::BTreeSet, sync::Arc};

fn contains(c: &Value, key: &str, id: u64) -> bool {
    c[key]
        .as_array()
        .is_some_and(|a| a.iter().any(|v| v.as_u64() == Some(id)))
}
fn stores(c: &Value) -> Vec<Arc<StoreInfo>> {
    (1..=6)
        .filter(|id| !contains(c, "missing", *id))
        .map(|id| {
            let mut s = StoreInfo::new(StoreMeta {
                id,
                last_heartbeat: 1_000_000_000,
                node_state: if contains(c, "removed", id) {
                    NodeState::Removed
                } else {
                    NodeState::Serving
                },
                ..Default::default()
            });
            s.set_labels(vec![
                StoreLabel {
                    key: "zone".into(),
                    value: match id {
                        1 | 4 => "a",
                        2 | 5 => "b",
                        _ => "c",
                    }
                    .into(),
                },
                StoreLabel {
                    key: "engine".into(),
                    value: if contains(c, "special", id) {
                        "tiflash"
                    } else {
                        "tikv"
                    }
                    .into(),
                },
            ]);
            s.update_raw_stats(StoreStats {
                capacity: 1_000_000_000_000,
                available: 900_000_000_000,
                used_size: 100_000_000_000,
                is_busy: contains(c, "busy", id),
                ..Default::default()
            });
            Arc::new(s)
        })
        .collect()
}
fn region(c: &Value) -> RegionInfo {
    let peers: Vec<_> = (1..=3)
        .map(|id| Peer {
            id: 100 + id,
            store_id: id,
            role: if contains(c, "learner", id) {
                PeerRole::Learner
            } else if contains(c, "demoting", id) {
                PeerRole::DemotingVoter
            } else if contains(c, "incoming", id) {
                PeerRole::IncomingVoter
            } else {
                PeerRole::Voter
            },
            is_witness: contains(c, "witness", id),
        })
        .collect();
    let leader = if c["no_leader"] == true {
        None
    } else {
        Some(peers[0].clone())
    };
    RegionInfo::new(
        RegionMeta {
            id: 59,
            peers,
            ..Default::default()
        },
        leader,
        vec![],
        vec![],
    )
}
fn rules(c: &Value) -> Vec<Arc<Rule>> {
    (1..=3)
        .map(|id| {
            Arc::new(Rule {
                id: id.to_string(),
                role: if contains(c, "learner", id) {
                    PeerRoleType::Learner
                } else if contains(c, "follower", id) {
                    PeerRoleType::Follower
                } else {
                    PeerRoleType::Voter
                },
                count: 1,
                is_witness: contains(c, "witness", id),
                label_constraints: vec![
                    LabelConstraint {
                        key: "engine".into(),
                        op: LabelConstraintOp::In,
                        values: vec![
                            if contains(c, "special", id) {
                                "tiflash"
                            } else {
                                "tikv"
                            }
                            .into(),
                        ],
                    },
                    LabelConstraint {
                        key: "zone".into(),
                        op: LabelConstraintOp::In,
                        values: vec![
                            match id {
                                1 => "a",
                                2 => "b",
                                _ => "c",
                            }
                            .into(),
                        ],
                    },
                ],
                ..Default::default()
            })
        })
        .collect()
}
#[tokio::test]
async fn task_059_behavior_cases() {
    let f: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_059.json"
    ))
    .unwrap();
    assert_eq!(f["go_execution"], false);
    for c in f["cases"].as_array().unwrap() {
        let stores = stores(c);
        let region = region(c);
        let rules = rules(c);
        let mut conf = ProviderSnapshot::<()>::default();
        conf.replication.enable_placement_rules = false;
        conf.schedule.low_space_ratio = 0.8;
        let mut counts = ScatterCounts::default();
        for (field, group, engine, leader) in [
            ("counts", "g", "tikv", false),
            ("special_counts", "g", "tiflash", false),
            ("leader_counts", "g", "tikv", true),
            ("other_group_counts", "other", "tikv", false),
        ] {
            if let Some(values) = c[field].as_object() {
                for (id, n) in values {
                    counts.set(
                        group,
                        engine,
                        id.parse().unwrap(),
                        leader,
                        n.as_u64().unwrap(),
                    );
                }
            }
        }
        let avoid: BTreeSet<_> = c["avoid"]
            .as_array()
            .map(|a| a.iter().map(|v| v.as_u64().unwrap()).collect())
            .unwrap_or_default();
        let selector = ScatterTargetSelector {
            config: &conf,
            stores: &stores,
            rules: &rules,
            counts: &counts,
            now_unix_nanos: 1_000_000_000,
        };
        let result = selector.select(&region, "g", &avoid).await;
        if let Some(error) = c["error"].as_str() {
            assert_eq!(format!("{:?}", result.unwrap_err()), error, "{}", c["name"]);
            continue;
        }
        let target = result.unwrap_or_else(|e| panic!("{}: {e}", c["name"]));
        assert_eq!(
            serde_json::json!(target.peers.keys().copied().collect::<Vec<_>>()),
            c["expected"],
            "{}",
            c["name"]
        );
        assert_eq!(
            target.leader,
            c["leader"].as_u64().unwrap(),
            "{}",
            c["name"]
        );
        assert_eq!(
            target.leader_picked_count,
            c["leader_counts"][target.leader.to_string()]
                .as_u64()
                .unwrap_or(0)
        );
        let mut actual: Vec<_> = target
            .peers
            .values()
            .map(|p| (format!("{:?}", p.role), p.is_witness))
            .collect();
        actual.sort();
        let mut expected: Vec<_> = region
            .meta()
            .peers
            .iter()
            .map(|p| (format!("{:?}", p.role), p.is_witness))
            .collect();
        expected.sort();
        assert_eq!(actual, expected);
        assert_eq!(
            region
                .meta()
                .peers
                .iter()
                .map(|p| p.store_id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert!(target.peers.contains_key(&target.leader));
    }
    println!(
        "task_059: {} source-derived cases passed",
        f["cases"].as_array().unwrap().len()
    );
}

#[tokio::test]
async fn task_059_planned_placement_is_refitted() {
    // Mirrors TestScatterPreservesPlacement: after moving A -> D, another D
    // cannot replace B/C; E/F alternatives let all three peers move.
    for enabled in [false, true] {
        for alternatives in [false, true] {
            let c = serde_json::json!({});
            let mut stores = stores(&c);
            for (i, s) in stores.iter_mut().enumerate() {
                Arc::make_mut(s).set_labels(vec![StoreLabel {
                    key: "zone".into(),
                    value: if i < 3 { ["a", "b", "c"][i] } else { "d" }.into(),
                }]);
            }
            if alternatives {
                for (id, zone) in [(7, "e"), (8, "f")] {
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
                    s.set_labels(vec![StoreLabel {
                        key: "zone".into(),
                        value: zone.into(),
                    }]);
                    stores.push(Arc::new(s));
                }
            }
            let region = region(&c);
            let rules = vec![Arc::new(Rule {
                role: PeerRoleType::Voter,
                count: 3,
                location_labels: vec!["zone".into()],
                isolation_level: "zone".into(),
                ..Default::default()
            })];
            let mut conf = ProviderSnapshot::<()>::default();
            conf.replication.enable_placement_rules = enabled;
            conf.replication.location_labels = vec!["zone".into()].into();
            let mut counts = ScatterCounts::default();
            for id in 1..=3 {
                counts.set("g", "tikv", id, false, 10);
            }
            let saved = counts.clone();
            let target = ScatterTargetSelector {
                config: &conf,
                stores: &stores,
                rules: &rules,
                counts: &counts,
                now_unix_nanos: 1_000_000_000,
            }
            .select(&region, "g", &BTreeSet::new())
            .await
            .unwrap();
            assert_eq!(
                target.peers.keys().copied().collect::<Vec<_>>(),
                if alternatives {
                    vec![4, 7, 8]
                } else {
                    vec![2, 3, 4]
                },
                "rules={enabled}, alternatives={alternatives}"
            );
            let planned = RegionInfo::new(
                RegionMeta {
                    peers: target
                        .peers
                        .values()
                        .enumerate()
                        .map(|(i, p)| Peer {
                            id: 1000 + i as u64,
                            ..p.clone()
                        })
                        .collect(),
                    ..Default::default()
                },
                None,
                vec![],
                vec![],
            );
            assert!(fit_region(&stores, &planned, &rules, true).is_satisfied());
            let zones: BTreeSet<_> = target
                .peers
                .keys()
                .map(|id| {
                    stores
                        .iter()
                        .find(|s| s.id() == *id)
                        .unwrap()
                        .label_value("zone")
                })
                .collect();
            assert_eq!(zones.len(), 3);
            assert_eq!(counts, saved);
            assert_eq!(region.leader().unwrap().store_id, 1);
        }
    }
}

#[tokio::test]
async fn task_059_reserved_peer_keeps_its_role() {
    let c = serde_json::json!({"learner":[2],"witness":[3]});
    let stores = stores(&c);
    let region = region(&c);
    let rules = rules(&c);
    let conf = ProviderSnapshot::<()>::default();
    let mut counts = ScatterCounts::default();
    for (id, count) in [(1, 5), (2, 0), (3, 0), (4, 1), (5, 2), (6, 2)] {
        counts.set("g", "tikv", id, false, count);
    }
    let target = ScatterTargetSelector {
        config: &conf,
        stores: &stores,
        rules: &rules,
        counts: &counts,
        now_unix_nanos: 1_000_000_000,
    }
    .select(&region, "g", &BTreeSet::new())
    .await
    .unwrap();
    assert_eq!(
        target.peers.keys().copied().collect::<Vec<_>>(),
        vec![2, 3, 4]
    );
    assert_eq!(target.peers[&2], region.meta().peers[1]);
    assert_eq!(target.peers[&3], region.meta().peers[2]);
    assert_eq!(target.peers[&4].role, PeerRole::Voter);
    assert!(!target.peers[&4].is_witness);
    assert_eq!(target.peers[&4].id, 0);
    assert_eq!(target.leader, 4);
}

#[tokio::test]
async fn task_059_target_consumed_by_real_operator() {
    use astersql_pd_core::StoresInfo;
    use astersql_pd_schedule_operator::{
        BuilderContext, BuilderError, OpStep, RangeOperatorFactory,
    };
    let c = serde_json::json!({});
    let stores = stores(&c);
    let region = region(&c);
    let rules = rules(&c);
    let mut conf = ProviderSnapshot::<()>::default();
    conf.replication.enable_placement_rules = false;
    let mut counts = ScatterCounts::default();
    counts.set("g", "tikv", 1, false, 5);
    let target = ScatterTargetSelector {
        config: &conf,
        stores: &stores,
        rules: &rules,
        counts: &counts,
        now_unix_nanos: 1_000_000_000,
    }
    .select(&region, "g", &BTreeSet::new())
    .await
    .unwrap();
    let registry = StoresInfo::new();
    for store in stores {
        registry.put_store(store);
    }
    let context = BuilderContext {
        stores: &registry,
        config: &conf,
        rules: &rules,
        now_unix_nanos: 1_000_000_000,
    };
    let op = RangeOperatorFactory::new("scatter-region", context, &region, false)
        .create_scatter(target.peers, target.leader, 0, true, true, || {
            Ok::<_, BuilderError>(900)
        })
        .await
        .unwrap();
    let steps: Vec<_> = (0..op.len()).map(|i| op.step(i).unwrap()).collect();
    assert!(
        steps
            .iter()
            .any(|s| matches!(s,OpStep::AddLearner(p) if p.to_store==4 && p.peer_id==900))
    );
    assert!(
        steps
            .iter()
            .any(|s| matches!(s,OpStep::TransferLeader(p) if p.to_store==4))
    );
    assert!(
        steps
            .iter()
            .any(|s| matches!(s,OpStep::RemovePeer(p) if p.from_store==1))
    );
}

#[tokio::test]
async fn task_059_invalid_layout_and_fixed_order() {
    use astersql_pd_core::RegionOption;
    let c = serde_json::json!({});
    let stores = stores(&c);
    let region = region(&c);
    let rules = rules(&c);
    let conf = ProviderSnapshot::<()>::default();
    let counts = ScatterCounts::default();
    let selector = ScatterTargetSelector {
        config: &conf,
        stores: &stores,
        rules: &rules,
        counts: &counts,
        now_unix_nanos: 1_000_000_000,
    };
    for id in [0, 2] {
        let mut peers = region.meta().peers.clone();
        peers[0].store_id = id;
        let invalid = region.clone_with([RegionOption::Peers(peers)]);
        assert_eq!(
            selector.select(&invalid, "g", &BTreeSet::new()).await,
            Err(ScatterTargetError::InvalidPeerLayout)
        );
    }
    let expected = selector
        .select(&region, "g", &BTreeSet::new())
        .await
        .unwrap();
    for _ in 0..20 {
        assert_eq!(
            selector
                .select(&region, "g", &BTreeSet::new())
                .await
                .unwrap(),
            expected
        );
    }
    let reversed = region.clone_with([RegionOption::Peers(
        region.meta().peers.iter().rev().cloned().collect(),
    )]);
    assert_eq!(
        selector
            .select(&reversed, "g", &BTreeSet::new())
            .await
            .unwrap()
            .leader,
        3
    );
}
