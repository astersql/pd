// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{KeyRange, Peer, RegionEpoch, RegionMeta};
use std::sync::Arc;
fn peer(
    id: u64,
    start: &[u8],
    end: &[u8],
    version: u64,
    conf_ver: u64,
    initialized: bool,
) -> PeerReport {
    PeerReport {
        region: Some(RegionMeta {
            id,
            key_range: KeyRange {
                start_key: start.to_vec(),
                end_key: end.to_vec(),
            },
            epoch: Some(RegionEpoch { version, conf_ver }),
            peers: if initialized {
                vec![Peer {
                    id: id + 1,
                    store_id: 1,
                    ..Default::default()
                }]
            } else {
                vec![]
            },
            ..Default::default()
        }),
        ..Default::default()
    }
}
#[test]
fn mcs_018_behavior() {
    let mut c = Controller::new(Arc::new(RecoverySteps::new(18)));
    c.expected_reports.insert(
        1,
        Some(StoreReport {
            step: 18,
            peers: vec![
                peer(100, b"", b"b", 1, 10, true),
                peer(200, b"a", b"", 10, 1, true),
                peer(300, b"", b"a", 11, 2, false),
            ],
        }),
    );
    let index = c.build_region_index().unwrap();
    assert_eq!(
        index.regions.len(),
        1,
        "higher epoch overlap must win, uninitialized peer must not enter tree"
    );
    assert!(index.regions.contains(200));
    assert!(!index.regions.contains(100));
    assert!(!index.regions.contains(300));
    assert_eq!(index.peers.len(), 3);
}
#[test]
fn mcs_018_half_open_index_rejects_wrong_order_and_duplicate_id() {
    let item = |id, start: &[u8], end: &[u8], version| {
        Arc::new(RegionItem::new(1, peer(id, start, end, version, 1, true)).unwrap())
    };
    let mut index = RegionIndex::default();
    assert!(index.insert(item(1, b"", b"b", 10)).unwrap());
    assert!(index.insert(item(2, b"b", b"", 9)).unwrap());
    assert_eq!(
        index
            .overlaps(item(9, b"a", b"c", 1).region())
            .iter()
            .map(|r| r.region().id)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert!(!index.insert(item(3, b"b", b"c", 8)).unwrap());
    assert!(
        index
            .insert(item(1, b"x", b"z", 1))
            .unwrap_err()
            .to_string()
            .contains("shouldn't be updated twice")
    );
    assert!(index.insert(item(4, b"a", b"c", 11)).is_err());
    assert_eq!(index.len(), 2);
    let empty = vec![
        peer(10, b"b", b"d", 1, 1, true).region.unwrap(),
        peer(11, b"f", b"", 1, 1, true).region.unwrap(),
    ];
    assert!(first_empty_overlap(&empty, item(9, b"", b"b", 1).region()).is_none());
    assert!(first_empty_overlap(&empty, item(9, b"d", b"f", 1).region()).is_none());
    assert_eq!(
        first_empty_overlap(&empty, item(9, b"c", b"g", 1).region())
            .unwrap()
            .id,
        10
    );
    assert_eq!(
        first_empty_overlap(&empty, item(9, b"z", b"", 1).region())
            .unwrap()
            .id,
        11
    );
}
#[test]
fn mcs_018_latest_epoch_orphans_and_nonoverlap_fast_path() {
    let mut c = Controller::new(Arc::new(RecoverySteps::new(18)));
    let mut old = peer(7, b"x", b"z", 9, 1, true);
    old.region.as_mut().unwrap().peers.push(Peer {
        id: 8,
        store_id: 2,
        ..Default::default()
    });
    let mut latest = old.clone();
    latest
        .region
        .as_mut()
        .unwrap()
        .epoch
        .as_mut()
        .unwrap()
        .conf_ver = 2;
    c.expected_reports.insert(
        1,
        Some(StoreReport {
            step: 18,
            peers: vec![old, latest],
        }),
    );
    c.expected_reports.insert(
        2,
        Some(StoreReport {
            step: 18,
            peers: vec![],
        }),
    );
    let index = c.build_region_index().unwrap();
    assert_eq!(
        index
            .regions
            .items()
            .next()
            .unwrap()
            .region()
            .epoch
            .unwrap()
            .conf_ver,
        2
    );
    assert_eq!(index.orphaned[&7][0].store_id, 2);
    let empty = vec![peer(100, b"b", b"d", 1, 1, true).region.unwrap()];
    assert!(index.find_overlap_with_empty(&empty).is_none());
    c.failed_stores.insert(2);
    assert!(c.build_region_index().unwrap().orphaned.is_empty());
    c.auto_detect = true;
    c.failed_stores.clear();
    c.expected_reports.remove(&2);
    assert!(c.build_region_index().unwrap().orphaned.is_empty());
    c.expected_reports.insert(3, None);
    assert!(c.build_region_index().is_err());
}
#[test]
fn mcs_018_raft_freshness_covers_go_source_select_leader_cases() {
    use astersql_pd_core::{StoreInfo, StoreLabel, StoreMeta, StoresInfo};
    let stores = StoresInfo::new();
    for id in 1..=6 {
        stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            labels: if id == 6 {
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
    let cases = [
        (vec![(1, 1, 10, 5, 4), (2, 2, 9, 9, 8)], 2),
        (
            vec![(1, 1, 10, 10, 9), (2, 1, 8, 8, 15), (3, 1, 12, 11, 11)],
            2,
        ),
        (
            vec![(1, 1, 9, 9, 11), (2, 1, 10, 8, 7), (3, 1, 11, 7, 6)],
            3,
        ),
        (
            vec![(1, 1, 11, 11, 8), (2, 1, 11, 10, 10), (3, 1, 11, 9, 8)],
            1,
        ),
        (
            vec![
                (6, 1, 11, 11, 9),
                (1, 1, 11, 11, 8),
                (2, 1, 11, 10, 10),
                (3, 1, 11, 9, 8),
            ],
            1,
        ),
    ];
    for (data, expected) in cases {
        let mut index = RecoveryIndex::default();
        for (store, term, last, commit, applied) in data {
            let mut p = peer(7, b"", b"", 1, 1, true);
            p.raft = Some(RaftState {
                term,
                vote: 0,
                commit,
                last_index: last,
            });
            p.applied_index = applied;
            index
                .peers
                .entry(7)
                .or_default()
                .push(Arc::new(RegionItem::new(store, p).unwrap()));
        }
        assert_eq!(
            index.select_leader(7, &stores).unwrap().unwrap().store_id,
            expected
        );
    }
}
#[test]
fn mcs_018_latest_uninitialized_peer_masks_old_initialized_epoch() {
    let mut c = Controller::new(Arc::new(RecoverySteps::new(18)));
    c.expected_reports.insert(
        1,
        Some(StoreReport {
            step: 18,
            peers: vec![peer(7, b"a", b"z", 9, 1, true)],
        }),
    );
    c.expected_reports.insert(
        2,
        Some(StoreReport {
            step: 18,
            peers: vec![peer(7, b"a", b"z", 10, 1, false)],
        }),
    );
    let index = c.build_region_index().unwrap();
    assert!(index.regions.is_empty());
    assert_eq!(index.peers[&7].len(), 2);
    let empty = vec![peer(99, b"b", b"c", 1, 1, true).region.unwrap()];
    assert_eq!(index.find_overlap_with_empty(&empty).unwrap().0.store_id, 1);
    assert_eq!(index.find_overlap_with_empty(&empty).unwrap().1.id, 99);
}
#[test]
fn mcs_018_equal_raft_prefers_voter_and_signed_wrap_matches_source() {
    use astersql_pd_core::{PeerRole, StoreInfo, StoreMeta, StoresInfo};
    let stores = StoresInfo::new();
    for id in 1..=2 {
        stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        })));
    }
    for role in [PeerRole::Learner, PeerRole::DemotingVoter] {
        let mut learner = peer(7, b"", b"", 1, 1, true);
        learner.region.as_mut().unwrap().peers[0].role = role;
        let mut voter = peer(7, b"", b"", 1, 1, true);
        voter.region.as_mut().unwrap().peers[0].store_id = 2;
        let a = Arc::new(RegionItem::new(1, learner).unwrap());
        let b = Arc::new(RegionItem::new(2, voter).unwrap());
        for peers in [vec![a.clone(), b.clone()], vec![b.clone(), a.clone()]] {
            let mut index = RecoveryIndex::default();
            index.peers.insert(7, peers);
            assert_eq!(
                index.select_leader(7, &stores).unwrap().unwrap().store_id,
                2
            );
        }
    }
    let mut high = peer(7, b"", b"", 1, 1, true);
    high.raft = Some(RaftState {
        term: u64::MAX,
        ..Default::default()
    });
    let mut low = high.clone();
    low.raft.as_mut().unwrap().term = 1;
    let a = RegionItem::new(1, high).unwrap();
    let b = RegionItem::new(2, low).unwrap();
    assert!(a.is_raft_stale(&b, &stores).unwrap());
    assert!(RegionItem::new(1, PeerReport::default()).is_err());
}
