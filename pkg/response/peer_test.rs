// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{Peer, PeerRole};
use serde_json::{Value, json};

#[test]
fn core_task_63_peer_roles_and_boundaries() {
    let fixture: Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_63.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let role = match case["role"].as_i64().unwrap() as i32 {
            0 => PeerRole::Voter,
            1 => PeerRole::Learner,
            2 => PeerRole::IncomingVoter,
            3 => PeerRole::DemotingVoter,
            n => PeerRole::Unknown(n),
        };
        let peer = Peer {
            id: case["id"].as_u64().unwrap(),
            store_id: case["store_id"].as_u64().unwrap(),
            role,
            is_witness: case["witness"].as_bool().unwrap(),
        };
        assert_eq!(
            serde_json::to_value(MetaPeer::from_peer(Some(&peer))).unwrap(),
            case["expected"]
        );
    }
}
#[test]
fn core_task_63_nil_and_default() {
    assert_eq!(
        serde_json::to_value(MetaPeer::from_peer(None)).unwrap(),
        json!({"role_name":""})
    );
    assert_eq!(
        serde_json::to_value(PDPeerStats::default()).unwrap(),
        json!({"peer":{"role_name":""}})
    );
}
#[test]
fn core_task_63_stats() {
    for (role, name, learner) in [
        (PeerRole::Voter, "Voter", false),
        (PeerRole::Learner, "Learner", true),
        (PeerRole::IncomingVoter, "IncomingVoter", false),
        (PeerRole::DemotingVoter, "DemotingVoter", false),
    ] {
        let peer = Peer {
            role,
            ..Peer::default()
        };
        for down_seconds in [0, 1, u64::MAX] {
            let stats = PDPeerStats {
                peer: MetaPeer::from_peer(Some(&peer)),
                down_seconds,
            };
            let value = serde_json::to_value(stats).unwrap();
            assert_eq!(value["peer"]["role_name"], name);
            assert_eq!(
                value["peer"]["is_learner"].as_bool().unwrap_or(false),
                learner
            );
            if down_seconds == 0 {
                assert!(value.get("down_seconds").is_none());
            } else {
                assert_eq!(value["down_seconds"], down_seconds);
            }
        }
    }
}

#[test]
fn core_task_63_down_peer_conversion() {
    let down = astersql_pd_core::DownPeer {
        peer: Peer {
            id: 2,
            store_id: 20,
            role: PeerRole::Learner,
            is_witness: false,
        },
        down_seconds: 1,
    };
    assert_eq!(
        serde_json::to_value(PDPeerStats::from(&down)).unwrap(),
        json!({
            "peer": {"id":2,"store_id":20,"role":1,"role_name":"Learner","is_learner":true},
            "down_seconds":1
        })
    );
    assert_eq!(
        serde_json::to_value(PDPeerStats {
            down_seconds: 9,
            ..PDPeerStats::default()
        })
        .unwrap(),
        json!({"peer":{"role_name":""},"down_seconds":9})
    );
}
