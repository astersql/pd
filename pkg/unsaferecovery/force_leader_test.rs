// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{Peer, RegionEpoch, RegionMeta, StoreInfo, StoreMeta, StoresInfo};
use std::sync::Arc;
fn fixture() -> (Controller, RecoveryIndex, StoresInfo) {
    let mut c = Controller::new(Arc::new(RecoverySteps::new(19)));
    c.auto_detect = true;
    let stores = StoresInfo::new();
    for id in 1..=2 {
        stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        })));
    }
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
                step: 19,
                peers: vec![PeerReport {
                    region: Some(region.clone()),
                    raft: Some(RaftState {
                        term: store,
                        last_index: 10,
                        commit: 10,
                        vote: 0,
                    }),
                    applied_index: 10,
                    ..Default::default()
                }],
            }),
        );
    }
    let index = c.build_region_index().unwrap();
    (c, index, stores)
}
#[test]
fn mcs_019_behavior() {
    let (c, index, stores) = fixture();
    let batch = c
        .generate_force_leader_plan(&index, &stores, false)
        .unwrap();
    let force = batch
        .plans
        .get(&2)
        .and_then(|p| p.force_leader.as_ref())
        .expect("freshest surviving peer requires an actual force-leader plan");
    assert_eq!(force.enter_force_leaders, vec![7]);
    assert_eq!(force.failed_stores, vec![3, 4, 5]);
    assert!(
        batch.plans[&1]
            .force_leader
            .as_ref()
            .unwrap()
            .enter_force_leaders
            .is_empty()
    );
    assert_eq!(batch.affected_regions, [7].into());
}
#[test]
fn mcs_019_joint_quorum_and_already_forced_reports() {
    use astersql_pd_core::PeerRole;
    let (mut c, index, stores) = fixture();
    let mut region = index.regions.items().next().unwrap().region().clone();
    region.peers = vec![
        Peer {
            id: 1,
            store_id: 1,
            role: PeerRole::Voter,
            ..Default::default()
        },
        Peer {
            id: 2,
            store_id: 2,
            role: PeerRole::IncomingVoter,
            ..Default::default()
        },
        Peer {
            id: 3,
            store_id: 3,
            role: PeerRole::DemotingVoter,
            ..Default::default()
        },
    ];
    assert!(!c.can_elect_leader(&region, false));
    assert!(c.can_elect_leader(&region, true));
    assert!(c.failed_peers(&index, &region).is_empty());
    region.peers[2].role = PeerRole::Learner;
    assert!(c.can_elect_leader(&region, false));
    region.peers.clear();
    assert!(!c.can_elect_leader(&region, false));
    c.expected_reports
        .get_mut(&1)
        .unwrap()
        .as_mut()
        .unwrap()
        .peers[0]
        .is_force_leader = true;
    assert!(
        !c.generate_force_leader_plan(&c.build_region_index().unwrap(), &stores, false)
            .unwrap()
            .has_work()
    );
    c.recovery_error = Some(RecoveryError::Backend("aborted".into()));
    assert!(
        !c.generate_force_leader_plan(&index, &stores, false)
            .unwrap()
            .has_work()
    );
}
#[test]
fn mcs_019_commit_merge_requires_its_phase_before_other_force_plans() {
    let (mut c, index, stores) = fixture();
    assert!(
        !c.generate_force_leader_plan(&index, &stores, true)
            .unwrap()
            .has_work()
    );
    for report in c.expected_reports.values_mut().flatten() {
        report.peers[0].has_commit_merge = true;
    }
    let index = c.build_region_index().unwrap();
    assert!(
        c.generate_force_leader_plan(&index, &stores, true)
            .unwrap()
            .has_work()
    );
    assert!(
        c.generate_force_leader_plan(&index, &stores, false)
            .unwrap_err()
            .to_string()
            .contains("unexpected commit merge state")
    );
    assert_eq!(
        c.prepare_force_leader(&index, &stores)
            .unwrap()
            .unwrap()
            .stage,
        Stage::ForceLeaderForCommitMerge
    );
}
#[test]
fn mcs_019_explicit_failed_stores_and_affected_table_or_meta() {
    let (mut c, _, stores) = fixture();
    c.auto_detect = false;
    c.failed_stores = [3, 4, 5, 99].into();
    for report in c.expected_reports.values_mut().flatten() {
        report.peers[0].region.as_mut().unwrap().key_range.start_key =
            astersql_pd_codec::encode_bytes(&astersql_pd_codec::generate_table_key(55));
    }
    let batch = c
        .generate_force_leader_plan(&c.build_region_index().unwrap(), &stores, false)
        .unwrap();
    assert_eq!(
        batch.plans[&2].force_leader.as_ref().unwrap().failed_stores,
        vec![3, 4, 5, 99]
    );
    assert_eq!(batch.affected_table_ids, [55].into());
    assert!(batch.affected_meta_regions.is_empty());
    for report in c.expected_reports.values_mut().flatten() {
        report.peers[0].region.as_mut().unwrap().key_range.start_key =
            astersql_pd_codec::encode_bytes(b"m");
    }
    let batch = c
        .generate_force_leader_plan(&c.build_region_index().unwrap(), &stores, false)
        .unwrap();
    assert_eq!(batch.affected_meta_regions, [7].into());
    assert!(batch.affected_table_ids.is_empty());
}
#[test]
fn mcs_019_fresher_raft_with_older_epoch_records_selected_peer_metadata() {
    let (mut c, _, stores) = fixture();
    let first = c
        .expected_reports
        .get_mut(&1)
        .unwrap()
        .as_mut()
        .unwrap()
        .peers[0]
        .region
        .as_mut()
        .unwrap();
    first.key_range.start_key =
        astersql_pd_codec::encode_bytes(&astersql_pd_codec::generate_table_key(55));
    let second = c
        .expected_reports
        .get_mut(&2)
        .unwrap()
        .as_mut()
        .unwrap()
        .peers[0]
        .region
        .as_mut()
        .unwrap();
    second.epoch.as_mut().unwrap().version = 8;
    second.key_range.start_key = astersql_pd_codec::encode_bytes(b"m");
    let index = c.build_region_index().unwrap();
    assert_eq!(index.regions.items().next().unwrap().store_id, 1);
    let batch = c
        .generate_force_leader_plan(&index, &stores, false)
        .unwrap();
    assert_eq!(
        batch.plans[&2]
            .force_leader
            .as_ref()
            .unwrap()
            .enter_force_leaders,
        vec![7]
    );
    assert_eq!(batch.affected_meta_regions, [7].into());
    assert!(batch.affected_table_ids.is_empty());
}
