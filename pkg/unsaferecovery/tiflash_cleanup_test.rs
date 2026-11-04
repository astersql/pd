// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    Peer, PeerRole, RegionEpoch, RegionMeta, StoreInfo, StoreLabel, StoreMeta, StoresInfo,
};
use std::sync::Arc;
fn fixture() -> (Controller, RecoveryIndex, StoresInfo) {
    let mut c = Controller::new(Arc::new(RecoverySteps::new(21)));
    c.auto_detect = true;
    let stores = StoresInfo::new();
    for id in [1, 3] {
        stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            labels: if id == 3 {
                vec![StoreLabel {
                    key: "engine".into(),
                    value: "tiflash".into(),
                }]
            } else {
                vec![]
            },
            ..Default::default()
        })));
    }
    let region = RegionMeta {
        id: 7,
        epoch: Some(RegionEpoch {
            version: 10,
            conf_ver: 1,
        }),
        peers: vec![
            Peer {
                id: 11,
                store_id: 1,
                ..Default::default()
            },
            Peer {
                id: 13,
                store_id: 3,
                role: PeerRole::Learner,
                ..Default::default()
            },
            Peer {
                id: 14,
                store_id: 4,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    for store in [1, 3] {
        c.expected_reports.insert(
            store,
            Some(StoreReport {
                step: 21,
                peers: vec![PeerReport {
                    region: Some(region.clone()),
                    raft: Some(RaftState {
                        term: 1,
                        last_index: if store == 3 { 12 } else { 10 },
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
fn mcs_021_behavior() {
    let (c, index, stores) = fixture();
    let batch = c
        .generate_tombstone_tiflash_learner_plan(&index, &stores)
        .unwrap();
    assert_eq!(
        batch
            .plans
            .get(&3)
            .expect("fresher TiFlash learner must be cleaned before selecting a voter")
            .tombstones,
        vec![7]
    );
    assert!(!batch.plans.contains_key(&1));
    assert_eq!(batch.affected_regions, [7].into());
}
#[test]
fn mcs_021_tikv_fresher_ties_and_healthy_quorum_are_not_deleted() {
    let (mut c, _, stores) = fixture();
    for last in [20, 12] {
        c.expected_reports
            .get_mut(&1)
            .unwrap()
            .as_mut()
            .unwrap()
            .peers[0]
            .raft
            .as_mut()
            .unwrap()
            .last_index = last;
        let index = c.build_region_index().unwrap();
        assert!(
            !c.generate_tombstone_tiflash_learner_plan(&index, &stores)
                .unwrap()
                .has_work()
        );
    }
    c.expected_reports.insert(2, Some(StoreReport::default()));
    stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 2,
        ..Default::default()
    })));
    for report in c.expected_reports.values_mut().flatten() {
        for peer in &mut report.peers {
            peer.region.as_mut().unwrap().peers.push(Peer {
                id: 12,
                store_id: 2,
                ..Default::default()
            });
        }
    }
    let index = c.build_region_index().unwrap();
    assert!(
        !c.generate_tombstone_tiflash_learner_plan(&index, &stores)
            .unwrap()
            .has_work()
    );
}
#[test]
fn mcs_021_other_engines_error_state_and_missing_internal_peer_are_safe() {
    let (mut c, mut index, stores) = fixture();
    for engine in ["tiflash_compute", "other"] {
        stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id: 3,
            labels: vec![StoreLabel {
                key: "engine".into(),
                value: engine.into(),
            }],
            ..Default::default()
        })));
        assert!(
            !c.generate_tombstone_tiflash_learner_plan(&index, &stores)
                .unwrap()
                .has_work()
        );
    }
    c.recovery_error = Some(RecoveryError::Backend("aborted".into()));
    assert!(
        !c.generate_tombstone_tiflash_learner_plan(&index, &StoresInfo::new())
            .unwrap()
            .has_work()
    );
    c.recovery_error = None;
    index.peers.remove(&7);
    assert!(
        c.generate_tombstone_tiflash_learner_plan(&index, &stores)
            .unwrap_err()
            .to_string()
            .starts_with("can't select leader for region 7")
    );
}
#[test]
fn mcs_021_cleanup_priority_and_affected_metadata_use_latest_epoch() {
    let (mut c, _, stores) = fixture();
    c.expected_reports
        .get_mut(&1)
        .unwrap()
        .as_mut()
        .unwrap()
        .peers[0]
        .region
        .as_mut()
        .unwrap()
        .key_range
        .start_key = astersql_pd_codec::encode_bytes(&astersql_pd_codec::generate_table_key(55));
    let flash = c
        .expected_reports
        .get_mut(&3)
        .unwrap()
        .as_mut()
        .unwrap()
        .peers[0]
        .region
        .as_mut()
        .unwrap();
    flash.epoch.as_mut().unwrap().version = 9;
    flash.key_range.start_key = astersql_pd_codec::encode_bytes(b"m");
    let index = c.build_region_index().unwrap();
    let prepared = c.prepare_recovery_plan(&index, &stores).unwrap().unwrap();
    assert_eq!(prepared.stage, Stage::TombstoneTiFlashLearner);
    assert_eq!(prepared.batch.affected_table_ids, [55].into());
    assert!(prepared.batch.affected_meta_regions.is_empty());
}
#[test]
fn mcs_021_removed_flash_replica_remains_orphan_for_subsequent_recovery() {
    let (mut c, _, stores) = fixture();
    c.expected_reports
        .get_mut(&3)
        .unwrap()
        .as_mut()
        .unwrap()
        .peers
        .clear();
    let index = c.build_region_index().unwrap();
    assert_eq!(index.orphaned[&7][0].store_id, 3);
    let prepared = c.prepare_recovery_plan(&index, &stores).unwrap().unwrap();
    assert_eq!(prepared.stage, Stage::ForceLeader);
    assert_eq!(
        prepared.batch.plans[&1]
            .force_leader
            .as_ref()
            .unwrap()
            .failed_stores,
        vec![3, 4]
    );
    c.expected_reports
        .get_mut(&1)
        .unwrap()
        .as_mut()
        .unwrap()
        .peers[0]
        .is_force_leader = true;
    let prepared = c
        .prepare_recovery_plan(&c.build_region_index().unwrap(), &stores)
        .unwrap()
        .unwrap();
    assert_eq!(prepared.stage, Stage::DemoteFailedVoter);
    let failed = &prepared.batch.plans[&1].demotes[0].failed_voters;
    assert_eq!(
        failed.iter().map(|p| p.store_id).collect::<Vec<_>>(),
        vec![3, 4]
    );
    assert_eq!(failed[0].role, PeerRole::Learner);
}
