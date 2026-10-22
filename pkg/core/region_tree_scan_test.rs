// Copyright 2026 AsterSQL.
use crate::*;
use std::sync::Arc;
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("rust-fixtures/core_task_42.json")).unwrap()
}
fn tree(f: &serde_json::Value) -> RegionTree {
    RegionTree::from_regions(f["regions"].as_array().unwrap().iter().map(|r| {
        Arc::new(RegionInfo::new(
            RegionMeta {
                id: r[0].as_u64().unwrap(),
                key_range: KeyRange {
                    start_key: r[1].as_str().unwrap().as_bytes().to_vec(),
                    end_key: r[2].as_str().unwrap().as_bytes().to_vec(),
                },
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        ))
    }))
}
#[test]
fn core_task_42_scan_holes_and_boundaries() {
    let f = fixture();
    let t = tree(&f);
    for case in f["scans"].as_array().unwrap() {
        let mut ids = vec![];
        t.scan_range(case[0].as_str().unwrap().as_bytes(), |r| {
            ids.push(r.meta().id);
            true
        });
        let expected: Vec<_> = case[1]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_u64().unwrap())
            .collect();
        assert_eq!(ids, expected, "{case}");
    }
    let all = t.scan_ranges();
    assert_eq!(
        all.iter().map(|r| r.meta().id).collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
    assert!(Arc::ptr_eq(&all[0], &t.search(b"a").unwrap()));
}
#[test]
fn core_task_42_callback_stops_immediately() {
    let t = tree(&fixture());
    for limit in [1, 2, 4] {
        let mut ids = vec![];
        t.scan_range(b"b", |r| {
            ids.push(r.meta().id);
            ids.len() < limit
        });
        assert_eq!(ids, vec![1, 2, 3, 4][..limit]);
    }
}
#[test]
fn core_task_42_count_rank_semantics() {
    let f = fixture();
    let t = tree(&f);
    for case in f["counts"].as_array().unwrap() {
        assert_eq!(
            t.get_count_by_range(
                case[0].as_str().unwrap().as_bytes(),
                case[1].as_str().unwrap().as_bytes()
            ),
            case[2].as_i64().unwrap() as isize,
            "{case}"
        );
    }
}
#[test]
fn core_task_42_empty_and_no_candidate() {
    let empty = RegionTree::new();
    empty.scan_range(b"", |_| panic!("empty tree visited"));
    assert!(empty.scan_ranges().is_empty());
    assert_eq!(empty.get_count_by_range(b"", b""), 0);
    let mut f = fixture();
    f["regions"].as_array_mut().unwrap().pop();
    let t = tree(&f);
    t.scan_range(b"z", |_| panic!("after finite last region visited"));
    assert_eq!(t.get_count_by_range(b"z", b""), 0);
}
