// Copyright 2026 AsterSQL.
use crate::*;
fn input(id: u64, version: Option<u64>, conf: u64, start: Vec<u8>, end: Vec<u8>) -> RegionInfo {
    let peer = Peer {
        id,
        store_id: id,
        role: PeerRole::Learner,
        is_witness: true,
    };
    RegionInfo::new(
        RegionMeta {
            id,
            key_range: KeyRange {
                start_key: start,
                end_key: end,
            },
            epoch: version.map(|version| RegionEpoch {
                version,
                conf_ver: conf,
            }),
            peers: vec![peer.clone()],
            is_in_flashback: true,
            ..Default::default()
        },
        Some(peer),
        vec![],
        vec![],
    )
}
#[test]
fn core_task_38_pair_metadata_and_epoch_side_effects() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_38.json")).unwrap();
    for c in f["cases"].as_array().unwrap() {
        let n = |k: &str| c[k].as_u64().unwrap();
        let bytes = |k: &str| serde_json::from_value(c[k].clone()).unwrap();
        let mut inputs = vec![
            input(
                5,
                Some(n("left_version")),
                n("left_conf"),
                vec![],
                bytes("left_end"),
            ),
            input(
                6,
                Some(n("right_version")),
                n("right_conf"),
                bytes("right_start"),
                vec![],
            ),
        ];
        inputs[0].term = 42;
        let result = merge_regions(&mut inputs);
        assert_eq!(result.len(), 1, "{}", c["name"]);
        let r = &result[0];
        assert_eq!(r.meta().id, 5);
        assert!(r.meta().key_range.start_key.is_empty());
        assert!(r.meta().key_range.end_key.is_empty());
        assert_eq!(r.meta().peers, inputs[0].meta().peers);
        assert_eq!(r.leader(), inputs[0].leader());
        assert_eq!(
            r.meta().epoch,
            Some(RegionEpoch {
                version: n("version"),
                conf_ver: n("conf")
            })
        );
        assert_eq!(inputs[n("selected") as usize].meta().epoch, r.meta().epoch);
        let other = 1 - n("selected") as usize;
        assert_eq!(
            inputs[other].meta().epoch.unwrap().version,
            if other == 0 {
                n("left_version")
            } else {
                n("right_version")
            }
        );
        assert!(r.voters().is_empty());
        assert!(r.learners().is_empty());
        assert!(r.witnesses().is_empty());
        assert_eq!(r.term, 0);
        assert!(!r.meta().is_in_flashback);
    }
}
#[test]
fn core_task_38_empty_and_odd_tail() {
    assert!(merge_regions(&mut []).is_empty());
    let mut inputs = vec![
        input(1, Some(1), 1, vec![1], vec![2]),
        input(2, Some(2), 2, vec![2], vec![3]),
        input(3, Some(4), 3, vec![9], vec![]),
    ];
    let mut out = merge_regions(&mut inputs);
    assert_eq!(out.len(), 2);
    assert_eq!(
        out[0].meta().key_range,
        KeyRange {
            start_key: vec![1],
            end_key: vec![3]
        }
    );
    assert!(out[1].meta().key_range.end_key.is_empty());
    assert_eq!(out[1].meta().id, 3);
    assert_eq!(out[1].meta().epoch.unwrap().version, 5);
    assert_eq!(out[1].meta().key_range.start_key, vec![9]);
    out[1].meta.peers.clear();
    assert_eq!(inputs[2].meta().peers.len(), 1);
}
#[test]
#[should_panic(expected = "merge requires a selected region epoch")]
fn core_task_38_absent_selected_epoch() {
    merge_regions(&mut [input(1, None, 0, vec![], vec![])]);
}
#[test]
fn core_task_38_absent_unselected_epoch() {
    let mut inputs = [
        input(1, None, 0, vec![], vec![1]),
        input(2, Some(1), 2, vec![1], vec![]),
    ];
    assert_eq!(
        merge_regions(&mut inputs)[0].meta().epoch.unwrap().version,
        2
    );
    assert_eq!(inputs[0].meta().epoch, None);
}
