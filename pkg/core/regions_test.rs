// Copyright 2026 AsterSQL.
use crate::*;
use std::sync::Arc;
fn region(id: u64, start: &str, end: &str, size: i64) -> Arc<RegionInfo> {
    let mut r = RegionInfo::new(
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
    );
    r.stats.approximate_size = size;
    Arc::new(r)
}
#[test]
fn core_task_44_fixture_replacements() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_44.json")).unwrap();
    let mut regions = RegionsInfo::new();
    assert!(regions.get_region(99).is_none());
    assert!(regions.get_regions().is_empty());
    for step in f["steps"].as_array().unwrap() {
        let input = &step["region"];
        let r = region(
            input[0].as_u64().unwrap(),
            input[1].as_str().unwrap(),
            input[2].as_str().unwrap(),
            input[3].as_i64().unwrap(),
        );
        let (origin, overlaps, changed) = regions.set_region(r.clone());
        assert_eq!(
            origin.as_ref().map(|r| r.stats.approximate_size),
            step["origin_size"].as_i64()
        );
        assert_eq!(changed, step["changed"].as_bool().unwrap());
        assert_eq!(
            overlaps.iter().map(|r| r.meta().id).collect::<Vec<_>>(),
            serde_json::from_value::<Vec<u64>>(step["overlaps"].clone()).unwrap()
        );
        let all = regions.get_regions();
        let mut ids = all.iter().map(|r| r.meta().id).collect::<Vec<_>>();
        ids.sort();
        assert_eq!(
            ids,
            serde_json::from_value::<Vec<u64>>(step["ids"].clone()).unwrap()
        );
        assert_eq!(regions.tree_len(), all.len());
        for r in &all {
            assert!(Arc::ptr_eq(r, &regions.get_region(r.meta().id).unwrap()));
            assert!(Arc::ptr_eq(
                r,
                &regions
                    .read()
                    .root
                    .search(&r.meta().key_range.start_key)
                    .unwrap()
            ));
        }
        assert!(Arc::ptr_eq(&r, &regions.get_region(r.meta().id).unwrap()));
        for id in 0..6 {
            assert_eq!(regions.get_region(id).is_some(), ids.contains(&id));
        }
        assert_eq!(
            regions.read().root.total_size(),
            all.iter().map(|r| r.stats.approximate_size).sum::<i64>()
        );
    }
}
#[test]
fn core_task_44_snapshot_ownership() {
    let mut regions = RegionsInfo::new();
    let old = region(0, "", "", 10);
    regions.set_region(old.clone());
    let snapshot = regions.get_regions();
    let new = region(0, "", "", 20);
    let (origin, overlaps, changed) = regions.set_region(new.clone());
    assert!(Arc::ptr_eq(&origin.unwrap(), &old));
    assert!(overlaps.is_empty());
    assert!(!changed);
    assert!(Arc::ptr_eq(&snapshot[0], &old));
    assert!(Arc::ptr_eq(&regions.get_regions()[0], &new));
    assert_eq!(regions.read().root.total_size(), 20);
}
