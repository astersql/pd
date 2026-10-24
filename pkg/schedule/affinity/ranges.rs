// Copyright 2026 AsterSQL.
use astersql_pd_errs::{
    TypedError,
    catalog::{
        ERR_AFFINITY_GROUP_CONTENT, ERR_AFFINITY_GROUP_EXIST, ERR_AFFINITY_GROUP_NOT_FOUND,
        ERR_INVALID_KEY_FORMAT,
    },
};
use astersql_pd_keyutil::KeyRange;
use astersql_pd_schedule_labeler::{KEY_RANGE, KeyRangeRule, LabelRule, RegionLabel};
use std::collections::BTreeMap;

/// Prefix shared by affinity label creation and deletion.
pub const LABEL_RULE_ID_PREFIX: &str = "affinity_group/";
/// Region label identifying an affinity group.
pub const LABEL_KEY: &str = "affinity_group";
/// Owned canonical byte ranges associated with one group.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupKeyRanges {
    pub key_ranges: Vec<KeyRange>,
    pub group_id: String,
}
/// Returns the canonical label rule ID without validating the group ID.
pub fn get_label_rule_id(group_id: &str) -> String {
    format!("{LABEL_RULE_ID_PREFIX}{group_id}")
}
/// Requires the ID suffix to match the first affinity label.
pub fn parse_affinity_group_id_from_label_rule(rule: Option<&LabelRule>) -> Option<&str> {
    let rule = rule?;
    let id = rule.id.strip_prefix(LABEL_RULE_ID_PREFIX)?;
    (!id.is_empty()
        && rule
            .labels
            .iter()
            .find(|l| l.key == LABEL_KEY)
            .is_some_and(|l| l.value == id))
    .then_some(id)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|v| format!("{v:02x}")).collect()
}
/// Builds an unadjusted label rule with lowercase hexadecimal boundaries.
pub fn make_label_rule(ranges: &GroupKeyRanges) -> LabelRule {
    let mut rule = LabelRule::default();
    rule.id = get_label_rule_id(&ranges.group_id);
    let mut label = RegionLabel::default();
    label.key = LABEL_KEY.into();
    label.value = ranges.group_id.clone();
    rule.labels = vec![label];
    rule.rule_type = KEY_RANGE.into();
    rule.data = if ranges.key_ranges.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::Array(ranges.key_ranges.iter().map(|r| serde_json::json!({"start_key":hex(&r.start_key),"end_key":hex(&r.end_key)})).collect())
    };
    rule
}

/// Decodes hexadecimal boundaries, preserving the affinity error code.
pub fn decode_hex_key(value: &str, group_id: &str, key_type: &str) -> Result<Vec<u8>, TypedError> {
    let invalid = || ERR_INVALID_KEY_FORMAT.instantiate([key_type, value, group_id]);
    if !value.len().is_multiple_of(2) {
        return Err(invalid());
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|c| {
            let digit = |b: u8| match b {
                b'0'..=b'9' => Some(b - b'0'),
                b'a'..=b'f' => Some(b - b'a' + 10),
                b'A'..=b'F' => Some(b - b'A' + 10),
                _ => None,
            };
            Ok(digit(c[0]).ok_or_else(invalid)? * 16 + digit(c[1]).ok_or_else(invalid)?)
        })
        .collect()
}
/// Parses original hex fields in order, skipping absent entries.
pub fn parse_key_ranges_from_data(
    data: &[Option<&KeyRangeRule>],
    group_id: &str,
) -> Result<GroupKeyRanges, TypedError> {
    let key_ranges = data
        .iter()
        .flatten()
        .map(|r| {
            Ok(KeyRange::new(
                decode_hex_key(&r.start_key_hex, group_id, "start")?,
                decode_hex_key(&r.end_key_hex, group_id, "end")?,
            ))
        })
        .collect::<Result<_, TypedError>>()?;
    Ok(GroupKeyRanges {
        group_id: group_id.into(),
        key_ranges,
    })
}
/// Extracts ranges only from an adjusted affinity rule.
pub fn extract_key_ranges_from_label_rule(
    rule: Option<&LabelRule>,
) -> Result<GroupKeyRanges, TypedError> {
    let Some(rule) = rule else {
        return Ok(GroupKeyRanges::default());
    };
    if rule.data.is_null() {
        return Ok(GroupKeyRanges::default());
    }
    let Some(id) = parse_affinity_group_id_from_label_rule(Some(rule)) else {
        return Ok(GroupKeyRanges::default());
    };
    let data = rule.key_ranges().ok_or_else(|| {
        ERR_AFFINITY_GROUP_CONTENT.instantiate([format!(
            "invalid label rule data type for group {id}, got type {}",
            match &rule.data {
                serde_json::Value::Array(_) => "[]interface {}",
                serde_json::Value::Object(_) => "map[string]interface {}",
                serde_json::Value::String(_) => "string",
                serde_json::Value::Bool(_) => "bool",
                serde_json::Value::Number(_) => "float64",
                serde_json::Value::Null => "<nil>",
            }
        )])
    })?;
    parse_key_ranges_from_data(&data.iter().map(Some).collect::<Vec<_>>(), id)
}
/// Removes exact ranges; duplicate starts and missing ranges are errors.
pub fn apply_remove_ops(
    current: &GroupKeyRanges,
    removes: &GroupKeyRanges,
) -> Result<GroupKeyRanges, TypedError> {
    let mut set = BTreeMap::new();
    for r in &removes.key_ranges {
        if set.insert(&r.start_key, &r.end_key).is_some() {
            return Err(ERR_AFFINITY_GROUP_EXIST.instantiate([&removes.group_id]));
        }
    }
    let mut filtered = Vec::new();
    for r in &current.key_ranges {
        if let Some(end) = set.remove(&r.start_key) {
            if end != &r.end_key {
                return Err(ERR_AFFINITY_GROUP_NOT_FOUND.instantiate([&removes.group_id]));
            }
        } else {
            filtered.push(r.clone());
        }
    }
    if !set.is_empty() {
        return Err(ERR_AFFINITY_GROUP_NOT_FOUND.instantiate([&removes.group_id]));
    }
    Ok(GroupKeyRanges {
        key_ranges: filtered,
        group_id: current.group_id.clone(),
    })
}
/// Tests half-open overlap with empty boundaries denoting infinity.
pub fn check_key_ranges_overlap(a: &KeyRange, b: &KeyRange) -> bool {
    if a.start_key == b.start_key || a.end_key == b.end_key {
        return true;
    }
    !(!a.end_key.is_empty() && !b.start_key.is_empty() && a.end_key <= b.start_key
        || !b.end_key.is_empty() && !a.start_key.is_empty() && b.end_key <= a.start_key)
}
/// Sorts by start and reports the first adjacent conflict.
pub fn validate_no_overlap(ranges: &[GroupKeyRanges]) -> Result<(), TypedError> {
    let mut all = ranges
        .iter()
        .flat_map(|g| g.key_ranges.iter().map(move |r| (r, &g.group_id)))
        .collect::<Vec<_>>();
    all.sort_by(|a, b| a.0.start_key.cmp(&b.0.start_key));
    for pair in all.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if check_key_ranges_overlap(a.0, b.0) {
            return Err(ERR_AFFINITY_GROUP_CONTENT.instantiate([if a.1 == b.1 {
                format!("key ranges overlap within group {}", a.1)
            } else {
                format!("key range overlaps between groups: {} and {}", a.1, b.1)
            }]));
        }
    }
    Ok(())
}
