// Copyright 2026 AsterSQL.
use super::ranges::*;
use astersql_pd_keyutil::KeyRange;
use astersql_pd_schedule_labeler::{KeyRangeRule, RegionLabel};
fn group(id: &str, pairs: &[(&str, &str)]) -> GroupKeyRanges {
    GroupKeyRanges {
        group_id: id.into(),
        key_ranges: pairs.iter().map(|(a, b)| KeyRange::new(a, b)).collect(),
    }
}
#[test]
fn affinity_ranges_label_roundtrip() {
    let g = group("g", &[("a", "b"), ("b", "c")]);
    let mut rule = make_label_rule(&g);
    assert_eq!(rule.id, "affinity_group/g");
    assert_eq!(
        rule.data,
        serde_json::json!([{"start_key":"61","end_key":"62"},{"start_key":"62","end_key":"63"}])
    );
    assert!(
        extract_key_ranges_from_label_rule(Some(&rule))
            .unwrap_err()
            .to_string()
            .contains("got type []interface {}")
    );
    rule.check_and_adjust(0).unwrap();
    assert_eq!(extract_key_ranges_from_label_rule(Some(&rule)).unwrap(), g);
    assert_eq!(
        make_label_rule(&group("e", &[])).data,
        serde_json::Value::Null
    );
    assert_eq!(
        extract_key_ranges_from_label_rule(None).unwrap(),
        GroupKeyRanges::default()
    );
}
#[test]
fn affinity_ranges_identity() {
    let mut rule = make_label_rule(&group("bad/id", &[]));
    assert_eq!(
        parse_affinity_group_id_from_label_rule(Some(&rule)),
        Some("bad/id")
    );
    let mut label = RegionLabel::default();
    label.key = LABEL_KEY.into();
    label.value = "wrong".into();
    rule.labels.insert(0, label);
    assert_eq!(parse_affinity_group_id_from_label_rule(Some(&rule)), None);
    rule.id = "affinity_group/".into();
    assert_eq!(parse_affinity_group_id_from_label_rule(Some(&rule)), None);
    assert_eq!(parse_affinity_group_id_from_label_rule(None), None);
}
#[test]
fn affinity_ranges_hex() {
    assert_eq!(
        decode_hex_key("00fFA0", "g", "start").unwrap(),
        vec![0, 255, 160]
    );
    assert!(decode_hex_key("", "g", "start").unwrap().is_empty());
    for bad in ["a", "gg", "é", "0é"] {
        assert_eq!(
            decode_hex_key(bad, "g", "end").unwrap_err().to_string(),
            format!("[PD:affinity:ErrInvalidKeyFormat]invalid hex end key '{bad}' for group 'g'")
        );
    }
    let r = KeyRangeRule {
        start_key: vec![9],
        end_key: vec![9],
        start_key_hex: "61".into(),
        end_key_hex: "".into(),
    };
    assert_eq!(
        parse_key_ranges_from_data(&[None, Some(&r)], "g").unwrap(),
        group("g", &[("a", "")])
    );
}
#[test]
fn affinity_ranges_remove() {
    let g = group("g", &[("a", "b"), ("b", "c")]);
    assert_eq!(
        apply_remove_ops(&g, &group("other", &[("a", "b")])).unwrap(),
        group("g", &[("b", "c")])
    );
    assert_eq!(apply_remove_ops(&g, &group("g", &[])).unwrap(), g);
    for pairs in [vec![("a", "c")], vec![("x", "y")]] {
        assert!(
            apply_remove_ops(&g, &group("g", &pairs))
                .unwrap_err()
                .to_string()
                .contains("not found")
        );
    }
    assert!(
        apply_remove_ops(&g, &group("g", &[("a", "b"), ("a", "c")]))
            .unwrap_err()
            .to_string()
            .contains("already exists")
    );
    assert!(apply_remove_ops(&g, &g).unwrap().key_ranges.is_empty());
}
#[test]
fn affinity_ranges_overlap() {
    for pairs in [
        vec![("a", "b"), ("b", "c")],
        vec![("", "a"), ("a", "")],
        vec![],
    ] {
        validate_no_overlap(&[group("g", &pairs)]).unwrap();
    }
    assert!(
        validate_no_overlap(&[group("g", &[("a", "c"), ("b", "d")])])
            .unwrap_err()
            .to_string()
            .contains("within group g")
    );
    assert!(
        validate_no_overlap(&[group("b", &[("b", "d")]), group("a", &[("a", "c")])])
            .unwrap_err()
            .to_string()
            .contains("between groups: a and b")
    );
    for (a, b) in [
        (KeyRange::new("", ""), KeyRange::new("a", "b")),
        (KeyRange::new("a", "b"), KeyRange::new("a", "c")),
        (KeyRange::new("a", "c"), KeyRange::new("b", "c")),
    ] {
        assert!(check_key_ranges_overlap(&a, &b));
    }
}

#[test]
fn affinity_ranges_go_runtime_fixture() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule/affinity/task32/go.json"
    ))
    .unwrap();
    let ranges = GroupKeyRanges {
        group_id: "range32".into(),
        key_ranges: vec![KeyRange::new([0], [1]), KeyRange::new([1], [255])],
    };
    let mut rule = make_label_rule(&ranges);
    assert_eq!(serde_json::to_value(&rule).unwrap(), fixture["label_rule"]);
    rule.check_and_adjust(0).unwrap();
    assert_eq!(
        extract_key_ranges_from_label_rule(Some(&rule)).unwrap(),
        ranges
    );
}
