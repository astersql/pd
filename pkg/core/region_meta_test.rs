// Copyright 2026 AsterSQL.
use super::{KeyRange, RegionEpoch, RegionId, RegionMeta};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    ranges: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    start: Vec<u8>,
    end: Vec<u8>,
    key: Vec<u8>,
    contains: bool,
    source: String,
}

#[test]
fn core_task_10_key_ranges() {
    let fixture: Fixture =
        serde_json::from_str(include_str!("rust-fixtures/core_task_10.json")).unwrap();
    assert_eq!(fixture.ranges.len(), 12);
    for case in fixture.ranges {
        let range = KeyRange {
            start_key: case.start,
            end_key: case.end,
        };
        assert_eq!(range.contains(&case.key), case.contains, "{}", case.source);
    }
}

#[test]
fn core_task_10_owned_metadata_and_u64_boundaries() {
    let id: RegionId = u64::MAX;
    let meta = RegionMeta {
        id,
        key_range: KeyRange {
            start_key: vec![0, 255],
            end_key: vec![],
        },
        epoch: Some(RegionEpoch {
            conf_ver: u64::MAX,
            version: u64::MAX,
        }),
        peers: vec![super::Peer {
            id: u64::MAX,
            ..Default::default()
        }],
    };
    let mut copy = meta.clone();
    copy.key_range.start_key[0] = 1;
    copy.peers[0].id = 0;
    assert_eq!(meta.id, u64::MAX);
    assert_eq!(meta.epoch.unwrap().conf_ver, u64::MAX);
    assert_eq!(meta.epoch.unwrap().version, u64::MAX);
    assert_eq!(meta.key_range.start_key, [0, 255]);
    assert_eq!(meta.peers[0].id, u64::MAX);
    assert_ne!(copy, meta);
}

#[test]
fn core_task_10_defaults_preserve_absent_epoch() {
    let meta = RegionMeta::default();
    assert_eq!(meta.id, 0);
    assert_eq!(meta.epoch, None);
    assert!(meta.peers.is_empty());
    assert!(meta.key_range.contains(&[]));
    assert!(meta.key_range.contains(&[255]));
    assert_eq!(
        RegionEpoch::default(),
        RegionEpoch {
            conf_ver: 0,
            version: 0
        }
    );
}
