// Copyright 2026 AsterSQL.
use crate::*;
use serde_json::{Value, json};
const NOW: Timestamp = 1_792_800_000_123_456_789;
fn input() -> Value {
    json!({"id":"r", "index":-2,"labels":[{"key":"k","value":"v"}],"rule_type":"key-range","data":[{"start_key":"AA","end_key":""}]})
}
fn decode(value: &Value) -> Result<LabelRule, RuleError> {
    LabelRule::from_json(&serde_json::to_vec(value).unwrap(), NOW)
}

#[test]
fn label_rule_go_oracle_cases() {
    let oracle: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule/labeler/task14/go.json"
    ))
    .unwrap();
    for case in oracle["cases"].as_array().unwrap() {
        let result = decode(&case["input"]);
        let name = case["name"].as_str().unwrap();
        if case["status"] == 200 {
            let rule = result.unwrap_or_else(|e| panic!("{name}: {e}"));
            let mut actual = serde_json::to_value(rule).unwrap();
            let mut expected = case["adjusted"].clone();
            if name == "auto" {
                actual["labels"][0]["start_at"] = json!("NORMALIZED");
                expected["labels"][0]["start_at"] = json!("NORMALIZED");
            }
            assert_eq!(actual, expected, "{name}");
        } else {
            let error = result.expect_err(name);
            let expected = case["output"].as_str().unwrap();
            assert!(
                expected.starts_with(&format!("[{}]", error.code().unwrap())),
                "{name}: {error}"
            );
            if ![
                "object",
                "null",
                "item_number",
                "missing_start",
                "missing_end",
                "numeric_start",
                "invalid_ttl",
                "overflow_ttl",
                "invalid_start",
                "invalid_zone",
                "invalid_day",
            ]
            .contains(&name)
            {
                assert_eq!(error.to_string(), expected, "{name}");
            }
        }
    }
}
#[test]
fn label_rule_expiration_boundary_and_minimum() {
    let mut data = input();
    data["labels"] = json!([
        {"key":"permanent","value":"v","start_at":"ignored"},
        {"key":"first","value":"v","ttl":"1ns","start_at":"Thu Jan  1 00:00:00 UTC 1970"},
        {"key":"second","value":"v","ttl":"2ns","start_at":"Thu Jan  1 00:00:00 UTC 1970"}]);
    let mut rule = LabelRule::from_json(&serde_json::to_vec(&data).unwrap(), 0).unwrap();
    assert_eq!(rule.min_expire(), Some(1));
    assert!(!rule.expire_before(1));
    assert!(!rule.labels[1].expire_before(1));
    assert!(!rule.check_and_remove_expire_labels(1));
    assert!(rule.check_and_remove_expire_labels(2));
    assert_eq!(
        rule.labels
            .iter()
            .map(|l| l.key.as_str())
            .collect::<Vec<_>>(),
        ["permanent", "second"]
    );
    assert_eq!(rule.min_expire(), Some(2));
    assert!(rule.check_and_remove_expire_labels(3));
    assert_eq!(rule.min_expire(), None);
    assert!(!rule.expire_before(i128::MAX));
}
#[test]
fn label_rule_auto_start_roundtrip_precision() {
    let mut data = input();
    data["labels"][0]["ttl"] = json!("10h10m10s10ms");
    let rule = decode(&data).unwrap();
    let l = &rule.labels[0];
    assert_eq!(l.expires_at(), Some(NOW + 36_610_010_000_000));
    assert!(l.start_at.ends_with("UTC 2026"));
    let again = decode(&serde_json::to_value(&rule).unwrap()).unwrap();
    assert_eq!(
        l.expires_at().unwrap() - again.labels[0].expires_at().unwrap(),
        123_456_789
    );
}
#[test]
fn label_rule_hex_bytes_and_case_preservation() {
    let rule = decode(&input()).unwrap();
    let r = &rule.key_ranges().unwrap()[0];
    assert_eq!(r.start_key, vec![0xaa]);
    assert!(r.end_key.is_empty());
    assert_eq!(r.start_key_hex, "AA");
    for (start, end) in [("aa", "AA"), ("ff", "aa")] {
        let mut d = input();
        d["data"] = json!([{"start_key":start,"end_key":end}]);
        assert!(decode(&d).is_err(), "non-increasing range {start}..{end}");
    }
}
#[test]
fn label_rule_unchecked_and_json_errors() {
    let rule = LabelRule::from_json_without_check(b"{}").unwrap();
    assert!(rule.key_ranges().is_none());
    assert!(rule.labels.is_empty());
    for data in [
        b"{".as_slice(),
        b"[]",
        b"{\"index\":1.5}",
        b"{\"labels\":42}",
        b"{\"id\":false}",
    ] {
        assert!(matches!(
            LabelRule::from_json(data, NOW),
            Err(RuleError::Json(_))
        ));
    }
}
#[test]
fn label_rule_ttl_signed_zero_and_explicit_start() {
    for ttl in [
        "0",
        "0s",
        "-0s",
        "1ns",
        "+1ns",
        ".5h",
        "1h30m",
        "1µs",
        "1μs",
        "9223372036854775807ns",
        "-9223372036854775808ns",
    ] {
        let mut l: RegionLabel =
            serde_json::from_value(json!({"key":"k","value":"v","ttl":ttl})).unwrap();
        l.check_and_adjust_expire(NOW).unwrap();
        l.ttl.clear();
        l.check_and_adjust_expire(NOW).unwrap();
        assert_eq!(l.expires_at(), None);
    }
}

#[test]
fn label_rule_json_null_case_and_duplicate_fields() {
    let rule=LabelRule::from_json_without_check(br#"{"id":"first","ID":"second","id":null,"labels":[null,{"key":"k","key":null,"VALUE":"v"}]}"#).unwrap();
    assert_eq!(rule.id, "second");
    assert_eq!(rule.labels[0], RegionLabel::default());
    assert_eq!(rule.labels[1].key, "k");
    assert!(
        LabelRule::from_json_without_check(b"null")
            .unwrap()
            .id
            .is_empty()
    );
}
#[test]
fn label_rule_timezone_and_calendar_expiry() {
    for (zone, offset) in [
        ("UTC", 0),
        ("XYZ", 0),
        ("ChST", 0),
        ("+03", 0),
        ("GMT+8", 8 * 3600),
        ("GMT-3", -3 * 3600),
    ] {
        let mut l: RegionLabel = serde_json::from_value(
            json!({"ttl":"1ns","start_at":format!("Mon Jan  1 00:00:00 {zone} 2100")}),
        )
        .unwrap();
        l.check_and_adjust_expire(0).unwrap();
        assert_eq!(
            l.expires_at(),
            Some((4_102_444_800_i128 - offset) * 1_000_000_000 + 1)
        );
    }
}

#[test]
fn label_rule_canonical_range_json_discards_unknown_fields() {
    let mut value = input();
    value["data"][0]["extra"] = json!({"untrusted":"field"});
    let rule = decode(&value).unwrap();
    assert_eq!(rule.data, json!([{"start_key":"AA","end_key":""}]));
    assert_eq!(
        serde_json::to_value(rule).unwrap()["data"],
        json!([{"start_key":"AA","end_key":""}])
    );
}
