// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{Peer, RegionEpoch, RegionMeta};
use std::sync::Arc;
fn fixture() -> (Controller, RecoveryIndex) {
    let mut c = Controller::new(Arc::new(RecoverySteps::new(20)));
    c.auto_detect = true;
    let region = RegionMeta {
        id: 7,
        epoch: Some(RegionEpoch {
            version: 9,
            conf_ver: 1,
        }),
        peers: (1..=5)
            .map(|id| Peer {
                id: id + 10,
                store_id: id,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    for store in 1..=2 {
        c.expected_reports.insert(
            store,
            Some(StoreReport {
                step: 20,
                peers: vec![PeerReport {
                    region: Some(region.clone()),
                    is_force_leader: store == 2,
                    ..Default::default()
                }],
            }),
        );
    }
    let index = c.build_region_index().unwrap();
    (c, index)
}
#[test]
fn mcs_020_behavior() {
    let (c, index) = fixture();
    let batch = c.generate_demote_failed_voter_plan(&index).unwrap();
    let demotes = &batch
        .plans
        .get(&2)
        .expect("current force leader must receive real demotions")
        .demotes;
    assert_eq!(demotes.len(), 1);
    assert_eq!(demotes[0].region_id, 7);
    assert_eq!(
        demotes[0]
            .failed_voters
            .iter()
            .map(|p| p.store_id)
            .collect::<Vec<_>>(),
        vec![3, 4, 5]
    );
    assert!(!batch.plans.contains_key(&1));
    assert_eq!(batch.affected_regions, [7].into());
}
#[test]
fn mcs_020_joint_exit_emits_empty_demotes_and_keeps_learner_roles() {
    use astersql_pd_core::PeerRole;
    let (mut c, _) = fixture();
    for report in c.expected_reports.values_mut().flatten() {
        for peer in &mut report.peers[0].region.as_mut().unwrap().peers {
            peer.role = if peer.store_id <= 2 {
                PeerRole::IncomingVoter
            } else {
                PeerRole::DemotingVoter
            };
        }
    }
    let batch = c
        .generate_demote_failed_voter_plan(&c.build_region_index().unwrap())
        .unwrap();
    assert_eq!(batch.plans[&2].demotes.len(), 1);
    assert!(batch.plans[&2].demotes[0].failed_voters.is_empty());
    let (mut c, _) = fixture();
    for report in c.expected_reports.values_mut().flatten() {
        report.peers[0].region.as_mut().unwrap().peers.push(Peer {
            id: 99,
            store_id: 6,
            role: PeerRole::Learner,
            is_witness: true,
        });
    }
    let before = c.expected_reports[&2].clone();
    let batch = c
        .generate_demote_failed_voter_plan(&c.build_region_index().unwrap())
        .unwrap();
    let failed = &batch.plans[&2].demotes[0].failed_voters;
    assert_eq!(
        failed.iter().map(|p| p.store_id).collect::<Vec<_>>(),
        vec![3, 4, 5, 6]
    );
    assert_eq!(failed.last().unwrap().role, PeerRole::Learner);
    assert!(failed.last().unwrap().is_witness);
    assert_eq!(c.expected_reports[&2], before);
}
#[test]
fn mcs_020_missing_force_leader_and_surviving_quorum_do_not_demote() {
    let (mut c, _) = fixture();
    for report in c.expected_reports.values_mut().flatten() {
        report.peers[0].is_force_leader = false;
    }
    assert!(
        !c.generate_demote_failed_voter_plan(&c.build_region_index().unwrap())
            .unwrap()
            .has_work()
    );
    let (mut c, _) = fixture();
    for report in c.expected_reports.values_mut().flatten() {
        report.peers[0].region.as_mut().unwrap().peers.truncate(3);
    }
    assert!(
        !c.generate_demote_failed_voter_plan(&c.build_region_index().unwrap())
            .unwrap()
            .has_work()
    );
    c.recovery_error = Some(RecoveryError::Backend("failed".into()));
    assert!(
        !c.generate_demote_failed_voter_plan(&RecoveryIndex::default())
            .unwrap()
            .has_work()
    );
}
#[test]
fn mcs_020_only_obsolete_regions_without_quorum_receive_tombstones() {
    let (mut c, _) = fixture();
    let mut old = c.expected_reports[&1].as_ref().unwrap().peers[0].clone();
    old.region.as_mut().unwrap().id = 8;
    old.region.as_mut().unwrap().epoch.as_mut().unwrap().version = 8;
    c.expected_reports
        .get_mut(&1)
        .unwrap()
        .as_mut()
        .unwrap()
        .peers
        .push(old);
    let index = c.build_region_index().unwrap();
    assert!(!index.regions.contains(8));
    let batch = c.generate_demote_failed_voter_plan(&index).unwrap();
    assert_eq!(batch.plans[&1].tombstones, vec![8]);
    assert_eq!(batch.affected_regions, [7, 8].into());
    let old = c
        .expected_reports
        .get_mut(&1)
        .unwrap()
        .as_mut()
        .unwrap()
        .peers
        .last_mut()
        .unwrap();
    old.region.as_mut().unwrap().peers.truncate(3);
    let batch = c
        .generate_demote_failed_voter_plan(&c.build_region_index().unwrap())
        .unwrap();
    assert!(!batch.plans.contains_key(&1));
}
#[test]
fn mcs_020_orphaned_peer_is_included_but_present_healthy_peer_is_not() {
    let (mut c, _) = fixture();
    c.auto_detect = false;
    c.failed_stores = [3, 4, 5].into();
    c.expected_reports
        .get_mut(&2)
        .unwrap()
        .as_mut()
        .unwrap()
        .peers
        .clear();
    c.expected_reports
        .get_mut(&1)
        .unwrap()
        .as_mut()
        .unwrap()
        .peers[0]
        .is_force_leader = true;
    let index = c.build_region_index().unwrap();
    assert_eq!(index.orphaned[&7][0].store_id, 2);
    let batch = c.generate_demote_failed_voter_plan(&index).unwrap();
    assert_eq!(
        batch.plans[&1].demotes[0]
            .failed_voters
            .iter()
            .map(|p| p.store_id)
            .collect::<Vec<_>>(),
        vec![2, 3, 4, 5]
    );
}
