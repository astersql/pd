// Copyright 2026 AsterSQL.
use crate::*;
use astersql_pd_core::{RegionBucketMeta, RegionInfo as CoreRegion, RegionMeta};
fn region(id: u64) -> CoreRegion {
    CoreRegion::new(
        RegionMeta {
            id,
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    )
}
#[test]
fn core_task_65_empty_even_when_cancelled() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_65.json")).unwrap();
    for cancelled in [false, true] {
        let bytes = marshal_regions_info_json(&[], || {
            cancelled.then_some(RegionListCancellation::Cancelled)
        })
        .unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
            fixture["empty"]
        );
    }
}
#[test]
fn core_task_65_order_duplicates_and_reuse() {
    let mut a = region(7);
    a.set_bucket_meta(Some(RegionBucketMeta {
        version: 9,
        keys: vec![vec![255]],
    }));
    let b = region(2);
    let bytes = marshal_regions_info_json(&[Some(&a), Some(&b), Some(&a), None], || None).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_65.json")).unwrap();
    assert_eq!(value["count"], 4);
    let entries = value["regions"].as_array().unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|r| r["id"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        serde_json::from_value::<Vec<u64>>(fixture["ids"].clone()).unwrap()
    );
    assert_eq!(entries[0], entries[2]);
    assert_eq!(entries[2], entries[3]);
    assert!(entries[1].get("buckets").is_none());
    assert_eq!(entries[1]["bucket_version"], 9);
}
#[test]
fn core_task_65_cancel_before_or_during_list_discards_output() {
    let r = region(7);
    for reason in [
        RegionListCancellation::Cancelled,
        RegionListCancellation::DeadlineExceeded,
    ] {
        for cancel_at in [0, 1] {
            let mut visited = 0;
            let result = marshal_regions_info_json(&[Some(&r), Some(&r)], || {
                let cancel = visited == cancel_at;
                visited += 1;
                cancel.then_some(reason)
            });
            assert!(matches!(result, Err(RegionListError::Cancelled(actual)) if actual == reason));
            assert_eq!(visited, cancel_at + 1);
        }
    }
}
