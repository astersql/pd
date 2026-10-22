// Copyright 2026 AsterSQL.
use crate::*;
use std::sync::Arc;
fn registry() -> RegionsInfo {
    let mut r = RegionsInfo::new();
    for (id, start, end) in [
        (1, "a", "b"),
        (2, "b", "c"),
        (3, "d", "f"),
        (4, "f", "i"),
        (6, "l", ""),
    ] {
        r.set_region(Arc::new(RegionInfo::new(
            RegionMeta {
                id,
                key_range: KeyRange {
                    start_key: start.as_bytes().to_vec(),
                    end_key: end.as_bytes().to_vec(),
                },
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        )));
    }
    r
}
#[test]
fn core_task_51_ranges_and_limits() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_51.json")).unwrap();
    let r = registry();
    for c in f["cases"].as_array().unwrap() {
        let actual: Vec<_> = r
            .scan_regions(
                c["start"].as_str().unwrap().as_bytes(),
                c["end"].as_str().unwrap().as_bytes(),
                c["limit"].as_i64().unwrap() as isize,
            )
            .iter()
            .map(|v| v.meta().id)
            .collect();
        assert_eq!(serde_json::to_value(actual).unwrap(), c["ids"], "{c}");
    }
}
#[test]
fn core_task_51_iterator_identity_and_stop() {
    let r = registry();
    for (start, count, expected) in [
        ("c", 1, vec![3]),
        ("e", 2, vec![3, 4]),
        ("z", 9, vec![6]),
        ("i", 9, vec![6]),
    ] {
        let mut ids = vec![];
        r.scan_region_with_iterator(start.as_bytes(), |region| {
            assert!(Arc::ptr_eq(
                region,
                &r.get_region(region.meta().id).unwrap()
            ));
            ids.push(region.meta().id);
            ids.len() < count
        });
        assert_eq!(ids, expected);
    }
}
#[test]
fn core_task_51_empty_and_finite_tail() {
    let mut r = registry();
    r.remove_region(&r.get_region(6).unwrap());
    assert!(r.scan_regions(b"z", b"", 0).is_empty());
    let empty = RegionsInfo::new();
    assert!(empty.scan_regions(b"", b"", -1).is_empty());
    empty.scan_region_with_iterator(b"", |_| panic!("empty registry called iterator"));
}
