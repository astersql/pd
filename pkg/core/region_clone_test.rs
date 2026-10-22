// Copyright 2026 AsterSQL.
use crate::*;
fn region() -> RegionInfo {
    let peer = Peer {
        id: 1,
        store_id: 10,
        ..Default::default()
    };
    RegionInfo::new(
        RegionMeta {
            peers: vec![peer.clone()],
            ..Default::default()
        },
        Some(peer),
        vec![],
        vec![],
    )
}
#[test]
fn core_task_29_snapshot_options() {
    let source = region();
    let copy = source.clone_with([
        RegionOption::StartKey(vec![0, 255]),
        RegionOption::EndKey(vec![9]),
        RegionOption::Leader(None),
        RegionOption::Flashback(true, 42),
        RegionOption::IncVersion,
        RegionOption::IncConfVer,
    ]);
    assert_eq!(source.meta().key_range, KeyRange::default());
    assert!(source.leader().is_some());
    assert_eq!(source.meta().epoch, None);
    assert!(!source.meta().is_in_flashback);
    assert_eq!(copy.meta().key_range.start_key, [0, 255]);
    assert_eq!(copy.meta().key_range.end_key, [9]);
    assert_eq!(
        copy.meta().epoch,
        Some(RegionEpoch {
            conf_ver: 1,
            version: 1
        })
    );
    assert!(copy.leader().is_none());
    assert!(copy.meta().is_in_flashback);
    assert_eq!(copy.meta().flashback_start_ts, 42);
    let reset = copy.clone_with([RegionOption::Flashback(false, 0)]);
    assert!(!reset.meta().is_in_flashback);
    assert!(copy.meta().is_in_flashback);
}
#[test]
fn core_task_29_peers_and_absent_candidates() {
    let source = region();
    let learner = Peer {
        id: 2,
        store_id: 20,
        role: PeerRole::Learner,
        is_witness: true,
    };
    let copy = source.clone_with([
        RegionOption::AddPeer(learner.clone()),
        RegionOption::ReplacePeerStore(10, 30),
        RegionOption::ReplaceLeaderStore(20),
    ]);
    assert_eq!(copy.leader().unwrap().store_id, 10); // learner cannot replace leader
    assert_eq!(copy.get_peer(1).unwrap().store_id, 30);
    assert_eq!(copy.learners(), &[learner.clone()]);
    assert_eq!(copy.witnesses(), &[learner.clone()]);
    let next = copy.clone_with([
        RegionOption::ReplaceLeaderStore(30),
        RegionOption::RemoveStorePeer(20),
    ]);
    assert_eq!(next.leader().unwrap().store_id, 30);
    assert!(next.learners().is_empty());
    assert!(next.witnesses().is_empty());
    let replaced = next.clone_with([
        RegionOption::Peers(vec![learner]),
        RegionOption::ReplaceLeaderStore(999),
    ]);
    assert!(replaced.voters().is_empty());
    assert_eq!(replaced.leader(), next.leader());
    assert_eq!(source.meta().peers.len(), 1);
    assert_eq!(source.get_peer(1).unwrap().store_id, 10);
    assert_eq!(
        source
            .clone_with([RegionOption::RemoveStorePeer(999)])
            .meta(),
        source.meta()
    );
}
#[test]
fn core_task_29_epoch_boundaries() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_29.json")).unwrap();
    let source = region();
    assert_eq!(
        source
            .clone_with([RegionOption::DecVersion, RegionOption::DecConfVer])
            .meta()
            .epoch,
        None
    );
    let zero = source.clone_with([
        RegionOption::IncVersion,
        RegionOption::DecVersion,
        RegionOption::DecVersion,
        RegionOption::DecConfVer,
    ]);
    let epoch = zero.meta().epoch.unwrap();
    assert_eq!(epoch.version, fixture["underflow"].as_u64().unwrap());
    assert_eq!(epoch.conf_ver, u64::MAX);
    let wrapped = zero.clone_with([RegionOption::IncVersion, RegionOption::IncConfVer]);
    assert_eq!(wrapped.meta().epoch, Some(RegionEpoch::default()));
}
