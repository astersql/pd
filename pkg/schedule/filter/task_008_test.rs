// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{StoreInfo, StoreLabel, StoreMeta, StoreStats};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_placement::LabelConstraint;
use serde_json::Value;
use std::sync::Arc;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_008.json"
    ))
    .unwrap()
}
fn labels(v: &Value) -> Vec<StoreLabel> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|l| StoreLabel {
            key: l["key"].as_str().unwrap().into(),
            value: l["value"].as_str().unwrap().into(),
        })
        .collect()
}
fn store(v: &Value) -> StoreInfo {
    let mut s = StoreInfo::new(StoreMeta {
        id: 1,
        labels: labels(&v["labels"]),
        ..Default::default()
    });
    s.update_raw_stats(StoreStats {
        capacity: v["capacity"].as_u64().unwrap_or(0),
        available: v["available"].as_u64().unwrap_or(0),
        ..Default::default()
    });
    s.set_region_count(v["regions"].as_i64().unwrap_or(0));
    let bits = v["evicted"].as_u64().unwrap_or(0);
    if bits & 1 != 0 {
        s.slow_store_evicted();
    }
    if bits & 2 != 0 {
        s.stopping_store_evicted();
    }
    if bits & 4 != 0 {
        s.slow_trend_evicted();
    }
    s
}
fn filter(case: &Value) -> (Box<dyn Filter<()>>, i64) {
    match case["kind"].as_str().unwrap() {
        "storage" => (Box::new(StorageThresholdFilter::new("task008")), 1),
        "engine" => (
            Box::new(EngineFilter::new(
                "task008",
                if case["special"] == true {
                    special_engines()
                } else {
                    not_special_engines()
                },
            )),
            6,
        ),
        "hot" => (Box::new(HotRegionEvictedTargetFilter::new("task008")), 9),
        "special_use" => {
            let allow: Vec<String> = serde_json::from_value(case["allow"].clone()).unwrap();
            (Box::new(SpecialUseFilter::new("task008", &allow)), 7)
        }
        "label" => {
            let cs: Vec<LabelConstraint> =
                serde_json::from_value(case["constraints"].clone()).unwrap();
            (Box::new(LabelConstraintFilter::new("task008", cs)), 3)
        }
        "isolation" => {
            let locations: Vec<String> = serde_json::from_value(case["locations"].clone()).unwrap();
            let stores: Vec<StoreInfo> = case["region_labels"]
                .as_array()
                .unwrap()
                .iter()
                .map(|ls| {
                    StoreInfo::new(StoreMeta {
                        labels: labels(ls),
                        ..Default::default()
                    })
                })
                .collect();
            (
                Box::new(IsolationFilter::new(
                    "task008",
                    case["level"].as_str().unwrap(),
                    locations,
                    &stores.iter().collect::<Vec<_>>(),
                )),
                8,
            )
        }
        other => panic!("unknown fixture kind {other}"),
    }
}
#[test]
fn task_008_behavior_cases() {
    let f = fixture();
    let mut conf = ProviderSnapshot::<()>::default();
    conf.schedule.low_space_ratio = 0.8;
    assert_eq!(
        conf.schedule.low_space_ratio,
        f["low_space_ratio"].as_f64().unwrap()
    );
    let cases = f["cases"].as_array().unwrap();
    assert!(cases.len() >= 115);
    for case in cases {
        let s = store(&case["store"]);
        let (filter, typ) = filter(case);
        assert_eq!(filter.scope(), "task008");
        assert_eq!(filter.filter_type(), FilterType(typ));
        for (i, status) in [filter.source(&conf, &s), filter.target(&conf, &s)]
            .iter()
            .enumerate()
        {
            assert_eq!(
                status.status_code.0,
                case["expected"][i]["status"].as_i64().unwrap(),
                "{} direction={i}",
                case["id"]
            );
            assert_eq!(
                status.detailed_reason,
                case["expected"][i]["detail"].as_str().unwrap(),
                "{} direction={i}",
                case["id"]
            );
        }
    }
    println!(
        "task_008 oracle cases={} source/target results={} passed",
        cases.len(),
        cases.len() * 2
    );
}

#[test]
fn task_008_chain_and_constructor_boundaries() {
    let mut conf = ProviderSnapshot::<()>::default();
    conf.schedule.low_space_ratio = 0.8;
    let f = fixture();
    let low = f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"] == "storage" && c["expected"][1]["status"] != 0)
        .unwrap();
    let s = Arc::new(store(&low["store"]));
    let capacity = StorageThresholdFilter::new("chain");
    let excluded = ExcludedFilter::new("chain", [], [1]);
    let stores = [s.clone(), s.clone()];
    for filters in [
        &[&capacity as &dyn Filter<()>, &excluded][..],
        &[&excluded as &dyn Filter<()>, &capacity][..],
    ] {
        let rejection = first_rejection(&s, filters, &conf, false).unwrap();
        assert_eq!(rejection.0, 0);
        assert_eq!(rejection.1, filters[0].target(&conf, &s));
        assert!(select_target_stores(&stores, filters, &conf, None).is_empty());
        assert_eq!(select_source_stores(&stores, filters, &conf, None).len(), 2);
        let rejected = select_unavailable_target_stores(&stores, filters, &conf, None);
        assert_eq!(rejected.len(), 2);
        assert!(Arc::ptr_eq(&rejected[0], &s));
    }
    // Go constructor defaults an unrecognized/empty isolation level to the first key.
    let region = store(&serde_json::json!({"labels":[{"key":"zone","value":"z1"}]}));
    let locations = vec!["zone".into(), "rack".into()];
    for level in ["unknown", "", "ZONE"] {
        let iso = IsolationFilter::new("scope", level, locations.clone(), &[&region]);
        assert!(!iso.target(&conf, &region).is_ok());
        assert!(iso.source(&conf, &region).is_ok());
    }
    let iso = IsolationFilter::new("scope", "", vec![], &[]);
    assert!(!iso.target(&conf, &region).is_ok());
    let special = SpecialUseFilter::new(
        "scope",
        &["reserved".into(), "hotRegion".into(), "unknown".into()],
    );
    for value in ["reserved", "hotRegion", "unknown", ""] {
        let s = store(&serde_json::json!({"labels":[{"key":"specialUse","value":value}]}));
        assert!(special.target(&conf, &s).is_ok());
    }
}

#[test]
#[should_panic(expected = "index out of bounds")]
fn task_008_invalid_isolation_locations_fault() {
    // Go also indexes the first key when region stores exist but locations are empty.
    let s = StoreInfo::new(StoreMeta::default());
    let _ = IsolationFilter::new("scope", "", vec![], &[&s]);
}
