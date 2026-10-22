// Copyright 2026 AsterSQL.
use crate::*;
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    size: i64,
    keys: i64,
    size_limit: i64,
    keys_limit: i64,
    empty: bool,
    merge: bool,
    oversized: bool,
    source: String,
}
#[test]
fn core_task_32_thresholds_and_empty_sentinels() {
    let fixture: Fixture =
        serde_json::from_str(include_str!("rust-fixtures/core_task_32.json")).unwrap();
    assert_eq!(fixture.cases.len(), 17);
    for c in fixture.cases {
        assert!(!c.source.is_empty());
        let mut region = RegionInfo::new(RegionMeta::default(), None, vec![], vec![]);
        region.stats.approximate_size = c.size;
        region.stats.approximate_keys = c.keys;
        assert_eq!(
            is_empty_region(c.size, c.keys),
            c.empty,
            "empty: {}",
            c.name
        );
        assert_eq!(
            region.need_merge(c.size_limit, c.keys_limit),
            c.merge,
            "merge: {}",
            c.name
        );
        assert_eq!(
            region.is_oversized(c.size_limit, c.keys_limit),
            c.oversized,
            "oversized: {}",
            c.name
        );
    }
}
