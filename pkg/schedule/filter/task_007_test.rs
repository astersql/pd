// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{StoreInfo, StoreMeta, StoreStats};
use astersql_pd_core_constant::{Direction, PriorityLevel};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_plan::StatusCode;
use astersql_pd_storelimit::{StoreRateLimit, Type};
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_007.json"
    ))
    .unwrap()
}
fn config(f: &Value) -> ProviderSnapshot<()> {
    let mut conf = ProviderSnapshot::default();
    // Deserialize through the real configuration duration contract.
    conf.schedule = serde_json::from_value(serde_json::json!({
        "max-store-down-time": "30m", "max-snapshot-count": f["max_snapshot"],
        "max-pending-peer-count": f["max_pending"]
    }))
    .unwrap();
    conf
}
async fn store(v: &Value) -> StoreInfo {
    let mut s = StoreInfo::new(StoreMeta {
        id: 1,
        last_heartbeat: v["heartbeat"].as_i64().unwrap(),
        ..Default::default()
    });
    s.edit_meta(|m| {
        m.node_state = match v["node"].as_i64().unwrap() {
            0 => astersql_pd_core::NodeState::Preparing,
            2 => astersql_pd_core::NodeState::Removing,
            3 => astersql_pd_core::NodeState::Removed,
            n => astersql_pd_core::NodeState::Unknown(n as i32),
        }
    });
    s.update_raw_stats(StoreStats {
        is_busy: v["busy"].as_bool().unwrap(),
        sending_snap_count: v["sending"].as_u64().unwrap() as u32,
        receiving_snap_count: v["receiving"].as_u64().unwrap() as u32,
        applying_snap_count: v["applying"].as_u64().unwrap() as u32,
        slow_score: v["slow_score"].as_u64().unwrap(),
        ..Default::default()
    });
    s.set_pending_peer_count(v["pending"].as_i64().unwrap());
    s.set_network_slow_triggers(v["network"].as_u64().unwrap());
    if v["pause_in"] == 1 {
        s.pause_leader_transfer(Direction::IN);
    }
    if v["pause_out"] == 1 {
        s.pause_leader_transfer(Direction::OUT);
    }
    if v["slow"] == 1 {
        s.slow_store_evicted();
    }
    if v["stopping"] == 1 {
        s.stopping_store_evicted();
    }
    if v["trend"] == 1 {
        s.slow_trend_evicted();
    }
    if v["limit"] == true {
        let limit = StoreRateLimit::new(0.000000001);
        for typ in [Type::ADD_PEER, Type::REMOVE_PEER, Type::TRANSFER_LEADER_IN] {
            assert!(limit.take(1000, typ, PriorityLevel::from_i32(0)).await);
        }
        s.set_store_limit(Some(astersql_pd_core::StoreLimiter::V1(limit)));
    }
    s
}
#[tokio::test]
async fn task_007_behavior_cases() {
    let f = fixture();
    let mut conf = config(&f);
    let cases = f["cases"].as_array().unwrap();
    assert!(cases.len() >= 234);
    for case in cases {
        let mut s = store(&case["store"]).await;
        conf.label_properties.clear();
        if case["reject_property"] == true {
            let label = astersql_pd_core::StoreLabel {
                key: "noleader".into(),
                value: "true".into(),
            };
            s.set_labels(vec![label.clone()]);
            conf.label_properties
                .insert("reject-leader".into(), vec![label]);
        }
        let v = &case["filter"];
        let filter = StoreStateFilter {
            transfer_leader: v["leader"].as_bool().unwrap(),
            move_region: v["region"].as_bool().unwrap(),
            allow_temporary_states: v["temporary"].as_bool().unwrap(),
            scatter_region: v["scatter"].as_bool().unwrap(),
            allow_fast_failover: v["fast"].as_bool().unwrap(),
            operator_level: PriorityLevel::from_i32(v["level"].as_i64().unwrap() as i32),
            now_unix_nanos: f["now_unix_nanos"].as_i64().unwrap() as i128,
            ..Default::default()
        };
        for (i, source) in [true, false].into_iter().enumerate() {
            let actual = if source {
                filter.source(&conf, &s).await
            } else {
                filter.target(&conf, &s).await
            };
            let expected = &case["expected"][i];
            assert_eq!(
                actual.status_code,
                StatusCode(expected["status"].as_i64().unwrap()),
                "{} source={source}",
                case["id"]
            );
            assert_eq!(actual.detailed_reason, expected["detail"].as_str().unwrap());
            assert_eq!(
                filter.reason().0,
                expected["reason"].as_i64().unwrap(),
                "{} source={source}",
                case["id"]
            );
        }
    }
    println!(
        "Go differential: {} cases, {} source/target results",
        cases.len(),
        cases.len() * 2
    );
}

#[tokio::test]
async fn task_007_clock_budget_and_boundaries() {
    let f = fixture();
    let mut conf = config(&f);
    let mut s = StoreInfo::new(StoreMeta {
        id: 1,
        last_heartbeat: 100,
        ..Default::default()
    });
    let mut filter = StoreStateFilter {
        transfer_leader: true,
        now_unix_nanos: 100,
        action_scope: "safety".into(),
        ..Default::default()
    };
    assert_eq!(filter.scope(), "safety");
    // Exact explicit-clock boundaries supplement the frozen Go elapsed classes.
    for (elapsed, code) in [
        (20_000_000_000, StatusCode::OK),
        (20_000_000_001, StatusCode::STORE_DISCONNECTED),
        (1_800_000_000_000, StatusCode::STORE_DISCONNECTED),
        (1_800_000_000_001, StatusCode::STORE_DOWN),
        (-1, StatusCode::OK),
    ] {
        filter.now_unix_nanos = 100 + elapsed;
        assert_eq!(filter.source(&conf, &s).await.status_code, code);
    }
    filter.now_unix_nanos = 100;
    filter.move_region = true;
    s.set_pending_peer_count(100);
    conf.schedule.max_pending_peer_count = 0;
    assert!(filter.target(&conf, &s).await.is_ok());
    conf.schedule.max_pending_peer_count = u64::MAX;
    assert_eq!(
        filter.target(&conf, &s).await.status_code,
        StatusCode::STORE_PENDING_PEER_THROTTLED
    );
    conf.schedule.max_pending_peer_count = 64;
    s.set_pending_peer_count(0);
    let limit = StoreRateLimit::new(0.000000001);
    s.set_store_limit(Some(astersql_pd_core::StoreLimiter::V1(limit)));
    for _ in 0..2 {
        assert!(filter.target(&conf, &s).await.is_ok());
    }
    // Selection must not reserve budget; the first real take still succeeds.
    assert!(
        s.limiter()
            .unwrap()
            .write()
            .unwrap()
            .take(1000, Type::TRANSFER_LEADER_IN, PriorityLevel::LOW)
            .await
    );
    for level in [
        PriorityLevel::LOW,
        PriorityLevel::MEDIUM,
        PriorityLevel::HIGH,
        PriorityLevel::URGENT,
    ] {
        filter.operator_level = level;
        assert_eq!(
            filter.target(&conf, &s).await.status_code,
            StatusCode::STORE_TRANSFER_LEADER_IN_LIMIT_THROTTLED
        );
    }
    filter.allow_temporary_states = true;
    assert!(filter.target(&conf, &s).await.is_ok());
    s.pause_leader_transfer(Direction::IN);
    assert_eq!(
        filter.target(&conf, &s).await.status_code,
        StatusCode::STORE_REJECT_LEADER
    );
    assert_eq!(filter.reason(), FilterType(14));
    filter.transfer_leader = false;
    filter.move_region = false;
    assert!(filter.target(&conf, &s).await.is_ok());
    assert_eq!(filter.filter_type(), FilterType(14));
    filter.transfer_leader = true;
    filter.allow_temporary_states = false;
    s.resume_leader_transfer(Direction::IN);
    s.set_store_limit(Some(astersql_pd_core::StoreLimiter::V2(
        astersql_pd_storelimit::SlidingWindows::new(),
    )));
    assert!(filter.target(&conf, &s).await.is_ok());
    // A missing limiter is never touched after an earlier hard rejection.
    s.set_store_limit(None);
    s.edit_meta(|m| m.node_state = astersql_pd_core::NodeState::Removed);
    assert_eq!(
        filter.target(&conf, &s).await.status_code,
        StatusCode::STORE_REMOVED
    );
}

#[tokio::test]
async fn task_007_async_chain_and_diagnostics() {
    use astersql_pd_schedule_plan::{BalanceSchedulerPlan, Collector};
    use std::sync::Arc;
    let f = fixture();
    let conf = config(&f);
    let healthy = Arc::new(store(&f["cases"][0]["store"]).await);
    let removed = Arc::new(StoreInfo::new(StoreMeta {
        id: 2,
        node_state: astersql_pd_core::NodeState::Removed,
        ..Default::default()
    }));
    let stores = [healthy.clone(), removed.clone(), healthy.clone()];
    let state = StoreStateFilter {
        transfer_leader: true,
        now_unix_nanos: f["now_unix_nanos"].as_i64().unwrap() as i128,
        ..Default::default()
    };
    let excluded = ExcludedFilter::new("excluded", [2], [2]);
    let sync = SynchronousFilter(&excluded);
    for (filters, expected) in [
        (
            [&sync as &dyn AsyncFilter<()>, &state],
            StatusCode::STORE_ALREADY_HAS_PEER,
        ),
        (
            [&state as &dyn AsyncFilter<()>, &sync],
            StatusCode::STORE_REMOVED,
        ),
    ] {
        for direction in [
            StoreSelection::Source,
            StoreSelection::Target,
            StoreSelection::UnavailableTarget,
        ] {
            let mut collector = Collector::new(BalanceSchedulerPlan::default());
            let selected =
                select_stores_async(&stores, &filters, &conf, Some(&mut collector), direction)
                    .await;
            if matches!(direction, StoreSelection::UnavailableTarget) {
                assert_eq!(selected.len(), 1);
                assert!(Arc::ptr_eq(&selected[0], &removed));
                assert_eq!(collector.get_plans()[0].step, 2);
                assert_eq!(collector.get_plans()[0].target.as_ref().unwrap().id(), 2);
            } else {
                assert_eq!(selected.len(), 2);
                assert!(selected.iter().all(|s| Arc::ptr_eq(s, &healthy)));
                assert_eq!(collector.get_plans()[0].step, 0);
                assert_eq!(collector.get_plans()[0].source.as_ref().unwrap().id(), 2);
            }
            assert_eq!(collector.get_plans().len(), 1);
            assert_eq!(collector.get_plans()[0].status.status_code, expected);
            assert_eq!(collector.base_plan.step, 0);
        }
    }
    assert!(
        select_stores_async(&[], &[&state], &conf, None, StoreSelection::Target)
            .await
            .is_empty()
    );
    assert_eq!(
        select_stores_async(&stores, &[], &conf, None, StoreSelection::Source)
            .await
            .len(),
        3
    );
    assert!(
        select_stores_async(&stores, &[], &conf, None, StoreSelection::UnavailableTarget)
            .await
            .is_empty()
    );
}

#[tokio::test]
#[should_panic(expected = "nil store limiter")]
async fn task_007_nil_limiter_fault() {
    let f = fixture();
    let conf = config(&f);
    let mut s = store(&f["cases"][0]["store"]).await;
    s.set_store_limit(None);
    StoreStateFilter {
        move_region: true,
        ..Default::default()
    }
    .source(&conf, &s)
    .await;
}

#[tokio::test]
#[should_panic(expected = "resource type does not match balance step")]
async fn task_007_collector_resource_fault() {
    use astersql_pd_schedule_plan::{BalanceSchedulerPlan, Collector};
    let conf = config(&fixture());
    let stores = [std::sync::Arc::new(StoreInfo::new(StoreMeta {
        node_state: astersql_pd_core::NodeState::Removed,
        ..Default::default()
    }))];
    let filter = StoreStateFilter {
        transfer_leader: true,
        ..Default::default()
    };
    let mut collector = Collector::new(BalanceSchedulerPlan {
        step: 1,
        ..Default::default()
    });
    select_stores_async(
        &stores,
        &[&filter],
        &conf,
        Some(&mut collector),
        StoreSelection::Target,
    )
    .await;
}
