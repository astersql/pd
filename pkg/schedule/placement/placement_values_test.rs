use super::*;
use astersql_pd_core::{StoreInfo, StoreLabel, StoreMeta};
fn store(labels: &[(&str, &str)]) -> StoreInfo {
    StoreInfo::new(StoreMeta {
        labels: labels
            .iter()
            .map(|(k, v)| StoreLabel {
                key: (*k).into(),
                value: (*v).into(),
            })
            .collect(),
        ..Default::default()
    })
}
#[test]
fn placement_values_json_null_duplicate_fields() {
    let r = Rule::from_json(
        br#"{"ID":"first","id":null,"location_labels":["zone"],"location_labels":null}"#,
    )
    .unwrap();
    assert_eq!(r.id, "first");
    assert!(r.location_labels.is_empty());
    let b: GroupBundle = serde_json::from_str(r#"{"rules":[null,{}],"rules":null}"#).unwrap();
    assert_eq!(b.rules, None);
}
#[test]
fn placement_values_rule_json_clone_store_key() {
    let mut r=Rule::from_json(br#"{"group_id":"g-1","id":"id","start_key":"AA","role":"leader","count":2,"unknown":true,"label_constraints":[{"key":"zone","op":"in","values":["z"]}]}"#).unwrap();
    r.start_key = vec![170];
    r.end_key = vec![255];
    r.group = Some(RuleGroup {
        id: "g-1".into(),
        index: 9,
        ..Default::default()
    });
    let c = r.clone();
    r.start_key[0] = 1;
    r.label_constraints[0].values[0] = "changed".into();
    assert_eq!(c.start_key, vec![170]);
    assert_eq!(c.end_key, vec![255]);
    assert_eq!(c.label_constraints[0].values, ["z"]);
    assert_eq!(c.group, None);
    assert_eq!(c.start_key_hex, "AA");
    assert_eq!(r.store_key(), "672d31-6964");
    assert_eq!(r.key(), ["g-1", "id"]);
    let z = Rule::default();
    assert_eq!(
        z.to_string(),
        r#"{"group_id":"","id":"","start_key":"","end_key":"","role":"","is_witness":false,"count":0}"#
    );
    assert_eq!(RuleGroup::default().to_string(), "{}");
    assert!(
        RuleGroup {
            id: "x".into(),
            ..Default::default()
        }
        .is_default()
    );
    assert_eq!(
        GroupBundle::default().to_string(),
        r#"{"group_id":"","group_index":0,"group_override":false,"rules":null}"#
    );
    assert!(Rule::from_json(br#"{"count":1.5}"#).is_err());
    assert!(Rule::from_json(br#"{"version":-1}"#).is_err());
    assert!(Rule::from_json(b"[]").is_err());
    assert_eq!(Rule::from_json(b"null").unwrap(), Rule::default());
    assert_eq!(
        Rule {
            id: "<>&\u{2028}".into(),
            ..Default::default()
        }
        .to_string(),
        r#"{"group_id":"","id":"\u003c\u003e\u0026\u2028","start_key":"","end_key":"","role":"","is_witness":false,"count":0}"#
    );
}
#[test]
fn placement_values_roles_and_operators() {
    for s in ["voter", "leader", "follower", "learner"] {
        let r: PeerRoleType = s.to_owned().into();
        assert!(r.is_valid());
        assert_eq!(String::from(r), s);
    }
    for s in ["", "Voter", "other"] {
        assert!(!PeerRoleType::from(s.to_owned()).is_valid());
    }
    for (op, expected) in [
        ("in", [false, true, false]),
        ("notIn", [true, false, true]),
        ("exists", [false, true, true]),
        ("notExists", [true, false, false]),
        ("bad", [false, false, false]),
    ] {
        let c = LabelConstraint {
            op: op.to_owned().into(),
            values: vec!["x".into(), "".into()],
            ..Default::default()
        };
        assert_eq!(
            [c.match_value(""), c.match_value("x"), c.match_value("y")],
            expected
        );
    }
}
#[test]
fn placement_values_store_constraints() {
    assert!(!match_label_constraints(None, &[]));
    assert!(match_label_constraints(Some(&store(&[])), &[]));
    for key in ["engine", "exclusive", "$foo"] {
        let s = store(&[(key, "")]);
        assert!(!match_label_constraints(Some(&s), &[]));
        let c = LabelConstraint {
            key: key.into(),
            op: LabelConstraintOp::NotExists,
            ..Default::default()
        };
        assert!(match_label_constraints(Some(&s), &[c]));
    }
    let s = store(&[("Zone", "z"), ("Zone", "second")]);
    assert!(
        LabelConstraint {
            key: "zone".into(),
            op: LabelConstraintOp::In,
            values: vec!["z".into()]
        }
        .match_store(&s)
    );
    let s = store(&[("engine", "tikv")]);
    let c = LabelConstraint {
        key: "ENGINE".into(),
        op: LabelConstraintOp::Exists,
        ..Default::default()
    };
    assert!(c.match_store(&s));
    assert!(!match_label_constraints(Some(&s), &[c]));
}

#[test]
fn placement_values_go_runtime_fixture() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule/placement/task21/go.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        if case["status"] != 200 {
            continue;
        }
        let input = serde_json::to_vec(&case["input"]).unwrap();
        let r = Rule::from_json(&input).unwrap();
        let mut expected = case["read"].clone();
        expected.as_object_mut().unwrap().remove("version");
        expected.as_object_mut().unwrap().remove("create_timestamp");
        assert_eq!(
            serde_json::to_value(&r).unwrap(),
            expected,
            "{}",
            case["name"]
        );
        let full = Rule::from_json(&serde_json::to_vec(&case["read"]).unwrap()).unwrap();
        assert_eq!(serde_json::to_value(full.clone()).unwrap(), case["read"]);
    }
    let r = Rule::from_json(
        fixture["duplicate_null"]["input"]
            .as_str()
            .unwrap()
            .as_bytes(),
    )
    .unwrap();
    assert!(r.location_labels.is_empty());
    let mut expected = fixture["duplicate_null"]["read"].clone();
    expected.as_object_mut().unwrap().remove("version");
    expected.as_object_mut().unwrap().remove("create_timestamp");
    assert_eq!(serde_json::to_value(r).unwrap(), expected);
    let bundle: GroupBundle = serde_json::from_value(fixture["bundle"]["output"].clone()).unwrap();
    assert_eq!(
        serde_json::to_value(bundle).unwrap(),
        fixture["bundle"]["output"]
    );
}
#[test]
fn placement_values_go_constraint_matrix() {
    let stores = [
        store(&[("zone", "zone1"), ("rack", "rack1")]),
        store(&[("zone", "zone1"), ("rack", "rack2")]),
        store(&[("zone", "zone2")]),
        store(&[("zone", "zone1"), ("engine", "rocksdb"), ("disk", "hdd")]),
        store(&[("rack", "rack1"), ("disk", "ssd")]),
        store(&[("zone", "zone3")]),
    ];
    let cases = [
        ("zone", "in", vec!["zone1"], vec![1, 2, 4]),
        ("zone", "in", vec!["zone1", "zone2"], vec![1, 2, 3, 4]),
        ("zone", "notIn", vec!["zone1"], vec![3, 5, 6]),
        ("zone", "notIn", vec!["zone1", "zone2"], vec![5, 6]),
        ("zone", "exists", vec![], vec![1, 2, 3, 4, 6]),
        ("disk", "notExists", vec![], vec![1, 2, 3, 6]),
    ];
    for (key, op, values, expected) in cases {
        let c = LabelConstraint {
            key: key.into(),
            op: op.to_owned().into(),
            values: values.into_iter().map(String::from).collect(),
        };
        let actual: Vec<_> = stores
            .iter()
            .enumerate()
            .filter(|(_, s)| c.match_store(s))
            .map(|(i, _)| i + 1)
            .collect();
        assert_eq!(actual, expected);
    }
    let r = Rule::from_json(
        br#"{"location_labels":[null,"z"],"label_constraints":[null,{"values":[null]}]}"#,
    )
    .unwrap();
    assert_eq!(r.location_labels, ["", "z"]);
    assert_eq!(r.label_constraints[0], LabelConstraint::default());
    assert_eq!(r.label_constraints[1].values, [""]);
}

#[test]
fn placement_values_go_runtime_matching() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule/placement/task21/go.json"
    ))
    .unwrap();
    let cases = fixture["matching"].as_array().unwrap();
    assert_eq!(cases.len(), 12);
    for case in cases {
        let labels: Vec<_> = case["labels"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str().unwrap()))
            .collect();
        let s = store(&labels);
        let c: LabelConstraint = serde_json::from_value(case["constraint"].clone()).unwrap();
        assert_eq!(
            match_label_constraints(Some(&s), &[c]),
            case["matched"].as_bool().unwrap(),
            "{case}"
        );
    }
    let g: RuleGroup = serde_json::from_value(fixture["group"].clone()).unwrap();
    assert_eq!(serde_json::to_value(g.clone()).unwrap(), fixture["group"]);
    assert!(!g.is_default());
    let r = Rule::from_json("{\"ſtart_Key\":\"ab\"}".as_bytes()).unwrap();
    assert_eq!(r.start_key_hex, "ab");
    let r = Rule {
        group_id: "分组".into(),
        id: "🦀".into(),
        ..Default::default()
    };
    assert_eq!(r.store_key(), "e58886e7bb84-f09fa680");
}
