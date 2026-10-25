// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{NodeState, StoreInfo, StoreMeta};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_plan::{BalanceSchedulerPlan, Collector, StatusCode};
use serde_json::{Value, json};
use std::sync::Arc;

fn store(id: u64) -> Arc<StoreInfo> {
    Arc::new(StoreInfo::new(StoreMeta {
        id,
        ..Default::default()
    }))
}
fn ids(value: &Value) -> Vec<u64> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect()
}

#[test]
fn task_005_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_005.json"
    ))
    .unwrap();
    let conf = ProviderSnapshot::<u64>::default();
    let mut count = 0;
    for case in fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["rust_parity"] == true)
    {
        let name = case["name"].as_str().unwrap();
        let stores: Vec<_> = ids(&case["input_ids"])
            .into_iter()
            .map(|id| {
                if case["removed"] == true {
                    Arc::new(StoreInfo::new(StoreMeta {
                        id,
                        node_state: NodeState::Removed,
                        ..Default::default()
                    }))
                } else {
                    store(id)
                }
            })
            .collect();
        let excluded = ExcludedFilter::new(
            "task005",
            ids(&case["source_exclusions"]),
            ids(&case["target_exclusions"]),
        );
        // Both filters are real exclusion filters. The later one also rejects;
        // the internal rejection index separately proves short circuit ordering.
        let filters: Vec<&dyn Filter<u64>> =
            vec![&excluded; case["filters"].as_u64().unwrap() as usize];
        let base = BalanceSchedulerPlan {
            step: case["base_step"].as_i64().unwrap(),
            ..Default::default()
        };
        let mut collector = Collector::new(base);
        let selected = match case["mode"].as_str().unwrap() {
            "Source" => select_source_stores(&stores, &filters, &conf, Some(&mut collector)),
            "Target" => select_target_stores(&stores, &filters, &conf, Some(&mut collector)),
            "UnavailableTarget" => {
                select_unavailable_target_stores(&stores, &filters, &conf, Some(&mut collector))
            }
            _ => panic!("unknown fixture mode"),
        };
        assert_eq!(
            selected.iter().map(|s| s.id()).collect::<Vec<_>>(),
            ids(&case["selected"]),
            "{name}"
        );
        for s in &selected {
            assert!(
                stores.iter().any(|original| Arc::ptr_eq(original, s)),
                "{name}"
            );
        }
        let plans: Vec<_> = collector.get_plans().iter().map(|p| json!({
            "step":p.step, "resource": if p.step == 2 {p.target.as_ref().unwrap().id()} else {p.source.as_ref().unwrap().id()},
            "status":p.status.status_code.0,"reason":p.status.detailed_reason,
        })).collect();
        assert_eq!(json!(plans), case["plans"], "{name}");
        assert_eq!(
            collector.base_plan.step,
            case["base_step"].as_i64().unwrap()
        );
        count += 1;
    }
    assert_eq!(count, 8);
    println!("task_005 oracle cases: {count} passed; 0 failed");
}

#[test]
fn task_005_order_and_boundaries() {
    let conf = ProviderSnapshot::<u64>::default();
    let s = store(u64::MAX);
    let first = ExcludedFilter::new("first", [u64::MAX], [u64::MAX]);
    let later = ExcludedFilter::new("later", [u64::MAX], [u64::MAX]);
    let filters: [&dyn Filter<u64>; 2] = [&first, &later];
    for source in [true, false] {
        let (index, status) =
            super::selection::first_rejection(&s, &filters, &conf, source).unwrap();
        assert_eq!(index, 0);
        assert_eq!(filters[index].scope(), "first");
        assert_eq!(status.status_code, StatusCode::STORE_ALREADY_HAS_PEER);
        assert!(status.detailed_reason.is_empty());
    }
    assert_eq!(
        <ExcludedFilter as Filter<u64>>::filter_type(&first),
        FilterType::EXCLUDED
    );
    let stores = vec![s.clone(), store(0), s.clone()];
    assert_eq!(
        select_unavailable_target_stores(&stores, &filters, &conf, None)
            .iter()
            .map(|s| s.id())
            .collect::<Vec<_>>(),
        [u64::MAX, u64::MAX]
    );
    assert_eq!(
        select_source_stores(&stores, &[], &conf, None)
            .iter()
            .map(|s| s.id())
            .collect::<Vec<_>>(),
        [u64::MAX, 0, u64::MAX]
    );
    assert!(select_target_stores(&[], &filters, &conf, None).is_empty());
    let pass = ExcludedFilter::new("pass", [], []);
    let chain: [&dyn Filter<u64>; 2] = [&pass, &first];
    assert_eq!(
        super::selection::first_rejection(&s, &chain, &conf, false)
            .unwrap()
            .0,
        1
    );
}

#[test]
fn task_005_collector_resource_fault() {
    let conf = ProviderSnapshot::<u64>::default();
    let stores = vec![store(2)];
    let excluded = ExcludedFilter::new("fault", [], [2]);
    let filters: [&dyn Filter<u64>; 1] = [&excluded];
    let mut collector = Collector::new(BalanceSchedulerPlan {
        step: 1,
        ..Default::default()
    });
    let fault = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        select_target_stores(&stores, &filters, &conf, Some(&mut collector))
    }));
    let payload = fault.expect_err("Go rejects a Store resource at region step 1");
    assert_eq!(
        payload.downcast_ref::<&str>().copied(),
        Some("resource type does not match balance step")
    );
    assert!(collector.get_plans().is_empty());
    assert_eq!(collector.base_plan.step, 1);
    // Explicit unavailable-target selection forces the correct step even when
    // the base was at region selection; the same real collector remains usable.
    let selected = select_unavailable_target_stores(&stores, &filters, &conf, Some(&mut collector));
    assert_eq!(selected[0].id(), 2);
    let plans = collector.get_plans();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].step, 2);
    assert_eq!(plans[0].target.as_ref().unwrap().id(), 2);
}
