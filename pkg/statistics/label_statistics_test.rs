// Copyright 2026 AsterSQL.
use crate::{LabelStatistics, get_region_label_isolation, is_region_label_isolation_satisfied};
use astersql_pd_core::{RegionInfo, RegionMeta, StoreInfo, StoreLabel, StoreMeta};
use std::collections::BTreeMap;
const LABELS: [&str; 3] = ["zone", "rack", "host"];
fn stores(paths: &[[&str; 3]]) -> Vec<StoreInfo> {
    paths
        .iter()
        .enumerate()
        .map(|(i, path)| {
            StoreInfo::new(StoreMeta {
                id: i as u64 + 1,
                labels: LABELS
                    .iter()
                    .zip(path)
                    .filter(|(_, v)| !v.is_empty())
                    .map(|(k, v)| StoreLabel {
                        key: k.to_string(),
                        value: v.to_string(),
                    })
                    .collect(),
                ..Default::default()
            })
        })
        .collect()
}
fn region(id: u64) -> RegionInfo {
    RegionInfo::new(
        RegionMeta {
            id,
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    )
}
fn counters(values: &[(&str, i64)]) -> BTreeMap<String, i64> {
    values.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}
fn check(stores: &[StoreInfo], labels: &[&str], expected: (&str, isize)) {
    let refs: Vec<_> = stores.iter().collect();
    assert_eq!(get_region_label_isolation(&refs, labels), expected);
    for (i, label) in labels.iter().enumerate() {
        assert_eq!(
            is_region_label_isolation_satisfied(&refs, labels, label),
            expected.1 >= 0 && expected.1 <= i as isize
        );
    }
    assert!(is_region_label_isolation_satisfied(&refs, labels, ""));
    assert!(is_region_label_isolation_satisfied(&refs, labels, "none"));
    assert!(!is_region_label_isolation_satisfied(
        &refs, labels, "unknown"
    ));
}
#[test]
fn state_task_17_label_statistics_go_levels_and_reobservation() {
    let paths = [
        [["z1", "r1", "h1"], ["z2", "r1", "h2"], ["z2", "r2", "h3"]],
        [["z1", "r1", "h1"], ["z2", "r2", "h2"], ["z2", "r2", "h3"]],
        [["z1", "r1", "h1"], ["z2", "r2", "h2"], ["z3", "r2", "h3"]],
        [["z1", "r1", "h1"], ["z1", "r2", "h2"], ["z1", "r3", "h3"]],
        [["z1", "r1", "h1"], ["z1", "r2", "h2"], ["z1", "r2", "h2"]],
        [["", "r1", "h1"], ["", "r2", "h2"], ["", "r3", "h3"]],
        [["z1", "r1", "h1"], ["z1", "r2", "h2"], ["z1", "", "h3"]],
    ];
    let all = [
        ("rack", 1),
        ("host", 2),
        ("zone", 0),
        ("rack", 1),
        ("none", -1),
        ("rack", 1),
        ("host", 2),
    ];
    let short = [
        ("rack", 1),
        ("none", -1),
        ("zone", 0),
        ("rack", 1),
        ("none", -1),
        ("rack", 1),
        ("none", -1),
    ];
    let mut stats = LabelStatistics::new();
    for (labels, expected, snapshot) in [
        (
            &LABELS[..],
            all,
            counters(&[("none", 1), ("host", 2), ("rack", 3), ("zone", 1)]),
        ),
        (
            &LABELS[..2],
            short,
            counters(&[("none", 3), ("host", 0), ("rack", 3), ("zone", 1)]),
        ),
    ] {
        for (i, paths) in paths.iter().enumerate() {
            let stores = stores(paths);
            check(&stores, labels, expected[i]);
            let refs: Vec<_> = stores.iter().collect();
            stats.observe(&region(i as u64 + 1), &refs, labels);
            let before = stats.label_counter();
            stats.observe(&region(i as u64 + 1), &refs, labels);
            assert_eq!(stats.label_counter(), before);
        }
        assert_eq!(stats.label_counter(), snapshot);
    }
}
#[test]
fn state_task_17_label_statistics_go_case_insensitive() {
    let cases = [
        (
            [["z1", "r1", "İ"], ["z1", "r1", "i"], ["z1", "r1", "b"]],
            ("host", 2),
        ),
        (
            [["z1", "r1", "ı"], ["z1", "r1", "i"], ["z1", "r1", "b"]],
            ("host", 2),
        ),
        (
            [["z1", "r1", "ς"], ["z1", "r1", "σ"], ["z1", "r1", "b"]],
            ("none", -1),
        ),
        (
            [["z1", "r1", "ſ"], ["z1", "r1", "s"], ["z1", "r1", "b"]],
            ("none", -1),
        ),
        (
            [["z1", "r1", "K8S"], ["z1", "r1", "K8s"], ["z1", "r1", "b"]],
            ("none", -1),
        ),
        (
            [["z1", "r1", "h-s"], ["z1", "r1", "H-ſ"], ["z1", "r1", "b"]],
            ("none", -1),
        ),
        (
            [["z1", "r1", "ß"], ["z1", "r1", "ss"], ["z1", "r1", "b"]],
            ("host", 2),
        ),
        (
            [["z1", "r1", "A"], ["z1", "r1", "a"], ["z1", "r1", "B"]],
            ("none", -1),
        ),
        (
            [["z1", "R1", "h1"], ["z1", "r1", "h2"], ["z1", "r2", "h3"]],
            ("host", 2),
        ),
        (
            [["Z1", "r1", "h1"], ["z1", "r2", "h1"], ["z2", "r1", "h1"]],
            ("rack", 1),
        ),
        (
            [["Z1", "r1", "h1"], ["z1", "R1", ""], ["z2", "r1", "h3"]],
            ("none", -1),
        ),
    ];
    for (paths, expected) in cases {
        let stores = stores(&paths);
        check(&stores, &LABELS, expected);
        let mut stats = LabelStatistics::new();
        stats.observe(&region(1), &stores.iter().collect::<Vec<_>>(), &LABELS);
        assert_eq!(stats.label_counter(), counters(&[(expected.0, 1)]));
    }
}
#[test]
fn state_task_17_label_statistics_empty_missing_and_keys() {
    check(&[], &LABELS, ("none", -1));
    check(&stores(&[["a", "b", "c"]]), &[], ("none", -1));
    check(&stores(&[["", "", ""]]), &LABELS, ("zone", 0));
    check(
        &stores(&[["", "", ""], ["", "", ""]]),
        &LABELS,
        ("none", -1),
    );
    let mut stores = stores(&[["z1", "r1", "h1"], ["z2", "r2", "h2"]]);
    stores[0].set_labels(vec![StoreLabel {
        key: "ZONE".into(),
        value: "Z1".into(),
    }]);
    check(&stores, &LABELS, ("zone", 0));
    assert!(!is_region_label_isolation_satisfied(
        &stores.iter().collect::<Vec<_>>(),
        &LABELS,
        "ZONE"
    ));
}
#[test]
fn state_task_17_label_statistics_defunct_lifecycle() {
    let a = stores(&[["z1", "r1", "h1"], ["z2", "r1", "h2"]]);
    let b = stores(&[["z1", "r1", "h1"], ["z1", "r2", "h2"]]);
    let a: Vec<_> = a.iter().collect();
    let b: Vec<_> = b.iter().collect();
    let mut stats = LabelStatistics::new();
    stats.observe(&region(1), &a, &LABELS);
    stats.observe(&region(2), &a, &LABELS);
    stats.mark_defunct_region(1);
    stats.mark_defunct_region(1);
    stats.mark_defunct_region(99);
    assert_eq!(stats.label_counter(), counters(&[("zone", 2)]));
    stats.observe(&region(1), &b, &LABELS);
    assert_eq!(stats.label_counter(), counters(&[("zone", 1), ("rack", 1)]));
    stats.clear_defunct_regions();
    assert_eq!(stats.label_counter(), counters(&[("zone", 1), ("rack", 0)]));
    stats.clear_defunct_regions();
    assert_eq!(stats.label_counter(), counters(&[("zone", 1), ("rack", 0)]));
    // Re-observing a cleared ID creates a fresh entry. Returned maps are independent.
    stats.observe(&region(1), &b, &LABELS);
    let mut snapshot = stats.label_counter();
    snapshot.clear();
    stats.mark_defunct_region(1);
    stats.mark_defunct_region(2);
    stats.clear_defunct_regions();
    assert_eq!(stats.label_counter(), counters(&[("zone", 0), ("rack", 0)]));
    // Unknown marks are drained: a later observation must survive the next clear.
    stats.observe(&region(99), &a, &LABELS);
    stats.clear_defunct_regions();
    assert_eq!(stats.label_counter(), counters(&[("zone", 1), ("rack", 0)]));
}
