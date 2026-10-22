// Copyright 2026 AsterSQL.
use super::{Peer, PeerRole, count_in_joint_state, is_in_joint_state};
use serde::Deserialize;

#[derive(Deserialize)]
struct InputPeer {
    id: u64,
    store_id: u64,
    role: i32,
    witness: bool,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Expected {
    classes: Vec<[bool; 5]>,
    joint: bool,
    count: usize,
}

#[derive(Deserialize)]
struct Scenario {
    name: String,
    peers: Vec<InputPeer>,
    expected: Expected,
}

fn verify(filter: impl Fn(&Scenario) -> bool) {
    let cases: Vec<Scenario> =
        serde_json::from_str(include_str!("rust-fixtures/core_task_09.json")).unwrap();
    let mut checked = 0;
    for case in cases.into_iter().filter(filter) {
        let peers: Vec<_> = case
            .peers
            .into_iter()
            .map(|p| Peer {
                id: p.id,
                store_id: p.store_id,
                role: match p.role {
                    0 => PeerRole::Voter,
                    1 => PeerRole::Learner,
                    2 => PeerRole::IncomingVoter,
                    3 => PeerRole::DemotingVoter,
                    value => PeerRole::Unknown(value),
                },
                is_witness: p.witness,
            })
            .collect();
        let actual = Expected {
            classes: peers
                .iter()
                .map(|p| {
                    [
                        p.is_voter(),
                        p.is_learner(),
                        p.is_voter_or_incoming_voter(),
                        p.is_learner_or_demoting_voter(),
                        p.is_witness(),
                    ]
                })
                .collect(),
            joint: is_in_joint_state(&peers),
            count: count_in_joint_state(&peers),
        };
        assert_eq!(actual, case.expected, "{}", case.name);
        checked += 1;
    }
    assert!(checked > 0);
}

#[test]
fn core_task_09_roles_and_witness() {
    verify(|s| s.name.starts_with("role_") && !s.name.starts_with("role_99"));
}

#[test]
fn core_task_09_unknown_role() {
    verify(|s| s.name.starts_with("role_99"));
}

#[test]
fn core_task_09_joint_and_empty() {
    verify(|s| s.name == "mixed" || s.name == "empty");
}

#[test]
fn core_task_09_default_and_identity() {
    let peer = Peer::default();
    assert_eq!(peer.id, 0);
    assert_eq!(peer.store_id, 0);
    assert_eq!(peer.role, PeerRole::Voter);
    assert!(peer.is_voter());
    assert!(!peer.is_witness());
    let peer = Peer {
        id: u64::MAX,
        store_id: u64::MAX,
        ..peer
    };
    assert_eq!(peer.clone(), peer);
    assert_eq!(peer.id, u64::MAX);
    assert_eq!(peer.store_id, u64::MAX);
}
