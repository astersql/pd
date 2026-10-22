// Copyright 2026 AsterSQL.
use crate::{StoreInfo, StoreLabel, StoreMeta, distinct_score};
use serde::Deserialize;
use std::collections::BTreeMap;
#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    left: BTreeMap<String, String>,
    right: BTreeMap<String, String>,
    index: Option<usize>,
    score: f64,
}
fn store(id: u64, labels: BTreeMap<String, String>) -> StoreInfo {
    StoreInfo::new(StoreMeta {
        id,
        labels: labels
            .into_iter()
            .map(|(key, value)| StoreLabel { key, value })
            .collect(),
        ..Default::default()
    })
}
#[test]
fn core_task_24_location_fixture() {
    let fixture: Fixture =
        serde_json::from_str(include_str!("rust-fixtures/core_task_24.json")).unwrap();
    for c in fixture.cases {
        let a = store(1, c.left);
        let b = store(2, c.right);
        assert_eq!(
            a.compare_location(&b, &["zone", "rack", "host"]),
            c.index,
            "{}",
            c.name
        );
        assert_eq!(
            b.compare_location(&a, &["zone", "rack", "host"]),
            c.index,
            "{}",
            c.name
        );
        assert_eq!(
            distinct_score(&["zone", "rack", "host"], &[&a], &b),
            c.score,
            "{}",
            c.name
        );
    }
}
#[test]
fn core_task_24_empty_and_same_id() {
    let a = store(1, BTreeMap::from([("zone".into(), "a".into())]));
    let b = store(1, BTreeMap::from([("zone".into(), "b".into())]));
    assert_eq!(a.compare_location(&b, &[]), None);
    assert_eq!(distinct_score(&["zone"], &[], &b), 0.0);
    assert_eq!(distinct_score(&["zone"], &[&a], &b), 0.0);
    assert_eq!(distinct_score(&[], &[&a], &b), 0.0);
}
#[test]
fn core_task_24_go_grid() {
    let mut stores = Vec::new();
    for i in 0..3 {
        for j in 0..3 {
            for k in 0..3 {
                let s = store(
                    (i * 9 + j * 3 + k) as u64,
                    BTreeMap::from([
                        ("zone".into(), format!("z{i}")),
                        ("rack".into(), format!("r{j}")),
                        ("host".into(), format!("h{k}")),
                    ]),
                );
                stores.push(s);
                let refs: Vec<_> = stores.iter().collect();
                assert_eq!(
                    distinct_score(&["zone", "rack", "host"], &refs, stores.last().unwrap()),
                    ((i * 9 * 100 + j * 3) * 100 + k) as f64
                );
            }
        }
    }
    assert_eq!(
        distinct_score(
            &["zone", "rack", "host"],
            &stores.iter().collect::<Vec<_>>(),
            &store(100, BTreeMap::new())
        ),
        0.0
    );
}
