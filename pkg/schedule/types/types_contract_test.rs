// Copyright 2026 AsterSQL.

use std::collections::BTreeMap;

use super::*;

fn oracle(nextgen: bool) -> serde_json::Value {
    serde_json::from_str(if nextgen {
        include_str!("fixtures/nextgen.json")
    } else {
        include_str!("fixtures/classic.json")
    })
    .unwrap()
}

#[test]
fn types_contract_kernel_defaults() {
    for nextgen in [false, true] {
        let actual: Vec<_> = default_schedulers(nextgen)
            .into_iter()
            .map(|t| t.to_string())
            .collect();
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            oracle(nextgen)["defaults"]
        );
    }
    assert_eq!(
        current_kernel_default_schedulers(),
        default_schedulers(astersql_pd_kerneltype::is_next_gen())
    );
    let mut owned = default_schedulers(false);
    owned.clear();
    assert_eq!(default_schedulers(false).len(), 5);
}

#[test]
fn types_contract_runtime_conversion_maps() {
    for nextgen in [false, true] {
        let fixture = oracle(nextgen);
        for (input, expected) in fixture["scheduler"].as_object().unwrap() {
            assert_eq!(
                string_to_scheduler_type(input).unwrap().as_str(),
                expected.as_str().unwrap()
            );
        }
        for (input, expected) in fixture["legacy"].as_object().unwrap() {
            assert_eq!(
                convert_old_str_to_type(input).unwrap().as_str(),
                expected.as_str().unwrap()
            );
        }
        let actual: BTreeMap<_, _> = SCHEDULER_TYPE_COMPATIBLE_MAP
            .into_iter()
            .map(|(t, old)| (t.to_string(), old))
            .collect();
        assert_eq!(serde_json::to_value(actual).unwrap(), fixture["compatible"]);
        assert_eq!(
            fixture["scheduler"].as_object().unwrap().len(),
            SCHEDULERS.len() + 1
        );
    }
}

#[test]
fn types_contract_checker_constants_and_strings() {
    assert_eq!(
        CHECKERS.map(|t| t.to_string()),
        [
            "joint-state-checker",
            "learner-checker",
            "merge-checker",
            "replica-checker",
            "rule-checker",
            "split-checker",
            "affinity-checker",
            "split-scatter-checker",
        ]
    );
    for checker in CHECKERS {
        assert!(is_checker(&checker));
        assert!(!is_scheduler(&checker));
        assert_eq!(string_to_scheduler_type(checker.as_str()), None);
        assert_eq!(convert_old_str_to_type(checker.as_str()), None);
        assert_eq!(scheduler_type_compatible(&checker), None);
        assert_eq!(checker.to_string(), checker.as_str());
    }
    for scheduler in SCHEDULERS {
        assert!(is_scheduler(&scheduler));
        assert!(!is_checker(&scheduler));
        assert_eq!(
            string_to_scheduler_type(scheduler.as_str()),
            Some(scheduler.clone())
        );
        assert_eq!(
            convert_old_str_to_type(scheduler_type_compatible(&scheduler).unwrap()),
            Some(scheduler)
        );
    }
}

#[test]
fn types_contract_unknown_strings_are_preserved_not_classified() {
    for value in [
        "",
        "unknown",
        "BALANCE-LEADER-SCHEDULER",
        " balance-leader-scheduler",
        "balance-leader-scheduler ",
        "自定义-checker",
        "unknown-scheduler",
        "unknown-checker",
    ] {
        let t = CheckerSchedulerType::from(value);
        assert_eq!(t.to_string(), value);
        assert!(!is_checker(&t));
        assert!(!is_scheduler(&t));
        assert_eq!(string_to_scheduler_type(value), None);
        assert_eq!(convert_old_str_to_type(value), None);
        assert_eq!(scheduler_type_compatible(&t), None);
    }
    assert_eq!(CheckerSchedulerType::default().as_str(), "");
    assert_eq!(
        CheckerSchedulerType::from("custom".to_owned()),
        CheckerSchedulerType::from_static("custom")
    );
}

#[test]
fn types_contract_legacy_and_scatter_alias_boundaries() {
    assert_eq!(string_to_scheduler_type("balance-leader"), None);
    assert_eq!(string_to_scheduler_type("hot-region"), None);
    assert_eq!(
        string_to_scheduler_type("scatter-range"),
        Some(SCATTER_RANGE_SCHEDULER)
    );
    assert_eq!(convert_old_str_to_type("scatter-range-scheduler"), None);
    assert_eq!(convert_old_str_to_type("balance-leader-scheduler"), None);
    assert!(!is_scheduler(&CheckerSchedulerType::from("scatter-range")));
}
