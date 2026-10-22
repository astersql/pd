// Copyright 2026 AsterSQL.
use crate::{KeyRange, RegionEpoch, RegionInfo, RegionMeta, split_regions};
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    name: String,
    start: Vec<u8>,
    end: Vec<u8>,
    middle: u8,
    id: u64,
    version: u64,
    left_id: u64,
    right_id: u64,
    next_version: u64,
}

fn region(case: &Case) -> RegionInfo {
    RegionInfo::new(
        RegionMeta {
            id: case.id,
            key_range: KeyRange {
                start_key: case.start.clone(),
                end_key: case.end.clone(),
            },
            epoch: Some(RegionEpoch {
                conf_ver: 9,
                version: case.version,
            }),
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    )
}

#[test]
fn core_task_37_boundaries_epoch_ids_and_owned_clones() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_37.json")).unwrap();
    let cases: Vec<Case> = serde_json::from_value(fixture["cases"].clone()).unwrap();
    for case in cases {
        let input = region(&case);
        let mut output = split_regions(std::slice::from_ref(&input));
        assert_eq!(output.len(), 2, "{}", case.name);
        assert_eq!(output[0].meta().id, case.left_id);
        assert_eq!(output[1].meta().id, case.right_id);
        assert_eq!(
            output[0].meta().key_range,
            KeyRange {
                start_key: case.start.clone(),
                end_key: vec![case.middle]
            }
        );
        assert_eq!(
            output[1].meta().key_range,
            KeyRange {
                start_key: vec![case.middle],
                end_key: case.end.clone()
            }
        );
        for child in &output {
            assert_eq!(
                child.meta().epoch,
                Some(RegionEpoch {
                    conf_ver: 9,
                    version: case.next_version
                })
            );
        }
        output[0].meta.key_range.start_key.push(42);
        assert_eq!(input.meta().key_range.start_key, case.start);
        assert_eq!(input.meta().epoch.unwrap().version, case.version);
        assert_eq!(output[1].meta().key_range.start_key, vec![case.middle]);
    }
}

#[test]
fn core_task_37_empty_and_batch_offsets() {
    assert!(split_regions(&[]).is_empty());
    let input: Vec<_> = [10, 20]
        .map(|id| {
            RegionInfo::new(
                RegionMeta {
                    id,
                    epoch: Some(RegionEpoch::default()),
                    ..Default::default()
                },
                None,
                vec![],
                vec![],
            )
        })
        .into();
    let output = split_regions(&input);
    assert_eq!(
        output.iter().map(|r| r.meta().id).collect::<Vec<_>>(),
        vec![12, 14, 22, 24]
    );
}

#[test]
#[should_panic(expected = "split requires a region epoch")]
fn core_task_37_absent_epoch_panics_like_go() {
    split_regions(&[RegionInfo::new(RegionMeta::default(), None, vec![], vec![])]);
}
