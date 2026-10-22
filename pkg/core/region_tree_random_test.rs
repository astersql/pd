// Copyright 2026 AsterSQL.
use crate::*;
use std::sync::Arc;
fn tree() -> RegionTree {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_43.json")).unwrap();
    RegionTree::from_regions(f["regions"].as_array().unwrap().iter().map(|r| {
        Arc::new(RegionInfo::new(
            RegionMeta {
                id: r[0].as_u64().unwrap(),
                key_range: range(r[1].as_str().unwrap(), r[2].as_str().unwrap()),
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        ))
    }))
}
fn range(a: &str, b: &str) -> KeyRange {
    KeyRange {
        start_key: a.as_bytes().to_vec(),
        end_key: b.as_bytes().to_vec(),
    }
}
fn sample(t: &RegionTree, n: usize, r: &[KeyRange], index: usize) -> Vec<u64> {
    t.random_regions(n, r, |bound| index % bound, |_| {})
        .iter()
        .map(|r| r.meta().id)
        .collect()
}
#[test]
fn core_task_43_ranges_and_candidates() {
    let t = tree();
    let f: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_43.json")).unwrap();
    for c in f["cases"].as_array().unwrap() {
        let r = range(c[0].as_str().unwrap(), c[1].as_str().unwrap());
        let expected: Vec<u64> = serde_json::from_value(c[2].clone()).unwrap();
        let mut seen = vec![];
        for i in 0..4 {
            for id in sample(&t, 1, std::slice::from_ref(&r), i) {
                assert!(expected.contains(&id));
                if !seen.contains(&id) {
                    seen.push(id);
                }
            }
        }
        seen.sort();
        assert_eq!(seen, expected, "{c}");
    }
}
#[test]
fn core_task_43_empty_duplicate_and_identity() {
    let t = tree();
    assert!(sample(&RegionTree::new(), 1, &[], 0).is_empty());
    assert!(sample(&t, 0, &[], 0).is_empty());
    assert_eq!(sample(&t, 3, &[], 0), vec![1; 3]);
    let r = t.random_region(&[], |_| 0, |_| {}).unwrap();
    assert!(Arc::ptr_eq(&r, &t.search(b"c").unwrap()));
}
#[test]
fn core_task_43_retry_boundaries() {
    let t = tree();
    let mut draws = 0;
    assert!(
        t.random_regions(
            3,
            &[range("d", "y")],
            |_| {
                draws += 1;
                0
            },
            |_| {}
        )
        .is_empty()
    );
    assert_eq!(draws, 1);
    let mut seq = [1, 0, 1, 1].into_iter();
    assert_eq!(
        t.random_regions(3, &[range("d", "y")], |_| seq.next().unwrap(), |_| {})
            .len(),
        3
    );
    let ranges = [range("d", "e"), range("g", "y")];
    assert_eq!(sample(&t, 3, &ranges, 0), vec![2; 3]);
    assert!(sample(&t, 3, &[range("d", "e"), range("x", "y")], 0).is_empty());
    let mut order = vec![];
    let ids = t
        .random_regions(
            2,
            &[range("c", "f"), range("n", "x")],
            |_| 0,
            |p| {
                p.reverse();
                order = p.to_vec();
            },
        )
        .iter()
        .map(|r| r.meta().id)
        .collect::<Vec<_>>();
    assert_eq!(order, vec![1, 0]);
    assert_eq!(ids, vec![2, 1]);
}

#[test]
fn core_task_43_continuous_ranges() {
    let t = RegionTree::from_regions(
        [("", "g"), ("g", "n"), ("n", "t"), ("t", "")]
            .into_iter()
            .enumerate()
            .map(|(i, (a, b))| {
                Arc::new(RegionInfo::new(
                    RegionMeta {
                        id: i as u64 + 1,
                        key_range: range(a, b),
                        ..Default::default()
                    },
                    None,
                    vec![],
                    vec![],
                ))
            }),
    );
    for (a, b, expected) in [
        ("g", "n", vec![2]),
        ("n", "t", vec![3]),
        ("t", "", vec![4]),
        ("h", "s", vec![]),
        ("a", "z", vec![2, 3]),
    ] {
        let r = range(a, b);
        let mut seen = vec![];
        for i in 0..4 {
            for id in sample(&t, 1, std::slice::from_ref(&r), i) {
                assert!(expected.contains(&id));
                if !seen.contains(&id) {
                    seen.push(id);
                }
            }
        }
        seen.sort();
        assert_eq!(seen, expected);
    }
}
#[test]
#[should_panic(expected = "random index exceeds bound")]
fn core_task_43_invalid_random_index() {
    tree().random_regions(1, &[], |n| n, |_| {});
}
