// Copyright 2026 AsterSQL.
use crate::{LabelRule, LabelRuleIndex};
use serde_json::json;
fn rule(id: &str, index: i64, ranges: &[(&str, &str)]) -> LabelRule {
    LabelRule::from_json(&serde_json::to_vec(&json!({
        "id":id,"index":index,"labels":[{"key":"k","value":id}],"rule_type":"key-range",
        "data": ranges.iter().map(|(start,end)|json!({"start_key":start,"end_key":end})).collect::<Vec<_>>()
    })).unwrap(),0).unwrap()
}
fn initial() -> LabelRuleIndex {
    LabelRuleIndex::new([
        rule("low", -2, &[("10", "40")]),
        rule("high", 5, &[("20", "30")]),
        rule("probe", 1, &[("50", "60")]),
    ])
}
fn ids(index: &LabelRuleIndex, start: &[u8], end: &[u8]) -> Option<Vec<String>> {
    index.get_range_rules(start, end).map(|rules| {
        let mut ids: Vec<_> = rules.iter().map(|r| r.id.clone()).collect();
        ids.sort();
        ids
    })
}
#[test]
fn label_index_go_boundaries_and_priority() {
    let index = initial();
    assert_eq!(
        index.get_split_keys(&[], &[]),
        vec![&[0x10][..], &[0x20], &[0x30], &[0x40], &[0x50], &[0x60]]
    );
    assert_eq!(
        index.get_split_keys(&[0x10], &[0x40]),
        vec![&[0x20][..], &[0x30]]
    );
    for (start, end, expected) in [
        (&[][..], &[0x10][..], vec![]),
        (&[0x10], &[0x20], vec!["low"]),
        (&[0x20], &[0x30], vec!["high", "low"]),
        (&[0x30], &[0x40], vec!["low"]),
        (&[0x40], &[0x50], vec![]),
        (&[0x60], &[], vec![]),
    ] {
        assert_eq!(
            ids(&index, start, end),
            Some(expected.into_iter().map(String::from).collect())
        );
    }
    assert!(index.get_range_rules(&[0x21], &[0x31]).is_none());
    let rules = index.get_range_rules(&[0x20], &[0x30]).unwrap();
    assert_eq!(rules.iter().max_by_key(|r| r.index).unwrap().id, "high");
}
#[test]
fn label_index_delete_and_replace_publish() {
    let mut index = initial();
    index.delete("high");
    assert_eq!(
        index.get_split_keys(&[], &[]),
        vec![&[0x10][..], &[0x40], &[0x50], &[0x60]]
    );
    assert_eq!(ids(&index, &[0x10], &[0x40]), Some(vec!["low".into()]));
    index.set(rule("low", 7, &[("50", "60")]));
    assert_eq!(index.get_split_keys(&[], &[]), vec![&[0x50][..], &[0x60]]);
    assert_eq!(
        ids(&index, &[0x50], &[0x60]),
        Some(vec!["low".into(), "probe".into()])
    );
    index.set(rule("low", 7, &[("70", "")]));
    assert_eq!(
        index.get_split_keys(&[], &[]),
        vec![&[0x50][..], &[0x60], &[0x70]]
    );
    assert_eq!(ids(&index, &[0x10], &[0x40]), Some(vec![]));
    assert_eq!(index.rule("low").unwrap().index, 7);
    index.delete("missing");
    index.delete("low");
    index.delete("probe");
    assert!(index.get_split_keys(&[], &[]).is_empty());
    assert_eq!(ids(&index, &[], &[]), Some(vec![]));
}
#[test]
fn label_index_multi_ranges_overlap_and_snapshot() {
    let index = LabelRuleIndex::new([rule("r", 0, &[("10", "30"), ("20", "40"), ("50", "")])]);
    assert_eq!(
        ids(&index, &[0x20], &[0x30]),
        Some(vec!["r".into(), "r".into()])
    );
    assert_eq!(ids(&index, &[0x40], &[0x50]), Some(vec![]));
    assert_eq!(ids(&index, &[0x50], &[]), Some(vec!["r".into()]));
    assert!(index.get_range_rules(&[], &[]).is_none());
    let empty = LabelRuleIndex::default();
    assert_eq!(ids(&empty, &[], &[]), Some(vec![]));
    let index = LabelRuleIndex::new([rule("r", 1, &[("10", "20")]), rule("r", 2, &[("30", "40")])]);
    assert_eq!(index.rules().count(), 1);
    assert_eq!(index.get_split_keys(&[], &[]), vec![&[0x30][..], &[0x40]]);
}

#[test]
fn label_index_expiry_snapshot_refresh() {
    let mut timed = rule("timed", 0, &[("10", "20")]);
    timed.labels[0].ttl = "1ns".into();
    timed.check_and_adjust(0).unwrap();
    let mut index = LabelRuleIndex::new([timed, rule("permanent", 0, &[("30", "")])]);
    assert_eq!(index.min_expire(), Some(1));
    assert!(!index.remove_expired_labels("timed", 1));
    assert!(!index.remove_expired_labels("missing", 2));
    assert!(index.remove_expired_labels("timed", 2));
    assert_eq!(index.min_expire(), None);
    assert!(index.rule("timed").unwrap().labels.is_empty());
    // GC owns deletion, not expiry filtering; range boundaries remain until delete.
    assert_eq!(ids(&index, &[0x10], &[0x20]), Some(vec!["timed".into()]));
    index.delete("timed");
    assert_eq!(index.get_split_keys(&[], &[]), vec![&[0x30][..]]);
}
