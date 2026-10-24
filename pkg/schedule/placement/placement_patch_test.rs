// Copyright 2026 AsterSQL.
use crate::*;
fn rule(group: &str, id: &str) -> Rule {
    Rule {
        group_id: group.into(),
        id: id.into(),
        role: PeerRoleType::Voter,
        count: 1,
        ..Default::default()
    }
}
#[test]
fn placement_patch_snapshot_and_versions() {
    let mut base = RuleConfig::default();
    let mut r = rule("g", "r");
    r.version = 4;
    r.create_timestamp = 12;
    base.set_rule(r);
    base.set_group(RuleGroup {
        id: "g".into(),
        index: 1,
        ..Default::default()
    });
    base.adjust();
    let old = base.get_rule("g", "r").unwrap().clone();
    let mut patch = base.begin_patch();
    patch.set_group(RuleGroup {
        id: "g".into(),
        index: 9,
        ..Default::default()
    });
    assert_eq!(patch.adjust().get_rule("g", "r").unwrap().group_index(), 9);
    assert_eq!(old.group_index(), 1);
    assert_eq!(base.get_rule("g", "r").unwrap().group_index(), 1);
    let mut updated = old.as_ref().clone();
    updated.count = 2;
    patch.set_rule(updated);
    patch.set_rule(rule("new", "n"));
    let committed = patch.commit(100);
    let updated = committed.get_rule("g", "r").unwrap();
    assert_eq!(
        (
            updated.version,
            updated.create_timestamp,
            updated.group_index()
        ),
        (5, 12, 9)
    );
    assert_eq!(
        committed.get_rule("new", "n").unwrap().create_timestamp,
        100
    );
    assert_eq!(old.count, 1);
    assert!(base.get_rule("new", "n").is_none());
    let mut patch = committed.begin_patch();
    patch.delete_group("g");
    patch.delete_rule("new", "n");
    let final_ = patch.commit(200);
    assert_eq!(final_.get_rule("g", "r").unwrap().group_index(), 0);
    assert!(!final_.groups().contains_key("new"));
    assert_eq!(committed.get_rule("g", "r").unwrap().group_index(), 9);
}
#[test]
fn placement_patch_json_trim_and_default_groups() {
    let mut base = RuleConfig::default();
    base.set_rule(rule("g", "r"));
    base.adjust();
    let mut patch = base.begin_patch();
    patch.delete_rule("g", "r");
    let mut r = rule("g", "r");
    r.start_key = vec![1];
    r.group = Some(RuleGroup {
        id: "g".into(),
        index: 99,
        ..Default::default()
    });
    patch.set_rule(r);
    patch.delete_rule("absent", "r");
    patch.delete_group("absent");
    patch.delete_group("g");
    patch.trim();
    assert_eq!(patch.mutation_counts(), (0, 0));
    let unchanged = patch.commit(100);
    assert_eq!(unchanged.get_rule("g", "r").unwrap().create_timestamp, 0);
    let mut patch = base.begin_patch();
    let mut r = rule("g", "r");
    r.version = 1;
    patch.set_rule(r);
    patch.trim();
    assert_eq!(patch.mutation_counts(), (1, 0));
    let mut patch = base.begin_patch();
    patch.set_rule(rule("z", "a"));
    patch.delete_rule("z", "a");
    patch.set_group(RuleGroup {
        id: "z".into(),
        index: 1,
        ..Default::default()
    });
    patch.delete_group("z");
    patch.trim();
    assert_eq!(patch.mutation_counts(), (0, 0));
}
#[test]
fn placement_patch_engine_restrictions() {
    let cases: Vec<(&str, [bool; 2])> = vec![
        (r#"[]"#, [false, false]),
        (
            r#"[{"key":"zone","op":"in","values":["z1"]}]"#,
            [true, false],
        ),
        (
            r#"[{"key":"engine","op":"in","values":["tiflash"]}]"#,
            [false, false],
        ),
        (
            r#"[{"key":"engine","op":"notIn","values":["tiflash"]}]"#,
            [false, false],
        ),
        (
            r#"[{"key":"engine","op":"in","values":["tiflash"]},{"key":"zone","op":"in","values":["z1"]}]"#,
            [false, true],
        ),
        (
            r#"[{"key":"engine","op":"in","values":["tikv"]}]"#,
            [true, false],
        ),
        (r#"[{"key":"engine","op":"exists"}]"#, [true, false]),
        (r#"[{"key":"engine","op":"notExists"}]"#, [true, false]),
        (
            r#"[{"key":"engine","op":"in","values":["tiflash"]},{"key":"engine","op":"notIn","values":["tiflash"]}]"#,
            [false, false],
        ),
    ];
    for (json, expected) in cases {
        let mut r = rule("g", "r");
        r.label_constraints = serde_json::from_str(json).unwrap();
        let mut c = RuleConfig::default();
        c.set_rule(r);
        c.adjust();
        assert_eq!(
            [
                c.may_restrict_store_load(true),
                c.may_restrict_store_load(false)
            ],
            expected,
            "{json}"
        );
        let mut p = c.begin_patch();
        p.delete_rule("g", "r");
        let next = p.commit(1);
        assert!(!next.may_restrict_store_load(true));
        assert!(!next.may_restrict_store_load(false));
    }
}
#[test]
fn placement_patch_go_runtime_fixture() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule/placement/task23/go.json"
    ))
    .unwrap();
    let steps = fixture["steps"].as_array().unwrap();
    let timestamp = steps[0]["rules"][0]["create_timestamp"].as_u64().unwrap();
    let mut config = RuleConfig::default();
    config.set_rule(rule("pd", "default"));
    config.adjust();
    for step in steps {
        let mut patch = config.begin_patch();
        match step["name"].as_str().unwrap() {
            "insert" => patch.set_rule(rule("g", "r")),
            "same" => patch.set_rule(config.get_rule("g", "r").unwrap().as_ref().clone()),
            "update" => {
                let mut r = config.get_rule("g", "r").unwrap().as_ref().clone();
                r.count = 2;
                patch.set_rule(r);
            }
            "group" => patch.set_group(RuleGroup {
                id: "g".into(),
                index: 8,
                ..Default::default()
            }),
            "default_group" => patch.delete_group("g"),
            "delete" => patch.delete_rule("g", "r"),
            _ => panic!("unknown fixture step"),
        }
        patch.adjust();
        patch.trim();
        config = patch.commit(timestamp);
        let rules: Vec<_> = config
            .rules()
            .filter(|r| r.group_id == "g")
            .map(|r| serde_json::to_value(r.as_ref()).unwrap())
            .collect();
        let expected = step["rules"].as_array().cloned().unwrap_or_default();
        assert_eq!(rules, expected, "{}", step["name"]);
        let mut groups: Vec<_> = config
            .groups()
            .values()
            .map(|g| serde_json::to_value(g).unwrap())
            .collect();
        let mut expected = step["groups"].as_array().unwrap().clone();
        groups.sort_by_key(|g| g["id"].as_str().unwrap().to_owned());
        expected.sort_by_key(|g| g["id"].as_str().unwrap().to_owned());
        assert_eq!(groups, expected, "{}", step["name"]);
    }
}
#[test]
fn placement_patch_preview_engine_summary() {
    let base = RuleConfig::default();
    let mut patch = base.begin_patch();
    let mut r = rule("g", "r");
    r.label_constraints = vec![LabelConstraint {
        key: "zone".into(),
        op: LabelConstraintOp::In,
        values: vec!["z1".into()],
    }];
    patch.set_rule(r);
    // The returned RuleConfig preview must describe its effective rules, rather
    // than retain the base snapshot's stale cached engine summary.
    assert!(patch.adjust().may_restrict_store_load(true));
    assert!(!base.may_restrict_store_load(true));
    patch.delete_rule("g", "r");
    assert!(!patch.adjust().may_restrict_store_load(true));
}
