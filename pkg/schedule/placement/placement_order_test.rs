// Copyright 2026 AsterSQL.
use crate::{
    PeerRoleType, Rule, RuleGroup, RuleList, check_apply_rules, compare_rule,
    prepare_rules_for_apply,
};
use std::sync::Arc;
fn rule(
    group: &str,
    id: &str,
    index: i64,
    override_: bool,
    group_index: i64,
    group_override: bool,
) -> Arc<Rule> {
    Arc::new(Rule {
        group_id: group.into(),
        id: id.into(),
        index,
        override_,
        role: PeerRoleType::Voter,
        count: 1,
        group: Some(RuleGroup {
            id: group.into(),
            index: group_index,
            override_: group_override,
        }),
        ..Rule::default()
    })
}
fn keys(rules: &[Arc<Rule>]) -> Vec<[String; 2]> {
    rules.iter().map(|r| r.key()).collect()
}
#[test]
fn placement_order_priority_and_override() {
    let mut rules = vec![
        rule("g3", "e", 0, false, 2, false),
        rule("g2", "d", 1, false, 1, true),
        rule("g2", "c", 0, true, 1, true),
        rule("g1", "a", 0, false, 0, false),
        rule("g2", "b", 0, false, 1, true),
    ];
    rules.sort_by(|a, b| compare_rule(a, b));
    let selected = prepare_rules_for_apply(&rules);
    assert_eq!(
        keys(&selected),
        vec![
            [String::from("g2"), String::from("c")],
            [String::from("g2"), String::from("d")],
            [String::from("g3"), String::from("e")]
        ]
    );
    assert!(Arc::ptr_eq(&selected[0], &rules[2]));
    assert!(prepare_rules_for_apply(&[]).is_empty());
}
#[test]
fn placement_order_rule_override_and_ties() {
    let mut rules = vec![
        rule("g", "z", -1, true, 0, false),
        rule("g", "a", -1, true, 0, false),
        rule("g", "after", 0, false, 0, false),
    ];
    rules.sort_by(|a, b| compare_rule(a, b));
    assert_eq!(
        prepare_rules_for_apply(&rules)
            .iter()
            .map(|r| r.id.as_str())
            .collect::<Vec<_>>(),
        ["z", "after"]
    );
    assert!(
        compare_rule(
            &rule("b", "a", -99, false, -1, false),
            &rule("a", "a", -99, false, 0, false)
        )
        .is_lt()
    );
}
fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}
#[test]
fn placement_order_go_runtime_fixture() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule/placement/task22/go.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let mut rules = Vec::new();
        for bundle in case["input"].as_array().unwrap() {
            for value in bundle["rules"].as_array().unwrap() {
                let mut r: Rule = serde_json::from_value(value.clone()).unwrap();
                r.group_id = bundle["group_id"].as_str().unwrap().into();
                r.start_key = hex(&r.start_key_hex);
                r.end_key = hex(&r.end_key_hex);
                r.group = Some(RuleGroup {
                    id: r.group_id.clone(),
                    index: bundle["group_index"].as_i64().unwrap(),
                    override_: bundle["group_override"].as_bool().unwrap(),
                });
                rules.push(Arc::new(r));
            }
        }
        let built = RuleList::build(rules);
        if case["status"] == 500 {
            assert_eq!(
                built.unwrap_err().to_string(),
                case["output"].as_str().unwrap(),
                "{}",
                case["name"]
            );
            continue;
        }
        assert_eq!(case["status"], 200, "unusable oracle {}", case["name"]);
        let list = built.unwrap();
        for (key, values) in case["keys"].as_object().unwrap() {
            let expected: Vec<[String; 2]> = values
                .as_array()
                .unwrap()
                .iter()
                .map(|v| {
                    [
                        v["group_id"].as_str().unwrap().into(),
                        v["id"].as_str().unwrap().into(),
                    ]
                })
                .collect();
            assert_eq!(
                keys(list.get_rules_by_key(&hex(key))),
                expected,
                "key {key} {}",
                case["name"]
            );
        }
        for region in case["regions"].as_array().unwrap() {
            let expected: Vec<[String; 2]> = region["rules"].as_array().map_or(Vec::new(), |v| {
                v.iter()
                    .map(|r| {
                        [
                            r["group_id"].as_str().unwrap().into(),
                            r["id"].as_str().unwrap().into(),
                        ]
                    })
                    .collect()
            });
            assert_eq!(
                keys(list.get_rules_for_apply_range(
                    &hex(region["start"].as_str().unwrap()),
                    &hex(region["end"].as_str().unwrap())
                )),
                expected,
                "region {} {}",
                case["name"],
                region
            );
        }
    }
}
#[test]
fn placement_order_role_errors_and_gap() {
    assert_eq!(
        check_apply_rules(&[]),
        Err("needs at least one leader or voter")
    );
    let leader = Arc::new(Rule {
        role: PeerRoleType::Leader,
        count: 1,
        ..Rule::default()
    });
    assert!(check_apply_rules(&[Arc::clone(&leader)]).is_ok());
    assert_eq!(
        check_apply_rules(&[Arc::clone(&leader), leader]),
        Err("multiple leader replicas")
    );
    let a = Arc::new(Rule {
        role: PeerRoleType::Voter,
        count: 1,
        end_key: vec![0xA1],
        ..Rule::default()
    });
    let b = Arc::new(Rule {
        role: PeerRoleType::Voter,
        count: 1,
        start_key: vec![0xA2],
        ..Rule::default()
    });
    let error = RuleList::build([a, b]).unwrap_err();
    assert_eq!(error.0, "no rule for range {A1, A2}");
    assert_eq!(error.code(), "PD:placement:ErrBuildRuleList");
}
