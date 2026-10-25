// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    DownPeer, Peer, PeerRole, RegionInfo, RegionMeta, StoreInfo, StoreLabel, StoreMeta, StoresInfo,
};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_placement::{LabelConstraint, LabelConstraintOp, PeerRoleType, Rule};
use std::{collections::BTreeMap, sync::Arc};
fn peer(s: u64) -> Peer {
    Peer {
        id: s + 10,
        store_id: s,
        ..Default::default()
    }
}
#[tokio::test]
async fn task_026_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_026.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 53);
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let stores = StoresInfo::new();
        for s in 1..=4 {
            let mut store = StoreInfo::new(StoreMeta {
                id: s,
                labels: vec![StoreLabel {
                    key: "zone".into(),
                    value: if s == 1 { "a" } else { "b" }.into(),
                }],
                ..Default::default()
            });
            store.set_last_heartbeat_unix_nanos(if c["down"] == true {
                1
            } else {
                1_000_000_000_000
            });
            stores.put_store(Arc::new(store));
        }
        let mut config = ProviderSnapshot::<()>::default();
        config.replication.enable_placement_rules = c["rules"].is_string();
        let mut peers = vec![peer(1), peer(2), peer(3)];
        peers[2].role = PeerRole::Learner;
        if c["joint"] == true {
            peers[1].role = PeerRole::IncomingVoter;
        }
        if c["demoting"] == true {
            peers[1].role = PeerRole::DemotingVoter;
        }
        if c["zero_origin"] == true {
            peers[1].store_id = 0;
        }
        let r = RegionInfo::new(
            RegionMeta {
                id: 1,
                peers: peers.clone(),
                ..Default::default()
            },
            if c["no_leader"] == true {
                None
            } else {
                Some(peers[0].clone())
            },
            if c["down_peer"] == true || c["down_voter"] == true {
                vec![DownPeer {
                    peer: peers[if c["down_voter"] == true { 1 } else { 2 }].clone(),
                    down_seconds: 100,
                }]
            } else {
                vec![]
            },
            if c["pending"] == true || c["pending_learner"] == true {
                vec![peers[if c["pending_learner"] == true { 2 } else { 1 }].clone()]
            } else {
                vec![]
            },
        );
        let rules = match c["rules"].as_str() {
            Some("leader-zone") => vec![Arc::new(Rule {
                role: PeerRoleType::Leader,
                count: 1,
                label_constraints: vec![LabelConstraint {
                    key: "zone".into(),
                    op: LabelConstraintOp::In,
                    values: vec!["a".into()],
                }],
                ..Default::default()
            })],
            Some("follower") => vec![Arc::new(Rule {
                role: PeerRoleType::Follower,
                count: 2,
                ..Default::default()
            })],
            _ => vec![],
        };
        let context = BuilderContext {
            stores: &stores,
            config: &config,
            rules: &rules,
            now_unix_nanos: 1_000_000_000_000,
        };
        let mut b = Builder::new(
            "test",
            context,
            &r,
            BuilderOptions {
                skip_origin_joint_state_check: c["skip_joint"] == true,
                skip_placement_rules_check: c["skip_rules"] == true,
            },
        );
        for a in c["actions"].as_array().unwrap() {
            let s = a[1].as_u64().unwrap_or(0);
            match a[0].as_str().unwrap() {
                "add" => {
                    b.add_peer(Some(peer(s)));
                }
                "nil" => {
                    b.add_peer(None);
                }
                "joint" => {
                    let mut p = peer(s);
                    p.role = PeerRole::IncomingVoter;
                    b.add_peer(Some(p));
                }
                "remove" => {
                    b.remove_peer(s);
                }
                "leader" => {
                    b.set_leader(s);
                }
                "force" => {
                    b.enable_force_target_leader();
                }
                "promote" => {
                    b.promote_learner(s);
                }
                "demote" => {
                    b.demote_voter(s);
                }
                "witness" => {
                    let mut p = b.target_peers().clone();
                    p.get_mut(&s).unwrap().is_witness = true;
                    b.set_peers(p);
                }
                "only" => {
                    b.set_peers(BTreeMap::from([(s, peer(s))]));
                }
                "empty" => {
                    b.set_peers(BTreeMap::new());
                }
                "zeromap" => {
                    b.set_peers(BTreeMap::from([(0, peer(0))]));
                }
                "jointmap" => {
                    b.set_peers(BTreeMap::from([(
                        2,
                        Peer {
                            role: PeerRole::IncomingVoter,
                            ..peer(2)
                        },
                    )]));
                }
                "badmap" => {
                    b.set_peers(BTreeMap::from([(2, peer(4))]));
                }
                "learners" => {
                    let mut p = peer(4);
                    p.role = PeerRole::Learner;
                    b.set_peers(BTreeMap::from([(4, p)]));
                }
                "two_leaders" => {
                    b.set_expected_roles(BTreeMap::from([
                        (1, PeerRoleType::Leader),
                        (2, PeerRoleType::Leader),
                    ]));
                }
                "followers" => {
                    b.set_expected_roles(BTreeMap::from([(1, PeerRoleType::Follower)]));
                }
                "roles" => {
                    b.set_expected_roles(BTreeMap::from([
                        (1, PeerRoleType::Voter),
                        (2, PeerRoleType::Follower),
                    ]));
                }
                "missing_role" => {
                    b.set_expected_roles(BTreeMap::from([(9, PeerRoleType::Leader)]));
                }
                a => panic!("unknown {a}"),
            }
        }
        let result = b.validate_targets().await;
        assert_eq!(
            result.err().map(|e| format!("{e:?}")),
            c["error"].as_str().map(str::to_owned),
            "{name}"
        );
        if let Some(leader) = c["leader"].as_u64() {
            assert_eq!(b.target_leader(), leader, "{name}");
        }
        if let Some(err) = c["build"].as_str() {
            assert_eq!(
                format!("{:?}", b.build(OpKind::REGION).await.err().unwrap()),
                err,
                "{name}"
            );
        }
        if name == "sticky-first-error" {
            assert!(!b.target_peers().contains_key(&4));
        }
        assert_eq!(
            r.meta().peers,
            peers,
            "origin must remain immutable: {name}"
        );
    }
    println!("task026: {} fixture cases passed", cases.len());
}
#[tokio::test]
async fn task_026_multi_target_and_ownership() {
    let stores = StoresInfo::new();
    let config = ProviderSnapshot::<()>::default();
    let peers = vec![
        peer(1),
        peer(2),
        Peer {
            role: PeerRole::Learner,
            ..peer(3)
        },
    ];
    let r = RegionInfo::new(
        RegionMeta {
            peers: peers.clone(),
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![peers[1].clone()],
    );
    let mut b = Builder::new(
        "test",
        BuilderContext {
            stores: &stores,
            config: &config,
            rules: &[],
            now_unix_nanos: 0,
        },
        &r,
        BuilderOptions::default(),
    );
    b.set_leaders(vec![9, 3, 2, 1, 1]);
    b.set_leaders(vec![1]);
    assert_eq!(b.target_leaders(), [1, 1, 1]);
    assert!(b.allow_leader(&peer(1), false).await);
    assert!(!b.allow_leader(&peer(9), true).await);
    for role in [PeerRole::Learner, PeerRole::DemotingVoter] {
        assert!(!b.allow_leader(&Peer { role, ..peer(1) }, true).await);
    }
    assert!(
        !b.allow_leader(
            &Peer {
                is_witness: true,
                ..peer(1)
            },
            true
        )
        .await
    );
    let mut targets = b.target_peers().clone();
    targets.get_mut(&1).unwrap().id = 999;
    b.set_peers(targets.clone());
    targets.get_mut(&1).unwrap().id = 888;
    assert_eq!(b.target_peers()[&1].id, 999);
    assert_eq!(
        b.build(OpKind::REGION).await.err(),
        Some(BuilderError::NoSteps)
    );
    assert_eq!(r.meta().peers, peers);
    assert!(
        is_allowed_leader_target(
            BuilderContext {
                stores: &stores,
                config: &config,
                rules: &[],
                now_unix_nanos: 0
            },
            &r,
            &peer(1)
        )
        .await
    );
    let no_leader = RegionInfo::new(r.meta().clone(), None, vec![], vec![]);
    assert!(
        !is_allowed_leader_target(
            BuilderContext {
                stores: &stores,
                config: &config,
                rules: &[],
                now_unix_nanos: 0
            },
            &no_leader,
            &peer(1)
        )
        .await
    );
}
