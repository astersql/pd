// Copyright 2026 AsterSQL.
use crate::*;
fn peer(id: u64, role: PeerRole, witness: bool) -> Peer {
    Peer {
        id,
        store_id: id + 10,
        role,
        is_witness: witness,
    }
}
fn ids(peers: &[Peer]) -> Vec<u64> {
    peers.iter().map(|p| p.id).collect()
}
#[test]
fn core_task_28_classification_and_queries() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_28.json")).unwrap();
    let peers = vec![
        peer(5, PeerRole::Unknown(99), false),
        peer(3, PeerRole::Learner, true),
        peer(2, PeerRole::IncomingVoter, false),
        peer(4, PeerRole::DemotingVoter, true),
        peer(1, PeerRole::Voter, false),
    ];
    let meta = RegionMeta {
        id: 42,
        peers: peers.clone(),
        ..Default::default()
    };
    let r = RegionInfo::new(
        meta.clone(),
        Some(peers[4].clone()),
        vec![
            DownPeer {
                peer: peers[0].clone(),
                down_seconds: 17,
            },
            DownPeer {
                peer: peers[1].clone(),
                down_seconds: 3,
            },
        ],
        vec![peers[3].clone(), peers[1].clone()],
    );
    assert_eq!(r.meta(), &meta);
    assert_eq!(r.leader(), Some(&peers[4]));
    assert_eq!(serde_json::json!(ids(r.voters())), fixture["voters"]);
    assert_eq!(ids(r.learners()), vec![3]);
    assert_eq!(ids(r.witnesses()), vec![3, 4]);
    assert_eq!(ids(r.pending_peers()), vec![3, 4]);
    assert_eq!(
        r.down_peers().iter().map(|p| p.peer.id).collect::<Vec<_>>(),
        vec![3, 5]
    );
    assert_eq!(r.down_peers()[1].down_seconds, 17);
    for p in &peers {
        assert_eq!(r.get_peer(p.id), Some(p));
        assert_eq!(r.get_store_peer(p.store_id), Some(p));
        assert_eq!(r.get_store_learner(p.store_id).is_some(), p.is_learner());
        assert_eq!(r.get_store_voter(p.store_id).is_some(), !p.is_learner());
        assert_eq!(r.get_store_witness(p.store_id).is_some(), p.is_witness);
    }
    assert_eq!(r.get_down_peer(5), Some(&peers[0]));
    assert!(r.get_down_voter(5).is_some());
    assert!(r.get_down_learner(5).is_none());
    assert!(r.get_down_learner(3).is_some());
    assert!(r.get_down_voter(3).is_none());
    assert!(r.get_pending_peer(3).is_some());
    assert!(r.get_pending_learner(3).is_some());
    assert!(r.get_pending_voter(3).is_none());
    assert!(r.get_pending_voter(4).is_some());
    assert!(r.get_pending_learner(4).is_none());
    assert_eq!(
        r.followers().keys().copied().collect::<Vec<_>>(),
        vec![12, 14, 15]
    );
    assert_eq!(
        r.non_witness_voters().keys().copied().collect::<Vec<_>>(),
        vec![11, 12, 15]
    );
    assert_eq!(r.store_ids().len(), 5);
    let other = RegionInfo::new(
        RegionMeta {
            peers: vec![peers[2].clone()],
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    );
    assert_eq!(
        r.diff_followers(&other)
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>(),
        vec![4, 5]
    );
}
#[test]
fn core_task_28_empty_and_missing() {
    let r = RegionInfo::new(RegionMeta::default(), None, vec![], vec![]);
    assert!(r.leader().is_none());
    assert!(r.voters().is_empty() && r.learners().is_empty() && r.witnesses().is_empty());
    assert!(r.followers().is_empty() && r.store_ids().is_empty());
    assert!(r.get_peer(0).is_none() && r.get_store_peer(0).is_none());
    assert!(r.get_down_peer(0).is_none() && r.get_pending_peer(0).is_none());
    let p = peer(0, PeerRole::Voter, false);
    let r = RegionInfo::new(
        RegionMeta {
            peers: vec![p.clone()],
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    );
    assert_eq!(r.followers().get(&10), Some(&&p));
    assert!(r.get_peer(u64::MAX).is_none());
}
#[test]
fn core_task_28_duplicate_store_lookup_order() {
    let a = Peer {
        id: 9,
        store_id: 7,
        ..Default::default()
    };
    let b = Peer {
        id: 1,
        store_id: 7,
        ..Default::default()
    };
    let r = RegionInfo::new(
        RegionMeta {
            peers: vec![a.clone(), b.clone()],
            ..Default::default()
        },
        Some(peer(99, PeerRole::Learner, false)),
        vec![],
        vec![],
    );
    assert_eq!(r.get_store_peer(7), Some(&a));
    assert_eq!(r.get_store_voter(7), Some(&b));
    assert_eq!(r.followers().get(&7), Some(&&a));
}
