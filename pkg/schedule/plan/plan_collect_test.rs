// Copyright 2026 AsterSQL.
use crate::{BalanceSchedulerPlan, Collector, PlanOption as O, Resource as R, Status, StatusCode};
use astersql_pd_core::{RegionInfo, RegionMeta, StoreInfo, StoreMeta};
use std::sync::Arc;
fn base(step: i64) -> BalanceSchedulerPlan {
    BalanceSchedulerPlan {
        source: Some(Arc::new(StoreInfo::new(StoreMeta {
            id: 11,
            ..Default::default()
        }))),
        region: Some(Arc::new(RegionInfo::new(
            RegionMeta {
                id: 22,
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        ))),
        target: Some(Arc::new(StoreInfo::new(StoreMeta {
            id: 33,
            ..Default::default()
        }))),
        step,
        ..Default::default()
    }
}
#[test]
fn plan_collect_completed_steps_only() {
    for step in -1..=4 {
        let b = base(step);
        let p = b.clone_with(&[]);
        assert_eq!(p.source.is_some(), step > 0, "source step {step}");
        assert_eq!(p.region.is_some(), step > 1, "region step {step}");
        assert_eq!(p.target.is_some(), step > 2, "target step {step}");
        assert_eq!(p.step, step);
        if let Some(s) = p.source {
            assert!(Arc::ptr_eq(&s, b.source.as_ref().unwrap()));
        }
    }
}
#[test]
fn plan_collect_options_and_isolation() {
    let b = base(3);
    let status = Status::new(StatusCode::STORE_BUSY, &["busy"]);
    let options = [
        O::SetStatus(status.clone()),
        O::SetResourceWithStep(R::Store(None), 0),
        O::SetResourceWithStep(R::Region(None), 1),
    ];
    let mut p = b.clone_with(&options);
    p.status.detailed_reason.push('!');
    assert_eq!(status.detailed_reason, "busy");
    assert!(b.status.is_ok());
    assert_eq!(b.step, 3);
    assert!(b.source.is_some());
    assert!(p.source.is_none());
    assert!(p.region.is_none());
    assert_eq!(p.step, 1);
    assert_eq!(p.get_resource(2), 0);
    assert_eq!(b.get_resource(0), 11);
    assert_eq!(b.get_resource(1), 22);
    assert_eq!(b.get_resource(2), 33);
}
#[test]
fn plan_collect_default_and_invalid_resources() {
    let mut p = BalanceSchedulerPlan::new();
    assert_eq!(p.get_step(), 0);
    assert!(p.get_status().is_ok());
    for step in [-2, -1, 1, 2, 3, 4] {
        assert_eq!(p.get_resource(step), 0);
    }
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| p.get_resource(0))).is_err());
    for step in [-1, 3, 99] {
        p.set_resource_with_step(R::Region(None), step);
        assert_eq!(p.step, step);
        assert_eq!(p.get_resource(step), 0);
    }
    for (step, resource) in [
        (0, R::Region(None)),
        (1, R::Store(None)),
        (2, R::Region(None)),
    ] {
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || p.set_resource_with_step(resource, step)
            ))
            .is_err()
        );
        assert_eq!(p.step, step);
    }
}
#[test]
fn plan_collect_order_and_owned_results() {
    let mut c = Collector::new(base(3));
    for code in [400, 0, 401, 0] {
        c.collect(&[O::SetStatus(Status::new(StatusCode(code), &[]))]);
    }
    let mut plans = c.get_plans();
    assert_eq!(
        plans
            .iter()
            .map(|p| p.status.status_code.0)
            .collect::<Vec<_>>(),
        [0, 0, 400, 401]
    );
    plans[0].status.detailed_reason.push_str("changed");
    plans[0].source = None;
    c.base_plan.status.detailed_reason.push_str("base");
    assert!(c.get_plans()[0].status.detailed_reason.is_empty());
    assert!(c.get_plans()[0].source.is_some());
    let p = base(0).clone_with(&[
        O::SetResourceWithStep(R::Region(None), 1),
        O::SetStatus(Status::default()),
    ]);
    assert_eq!(p.step, 1);
    let replacement = base(3).target.unwrap();
    let p = base(0).clone_with(&[
        O::SetResourceWithStep(R::Store(None), 2),
        O::SetResource(R::Store(Some(replacement))),
        O::SetStatus(Status::new(StatusCode::STORE_BUSY, &["first"])),
        O::SetStatus(Status::default()),
    ]);
    assert_eq!(p.get_resource(2), 33);
    assert!(p.status.is_ok());
}
