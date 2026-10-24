// Copyright 2026 AsterSQL.
use crate::{Rule, fit_rule};
use astersql_pd_core::{DownPeer, Peer, PeerRole, RegionInfo, RegionMeta, StoreInfo, StoreMeta};
use std::sync::Arc;
#[test]
fn placement_role_fit_go_runtime_fixture() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule/placement/task26/go.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let peers: Vec<_> = case["peers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| Peer {
                id: p["id"].as_u64().unwrap(),
                store_id: p["store_id"].as_u64().unwrap(),
                role: match p["role"].as_u64().unwrap_or(0) {
                    1 => PeerRole::Learner,
                    2 => PeerRole::IncomingVoter,
                    3 => PeerRole::DemotingVoter,
                    _ => PeerRole::Voter,
                },
                is_witness: p["is_witness"].as_bool().unwrap_or(false),
            })
            .collect();
        let health = case["health"].as_bool().unwrap_or(false);
        let region = RegionInfo::new(
            RegionMeta {
                peers: peers.clone(),
                ..Default::default()
            },
            Some(peers[0].clone()),
            if health {
                vec![DownPeer {
                    peer: peers[0].clone(),
                    down_seconds: 10,
                }]
            } else {
                vec![]
            },
            if health {
                vec![peers[1].clone()]
            } else {
                vec![]
            },
        );
        let stores: Vec<_> = case["stores"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| {
                let mut store = StoreInfo::new(StoreMeta {
                    id: entry["store"]["id"].as_u64().unwrap(),
                    ..Default::default()
                });
                store.set_witness_count(entry["status"]["witness_count"].as_i64().unwrap_or(0));
                Arc::new(store)
            })
            .collect();
        let expected = &case["output"]["rule-fits"][0];
        let rule =
            Arc::new(Rule::from_json(&serde_json::to_vec(&expected["rule"]).unwrap()).unwrap());
        let fit = fit_rule(
            &stores,
            &region,
            rule,
            case["support"].as_bool().unwrap_or(false),
        );
        let ids = |v: &serde_json::Value| {
            v.as_array()
                .map(|a| {
                    a.iter()
                        .map(|p| p["id"].as_u64().unwrap())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        assert_eq!(
            fit.peers.iter().map(|p| p.id).collect::<Vec<_>>(),
            ids(&expected["peers"]),
            "{name}"
        );
        assert_eq!(
            fit.peers_with_different_role
                .iter()
                .map(|p| p.id)
                .collect::<Vec<_>>(),
            ids(&expected["peers-different-role"]),
            "{name}"
        );
        assert_eq!(
            fit.isolation_score,
            expected["isolation-score"].as_f64().unwrap(),
            "{name}"
        );
        assert_eq!(
            fit.witness_score,
            expected["witness-score"].as_i64().unwrap(),
            "{name}"
        );
        assert_eq!(
            fit.is_satisfied(),
            fit.peers.len() as i64 == fit.rule.count
                && ids(&expected["peers-different-role"]).is_empty(),
            "{name}"
        );
    }
}

fn snapshots() -> (Vec<Arc<StoreInfo>>, RegionInfo) {
    let peers: Vec<_> = (1..=3)
        .map(|id| Peer {
            id,
            store_id: id,
            ..Default::default()
        })
        .collect();
    let stores = (1..=3)
        .map(|id| {
            Arc::new(StoreInfo::new(StoreMeta {
                id,
                ..Default::default()
            }))
        })
        .collect();
    (
        stores,
        RegionInfo::new(
            RegionMeta {
                peers,
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        ),
    )
}
fn rule(role: crate::PeerRoleType, count: i64) -> Arc<Rule> {
    Arc::new(Rule {
        role,
        count,
        ..Default::default()
    })
}
#[test]
fn placement_role_fit_nil_leader_unknown_and_witness() {
    use crate::PeerRoleType::*;
    let (stores, mut region) = snapshots();
    for role in [Leader, Unknown("invalid".into())] {
        let fit = fit_rule(&stores, &region, rule(role, 1), false);
        assert_eq!(fit.peers[0].id, 1);
        assert_eq!(fit.peers_with_different_role, fit.peers);
        assert!(!fit.is_satisfied());
    }
    assert!(fit_rule(&stores, &region, rule(Follower, 3), false).is_satisfied());
    let mut meta = region.meta().clone();
    meta.peers[0].is_witness = true;
    // Even an explicitly reported learner leader matches the Go strict leader ID check.
    meta.peers[0].role = PeerRole::Learner;
    region = RegionInfo::new(meta.clone(), Some(meta.peers[0].clone()), vec![], vec![]);
    let fit = fit_rule(&stores, &region, rule(Leader, 3), false);
    assert_eq!(fit.peers_with_different_role.len(), 3);
    let witness = Arc::new(Rule {
        is_witness: true,
        ..(*rule(Voter, 3)).clone()
    });
    let fit = fit_rule(&stores, &region, witness.clone(), true);
    assert!(!fit.contains_store(1)); // A leader is never selected as witness when enabled.
    assert_eq!(fit.peers.len(), 2);
    let fit = fit_rule(&stores, &region, witness, false);
    assert_eq!(fit.peers.len(), 3);
    assert_eq!(fit.peers_with_different_role.len(), 1);
}
#[test]
fn placement_role_fit_constraints_missing_and_health() {
    use crate::{LabelConstraint, LabelConstraintOp, PeerRoleType::Voter};
    let (stores, region) = snapshots();
    let constrained = Arc::new(Rule {
        label_constraints: vec![LabelConstraint {
            key: "zone".into(),
            op: LabelConstraintOp::Exists,
            values: vec![],
        }],
        ..(*rule(Voter, 1)).clone()
    });
    assert!(
        fit_rule(&stores, &region, constrained, false)
            .peers
            .is_empty()
    );
    assert!(
        fit_rule(&[], &region, rule(Voter, 1), false)
            .peers
            .is_empty()
    );
    let peers = region.meta().peers.clone();
    let region = RegionInfo::new(
        region.meta().clone(),
        None,
        vec![DownPeer {
            peer: peers[0].clone(),
            down_seconds: 1,
        }],
        vec![peers[0].clone(), peers[1].clone()],
    );
    let fit = fit_rule(&stores, &region, rule(Voter, 3), false);
    assert_eq!(
        fit.peers.iter().map(|p| p.id).collect::<Vec<_>>(),
        [3, 2, 1]
    );
    assert!(fit.is_satisfied()); // Down wins over pending for ordering, never affects satisfaction.
}
#[test]
fn placement_role_fit_comparison_location_and_witness_load() {
    use crate::PeerRoleType::Voter;
    use astersql_pd_core::StoreLabel;
    let (mut stores, mut region) = snapshots();
    for (i, s) in stores.iter_mut().enumerate() {
        let s = Arc::get_mut(s).unwrap();
        s.edit_meta(|m| {
            m.labels = vec![StoreLabel {
                key: "zone".into(),
                value: if i == 2 { "b" } else { "a" }.into(),
            }]
        });
        s.set_witness_count([9, 3, 1][i]);
    }
    let r = Arc::new(Rule {
        location_labels: vec!["zone".into()],
        ..(*rule(Voter, 2)).clone()
    });
    let fit = fit_rule(&stores, &region, r, false);
    assert_eq!(fit.peers.iter().map(|p| p.id).collect::<Vec<_>>(), [1, 3]);
    assert_eq!(fit.isolation_score, 1.0);
    let mut meta = region.meta().clone();
    for p in &mut meta.peers {
        p.is_witness = true;
    }
    region = RegionInfo::new(meta, None, vec![], vec![]);
    let r = Arc::new(Rule {
        is_witness: true,
        ..(*rule(Voter, 1)).clone()
    });
    let fit = fit_rule(&stores, &region, r, true);
    assert_eq!(fit.peers[0].id, 3);
    assert_eq!(fit.witness_score, 1);
    assert!(fit.is_satisfied());
    let mut less = fit_rule(&stores, &region, rule(Voter, 1), false);
    assert!(fit.compare(&less).is_gt()); // Fewer role mismatches wins before witness load.
    less.peers_with_different_role.clear();
    assert!(fit.compare(&less).is_lt()); // Smaller witness score wins after role/isolation ties.
}
