// Copyright 2026 AsterSQL.
use crate::{Rule, fit_region};
use astersql_pd_core::{
    DownPeer, Peer, PeerRole, RegionInfo, RegionMeta, StoreInfo, StoreLabel, StoreMeta,
};
use std::sync::Arc;
#[test]
fn placement_region_fit_go_runtime_fixture() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule/placement/task27/go.json"
    ))
    .unwrap();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 15);
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
                    labels: entry["store"]["labels"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .map(|l| StoreLabel {
                                    key: l["key"].as_str().unwrap().into(),
                                    value: l["value"].as_str().unwrap().into(),
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    ..Default::default()
                });
                store.set_witness_count(entry["status"]["witness_count"].as_i64().unwrap_or(0));
                Arc::new(store)
            })
            .collect();
        let expected_fits = case["output"]["rule-fits"].as_array().unwrap();
        let rules: Vec<_> = expected_fits
            .iter()
            .map(|e| Arc::new(Rule::from_json(&serde_json::to_vec(&e["rule"]).unwrap()).unwrap()))
            .collect();
        let result = fit_region(
            &stores,
            &region,
            &rules,
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
            result.orphan_peers.iter().map(|p| p.id).collect::<Vec<_>>(),
            ids(&case["output"]["orphan-peers"]),
            "{name} orphans"
        );
        assert_eq!(result.rule_fits.len(), expected_fits.len());
        for (fit, expected) in result.rule_fits.iter().zip(expected_fits) {
            assert_eq!(
                fit.peers.iter().map(|p| p.id).collect::<Vec<_>>(),
                ids(&expected["peers"]),
                "{name} {}",
                fit.rule.id
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
        }
        assert_eq!(
            result.is_satisfied(),
            !expected_fits.is_empty()
                && ids(&case["output"]["orphan-peers"]).is_empty()
                && expected_fits.iter().all(|f| ids(&f["peers"]).len() as i64
                    == f["rule"]["count"].as_i64().unwrap()
                    && ids(&f["peers-different-role"]).is_empty()),
            "{name}"
        );
    }
}

fn region_peers(ids: &[u64]) -> RegionInfo {
    RegionInfo::new(
        RegionMeta {
            peers: ids
                .iter()
                .map(|id| Peer {
                    id: *id,
                    store_id: *id,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    )
}
fn voter(count: i64) -> Arc<Rule> {
    Arc::new(Rule {
        role: crate::PeerRoleType::Voter,
        count,
        ..Default::default()
    })
}
fn stores(count: u64) -> Vec<Arc<StoreInfo>> {
    (1..=count)
        .map(|id| {
            Arc::new(StoreInfo::new(StoreMeta {
                id,
                ..Default::default()
            }))
        })
        .collect()
}
#[test]
fn placement_region_fit_empty_rules_and_missing_stores() {
    let region = region_peers(&[3, 1, 2]);
    let empty = fit_region(&[], &region, &[], false);
    assert!(!empty.is_satisfied());
    assert!(empty.rule_fits.is_empty());
    assert_eq!(
        empty.orphan_peers.iter().map(|p| p.id).collect::<Vec<_>>(),
        [1, 2, 3]
    );
    let missing = fit_region(&[], &region, &[voter(3)], false);
    assert!(missing.rule_fits[0].peers.is_empty());
    assert_eq!(missing.orphan_peers.len(), 3);
    assert!(!missing.is_satisfied());
    let no_peers = fit_region(&stores(1), &region_peers(&[]), &[voter(1)], false);
    assert!(!no_peers.is_satisfied());
    assert!(no_peers.orphan_peers.is_empty());
}
#[test]
fn placement_region_fit_ties_and_large_peer_set_early_exit() {
    // More than a machine word of peers: no shifts/overflow, and a satisfied
    // no-isolation fit must stop rather than enumerate all possible partitions.
    let rules = vec![voter(1); 70];
    let ids: Vec<_> = (1..=70).rev().collect();
    let result = fit_region(&stores(70), &region_peers(&ids), &rules, false);
    assert!(result.is_satisfied());
    assert_eq!(
        result
            .rule_fits
            .iter()
            .map(|r| r.peers[0].id)
            .collect::<Vec<_>>(),
        (1..=70).collect::<Vec<_>>()
    );
    let result = fit_region(
        &stores(3),
        &region_peers(&[3, 2, 1]),
        &[voter(1), voter(1)],
        false,
    );
    assert_eq!(result.rule_fits[0].peers[0].id, 1);
    assert_eq!(result.rule_fits[1].peers[0].id, 2);
    assert_eq!(result.orphan_peers[0].id, 3);
}
