// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{KeyRange, RegionInfo, RegionMeta};
use serde_json::Value;

fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}
fn expected(v: &Value) -> SplitScatterRangeHint {
    SplitScatterRangeHint {
        start_key: bytes(v["start"].as_str().unwrap()),
        end_key: bytes(v["end"].as_str().unwrap()),
        scatter_group: v["group"].as_str().unwrap().into(),
    }
}
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_052.json"
    ))
    .unwrap()
}
#[test]
fn task_052_behavior_cases() {
    let data = fixture();
    for c in data["cases"].as_array().unwrap() {
        let region = RegionInfo::new(
            RegionMeta {
                id: 52,
                key_range: KeyRange {
                    start_key: bytes(c["start"].as_str().unwrap()),
                    end_key: bytes(c["end"].as_str().unwrap()),
                },
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        );
        let validate = |id: u32| {
            c["valid"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v.as_u64() == Some(id.into()))
        };
        let validator: Option<&dyn Fn(u32) -> bool> = if c["valid"].is_null() {
            None
        } else {
            Some(&validate)
        };
        assert_eq!(
            resolve_split_scatter_range_hint(&region, validator),
            expected(&c["expected"]),
            "{}",
            c["name"]
        );
    }
    println!(
        "task_052: {} grouping cases",
        data["cases"].as_array().unwrap().len()
    );
}
#[test]
fn task_052_prefix_boundaries() {
    let data = fixture();
    for c in data["prefixes"].as_array().unwrap() {
        let raw = bytes(c["raw"].as_str().unwrap());
        let before = raw.clone();
        assert_eq!(
            split_scatter_next_prefix(&raw),
            bytes(c["next"].as_str().unwrap())
        );
        assert_eq!(
            split_scatter_prefix_range_with_keyspace_group(
                &bytes(c["keyspace"].as_str().unwrap()),
                &raw,
                "custom"
            ),
            expected(&c["expected"])
        );
        assert_eq!(raw, before);
    }
    println!(
        "task_052: {} prefix cases",
        data["prefixes"].as_array().unwrap().len()
    );
}
