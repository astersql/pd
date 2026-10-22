// Copyright 2026 AsterSQL.
use crate::*;
use std::sync::Arc;
fn region(v: &serde_json::Value) -> Arc<RegionInfo> {
    Arc::new(RegionInfo::new(
        RegionMeta {
            id: v[0].as_u64().unwrap(),
            key_range: KeyRange {
                start_key: v[1].as_str().unwrap().as_bytes().to_vec(),
                end_key: v[2].as_str().unwrap().as_bytes().to_vec(),
            },
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    ))
}
fn ids(regions: Vec<Arc<RegionInfo>>) -> Vec<u64> {
    regions.iter().map(|r| r.meta().id).collect()
}
#[test]
fn core_task_40_fixture_mutations() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_40.json")).unwrap();
    for case in f["cases"].as_array().unwrap() {
        let mut tree = RegionTree::new();
        for step in case["steps"].as_array().unwrap() {
            let r = region(&step["region"]);
            if step["op"] == "update" {
                let expected: Vec<u64> = serde_json::from_value(step["overlaps"].clone()).unwrap();
                assert_eq!(ids(tree.overlaps(&r)), expected, "{}", case["name"]);
                assert_eq!(ids(tree.update(r.clone())), expected);
                assert!(Arc::ptr_eq(
                    &tree.search(&r.meta().key_range.start_key).unwrap(),
                    &r
                ));
            } else {
                tree.remove(&r);
            }
            let actual: Vec<_> = case["keys"]
                .as_array()
                .unwrap()
                .iter()
                .map(|k| {
                    tree.search(k.as_str().unwrap().as_bytes())
                        .map(|r| r.meta().id)
                })
                .collect();
            let expected: Vec<Option<u64>> =
                serde_json::from_value(step["search"].clone()).unwrap();
            assert_eq!(actual, expected, "{}: {step}", case["name"]);
        }
    }
}

#[test]
fn core_task_40_repeated_split_merge() {
    // Mirrors TestRegionTreeSplitAndMerge's seven splits, seven merges, then
    // fourteen mixed rounds. Check every byte, not only each range's start.
    let mut meta = RegionMeta::default();
    meta.epoch = Some(RegionEpoch::default());
    let mut regions = vec![RegionInfo::new(meta, None, vec![], vec![])];
    let mut tree = RegionTree::new();
    for round in 0..28 {
        regions = if round < 7 || (round >= 14 && (round - 14 + 1) % 3 != 0) {
            split_regions(&regions)
        } else {
            merge_regions(&mut regions)
        };
        for r in &regions {
            tree.update(Arc::new(r.clone_with([])));
        }
        assert_eq!(tree.tree.len(), regions.len());
        for byte in 0..=255 {
            let key = [byte];
            let expected = regions.iter().find(|r| r.meta().key_range.contains(&key));
            assert_eq!(
                tree.search(&key).map(|r| r.meta().id),
                expected.map(|r| r.meta().id)
            );
        }
        for r in &regions {
            assert_eq!(ids(tree.overlaps(r)), vec![r.meta().id]);
        }
    }
}
