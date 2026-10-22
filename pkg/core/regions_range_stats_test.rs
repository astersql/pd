// Copyright 2026 AsterSQL.
use crate::*;
use std::sync::Arc;
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("rust-fixtures/core_task_54.json")).unwrap()
}
fn registry() -> RegionsInfo {
    let mut r = RegionsInfo::new();
    for row in fixture()["regions"].as_array().unwrap() {
        let peers = vec![
            Peer {
                id: 1,
                store_id: 1,
                role: PeerRole::Voter,
                is_witness: false,
            },
            Peer {
                id: 2,
                store_id: 2,
                role: PeerRole::Learner,
                is_witness: false,
            },
        ];
        let mut region = RegionInfo::new(
            RegionMeta {
                id: row[0].as_u64().unwrap(),
                key_range: KeyRange {
                    start_key: row[1].as_str().unwrap().as_bytes().to_vec(),
                    end_key: row[2].as_str().unwrap().as_bytes().to_vec(),
                },
                peers: peers.clone(),
                ..Default::default()
            },
            Some(peers[0].clone()),
            vec![],
            vec![],
        );
        region.stats.approximate_size = row[3].as_i64().unwrap();
        let region = Arc::new(region);
        let (old, overlaps, changed) = r.set_region(region.clone());
        r.update_sub_tree(region, old.as_deref(), &overlaps, changed);
    }
    r
}
#[test]
fn core_task_54_sizes() {
    let r = registry();
    for c in fixture()["sizes"].as_array().unwrap() {
        assert_eq!(
            r.get_region_size_by_range(
                c[0].as_str().unwrap().as_bytes(),
                c[1].as_str().unwrap().as_bytes()
            ),
            c[2].as_i64().unwrap(),
            "{c}"
        );
    }
    assert_eq!(RegionsInfo::new().get_region_size_by_range(b"", b""), 0);
}
#[test]
fn core_task_54_counts() {
    let r = registry();
    for c in fixture()["count_cases"].as_array().unwrap() {
        let (a, b, n) = (
            c[0].as_str().unwrap().as_bytes(),
            c[1].as_str().unwrap().as_bytes(),
            c[2].as_i64().unwrap() as isize,
        );
        assert_eq!(r.get_region_count_by_range(a, b), n);
        assert_eq!(r.store_role_count_by_range(1, RegionRole::Leader, a, b), n);
        assert_eq!(r.store_role_count_by_range(2, RegionRole::Learner, a, b), n);
        assert_eq!(r.store_peer_count_by_range(1, a, b), n);
        assert_eq!(r.store_peer_count_by_range(2, a, b), n);
        assert_eq!(r.store_peer_count_by_range(999, a, b), 0);
    }
}
#[test]
fn core_task_54_holes_and_neighbors() {
    let r = registry();
    let holes: Vec<[String; 2]> = serde_json::from_value(fixture()["holes"].clone()).unwrap();
    assert_eq!(r.get_range_holes(), holes);
    assert!(RegionsInfo::new().get_range_holes().is_empty());
    let a = r.get_region(1).unwrap();
    let b = r.get_region(2).unwrap();
    let c = r.get_region(3).unwrap();
    let (prev, next) = r.get_adjacent_regions(&a);
    assert!(prev.is_none());
    assert!(Arc::ptr_eq(&next.unwrap(), &b));
    let (prev, next) = r.get_adjacent_regions(&b);
    assert!(Arc::ptr_eq(&prev.unwrap(), &a));
    assert!(next.is_none());
    let (prev, next) = r.get_adjacent_regions(&c);
    assert!(prev.is_none());
    assert!(next.is_none());
    let mut bounded = RegionsInfo::new();
    bounded.set_region(b.clone());
    assert_eq!(
        bounded.get_range_holes(),
        vec![
            ["".to_owned(), "64".to_owned()],
            ["66".to_owned(), "".to_owned()]
        ]
    );
}

#[test]
fn core_task_54_size_across_go_scan_batches() {
    let mut r = RegionsInfo::new();
    for id in 0..1001u64 {
        let mut region = RegionInfo::new(
            RegionMeta {
                id: id + 1,
                key_range: KeyRange {
                    start_key: format!("{id:08}").into_bytes(),
                    end_key: if id == 1000 {
                        vec![]
                    } else {
                        format!("{:08}", id + 1).into_bytes()
                    },
                },
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        );
        region.stats.approximate_size = 10;
        r.set_region(Arc::new(region));
    }
    assert_eq!(r.get_region_size_by_range(b"", b"00001001"), 10010);
    assert_eq!(r.get_region_size_by_range(b"00000001", b""), 10000);
    assert_eq!(r.get_region_size_by_range(b"", b"00000010"), 100);
}
