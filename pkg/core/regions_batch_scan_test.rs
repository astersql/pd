// Copyright 2026 AsterSQL.
use crate::*;
use std::sync::Arc;
fn range(a: &str, b: &str) -> KeyRange {
    KeyRange {
        start_key: a.as_bytes().to_vec(),
        end_key: b.as_bytes().to_vec(),
    }
}
#[test]
fn core_task_52_fixture_behavior() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_52.json")).unwrap();
    for c in f["cases"].as_array().unwrap() {
        let mut registry = RegionsInfo::new();
        if !c["empty"].as_bool().unwrap() {
            for r in f["regions"].as_array().unwrap() {
                registry.set_region(Arc::new(RegionInfo::new(
                    RegionMeta {
                        id: r[0].as_u64().unwrap(),
                        key_range: range(r[1].as_str().unwrap(), r[2].as_str().unwrap()),
                        ..Default::default()
                    },
                    None,
                    vec![],
                    vec![],
                )));
            }
        }
        let ranges: Vec<_> = c["ranges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| range(r[0].as_str().unwrap(), r[1].as_str().unwrap()))
            .collect();
        let result = registry.batch_scan_regions(
            &ranges,
            BatchScanRegionsOptions {
                limit: c["limit"].as_i64().unwrap() as isize,
                output_must_contain_all_key_range: c["strict"].as_bool().unwrap(),
            },
        );
        assert_eq!(result.is_err(), c["error"].as_bool().unwrap(), "{c}");
        match result {
            Ok(regions) => {
                let ids: Vec<_> = regions
                    .iter()
                    .map(|r| {
                        assert!(Arc::ptr_eq(r, &registry.get_region(r.meta().id).unwrap()));
                        r.meta().id
                    })
                    .collect();
                assert_eq!(serde_json::to_value(ids).unwrap(), c["ids"], "{c}");
            }
            Err(e) => assert_eq!(e.code(), "ErrRegionNotAdjacent"),
        }
    }
}
#[test]
fn core_task_52_go_limit_matrix() {
    let mut registry = RegionsInfo::new();
    for (prefix, offset) in [("a", 0), ("b", 9)] {
        for i in 0..9 {
            registry.set_region(Arc::new(RegionInfo::new(
                RegionMeta {
                    id: offset + i,
                    key_range: range(&format!("{prefix}{i}"), &format!("{prefix}{}", i + 1)),
                    ..Default::default()
                },
                None,
                vec![],
                vec![],
            )));
        }
    }
    for limit in 1..=18 {
        let result = registry
            .batch_scan_regions(
                &[range("a", "b"), range("b0", "")],
                BatchScanRegionsOptions {
                    limit,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(result.len(), limit as usize);
        assert_eq!(
            result.iter().map(|r| r.meta().id).collect::<Vec<_>>(),
            (0..limit as u64).collect::<Vec<_>>()
        );
    }
}
