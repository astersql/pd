// Copyright 2026 AsterSQL.
use crate::{BalanceSchedulerPlan, Status, StatusCode, balance_plan_summary};
use astersql_pd_core::{Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta};
use std::{any::Any, collections::BTreeMap, sync::Arc};
fn plans(case: &str) -> Vec<BalanceSchedulerPlan> {
    include_str!("../../../docs/rust/schedule/plan/task8/inputs.tsv")
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split('\t').collect();
            if fields[0] != case {
                return None;
            }
            let nums: Vec<i64> = fields[1..].iter().map(|s| s.parse().unwrap()).collect();
            let store = |id| {
                (id != 0).then(|| {
                    Arc::new(StoreInfo::new(StoreMeta {
                        id: id as u64,
                        ..Default::default()
                    }))
                })
            };
            Some(BalanceSchedulerPlan {
                source: store(nums[0]),
                region: (nums[1] != 0).then(|| {
                    Arc::new(RegionInfo::new(
                        RegionMeta {
                            id: nums[1] as u64,
                            ..Default::default()
                        },
                        Some(Peer {
                            id: nums[1] as u64,
                            store_id: nums[1] as u64,
                            ..Default::default()
                        }),
                        vec![],
                        vec![],
                    ))
                }),
                target: store(nums[2]),
                step: nums[3],
                status: Status::new(StatusCode(nums[4]), &[]),
            })
        })
        .collect()
}
fn check(case: &str, codes: &[i64], normal: bool) {
    let plans = plans(case);
    let inputs: Vec<&dyn Any> = plans.iter().map(|p| p as &dyn Any).collect();
    let (actual, flag) = balance_plan_summary(&inputs).unwrap();
    let expected: BTreeMap<_, _> = codes
        .iter()
        .enumerate()
        .map(|(i, c)| (i as u64 + 1, Status::new(StatusCode(*c), &[])))
        .collect();
    assert_eq!(actual, expected, "{case}");
    assert_eq!(flag, normal, "{case}");
}
#[test]
fn plan_summary_analyzer1() {
    check("analyzer1", &[102; 5], true);
}
#[test]
fn plan_summary_analyzer2() {
    check("analyzer2", &[403; 5], false);
}
#[test]
fn plan_summary_analyzer3() {
    check("analyzer3", &[1004, 1004, 1004, 1004, 403], false);
}
#[test]
fn plan_summary_analyzer4() {
    check("analyzer4", &[101, 101, 102, 102, 403], false);
}
#[test]
fn plan_summary_analyzer5() {
    check("analyzer5", &[101, 101, 102, 102, 203], false);
}
#[test]
fn plan_summary_analyzer6() {
    check("analyzer6", &[403; 5], false);
}
#[test]
fn plan_summary_empty_success() {
    check("empty", &[], true);
    check("success", &[0], false);
}
#[test]
fn plan_summary_selection() {
    check("majority", &[403], false);
    check("tie", &[400], false);
    check("priority", &[102], true);
}
#[test]
fn plan_summary_depth_and_order() {
    check("skip_lower", &[102], true);
    check("replace_lower", &[102], false);
}
#[test]
fn plan_summary_invalid_plan() {
    let p = plans("success").pop().unwrap();
    let err = balance_plan_summary(&[&p, &42u64]).unwrap_err();
    assert_eq!(err.code(), "PD:diagnostic:ErrDiagnosticLoadPlan");
    assert_eq!(
        err.to_string(),
        "[PD:diagnostic:ErrDiagnosticLoadPlan]load plan failed"
    );
}
#[test]
fn plan_summary_reason_counts_and_owned_output() {
    let mut p = plans("majority");
    p[0].status.detailed_reason = "a".into();
    p[1].status.detailed_reason = "b".into();
    p[2].status = p[1].status.clone();
    let inputs: Vec<&dyn Any> = p.iter().map(|v| v as &dyn Any).collect();
    let (mut result, _) = balance_plan_summary(&inputs).unwrap();
    assert_eq!(result[&1].detailed_reason, "b");
    result.get_mut(&1).unwrap().detailed_reason.push('!');
    assert_eq!(p[1].status.detailed_reason, "b");
}
