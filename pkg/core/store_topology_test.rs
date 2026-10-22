// Copyright 2026 AsterSQL.
use crate::{StoreInfo, StoreLabel, StoreMeta, StoreState, get_store_topo_weight};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    labels: Vec<String>,
    stores: Vec<Vec<[String; 2]>>,
    count: i64,
    expected: f64,
    source: String,
    #[serde(default)]
    removed: bool,
    target_labels: Option<Vec<[String; 2]>>,
}
fn store(id: u64, labels: &[[String; 2]], removed: bool) -> StoreInfo {
    let mut store = StoreInfo::new(StoreMeta {
        id,
        labels: labels
            .iter()
            .map(|l| StoreLabel {
                key: l[0].clone(),
                value: l[1].clone(),
            })
            .collect(),
        ..Default::default()
    });
    if removed {
        store.set_store_state(StoreState::Tombstone, None);
    }
    store
}
#[test]
fn core_task_25_topology_fixtures() {
    let fixture: Fixture =
        serde_json::from_str(include_str!("rust-fixtures/core_task_25.json")).unwrap();
    assert_eq!(fixture.cases.len(), 9);
    for case in fixture.cases {
        let stores: Vec<_> = case
            .stores
            .iter()
            .enumerate()
            .map(|(i, l)| store(i as u64 + 1, l, case.removed))
            .collect();
        let target = case.target_labels.as_ref().map(|l| store(99, l, false));
        let target = target.as_ref().unwrap_or(&stores[0]);
        let labels: Vec<_> = case.labels.iter().map(String::as_str).collect();
        let refs: Vec<_> = stores.iter().collect();
        let before = target.labels().to_vec();
        let actual = get_store_topo_weight(target, &refs, &labels, case.count);
        assert!(
            (actual - case.expected).abs() < 1e-14,
            "{}: {} != {}; {}",
            case.name,
            actual,
            case.expected,
            case.source
        );
        assert_eq!(target.labels(), before);
    }
}
