// Copyright 2026 AsterSQL.
use astersql_pd_codec::{
    ENCODED_KEYSPACE_PREFIX_LEN, RAW_KEYSPACE_MODE_PREFIX, TXN_KEYSPACE_MODE_PREFIX, decode_bytes,
    encode_keyspace_boundary, parse_keyspace_prefix,
};
use astersql_pd_keyspace_constant::{
    DEFAULT_KEYSPACE_ID, DEFAULT_KEYSPACE_NAME, MAX_VALID_KEYSPACE_ID, REGION_LABEL_ID_PREFIX,
    REGION_LABEL_KEY, SYSTEM_KEYSPACE_ID, SYSTEM_KEYSPACE_NAME,
};
use astersql_pd_schedule_labeler::{KEY_RANGE, LabelRule, RegionLabel, parse_keyspace_rule_id};
use serde_json::{Value, json};
/// Explicit kernel policy; no mutable process-wide mode enters domain validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kernel {
    Classic,
    NextGeneration,
}
impl Kernel {
    pub const fn current() -> Self {
        if astersql_pd_kerneltype::is_next_gen() {
            Self::NextGeneration
        } else {
            Self::Classic
        }
    }
    pub fn is_protected_id(self, id: u32) -> bool {
        id == match self {
            Self::Classic => DEFAULT_KEYSPACE_ID,
            Self::NextGeneration => SYSTEM_KEYSPACE_ID,
        }
    }
    pub fn is_protected_name(self, name: &str) -> bool {
        name == match self {
            Self::Classic => DEFAULT_KEYSPACE_NAME,
            Self::NextGeneration => SYSTEM_KEYSPACE_NAME,
        }
    }
    pub fn validate_id(self, id: u32) -> Result<(), ValidationError> {
        if id > MAX_VALID_KEYSPACE_ID {
            return Err(ValidationError(format!(
                "illegal keyspace id {id}, larger than spaceID Max {MAX_VALID_KEYSPACE_ID}"
            )));
        }
        if self.is_protected_id(id) {
            return Err(ValidationError(format!(
                "illegal keyspace id {id}, collides with a protected keyspace id"
            )));
        }
        Ok(())
    }
    pub fn validate_name(self, name: &str) -> Result<(), ValidationError> {
        if name.is_empty()
            || name.len() > 20
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(ValidationError(format!(
                "illegal keyspace name {name}, should contain only alphanumerical and underline"
            )));
        }
        if self.is_protected_name(name) {
            return Err(ValidationError(format!(
                "illegal keyspace name {name}, collides with a protected keyspace name"
            )));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationError(String);
impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ValidationError {}
pub const fn mask_keyspace_id(id: u32) -> u32 {
    id & 0xff
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundType {
    Raw,
    Txn,
}
impl BoundType {
    pub fn from_key_type(value: &str) -> Self {
        if value == "raw" { Self::Raw } else { Self::Txn }
    }
    pub fn bounds(self, b: &RegionBound) -> (&[u8], &[u8]) {
        match self {
            Self::Raw => (&b.raw_left_bound, &b.raw_right_bound),
            Self::Txn => (&b.txn_left_bound, &b.txn_right_bound),
        }
    }
}
impl std::fmt::Display for BoundType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Raw => "raw",
            Self::Txn => "txn",
        })
    }
}
/// Memcomparable boundaries; maximum ID's upper boundary advances the mode byte.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegionBound {
    pub raw_left_bound: [u8; ENCODED_KEYSPACE_PREFIX_LEN],
    pub raw_right_bound: [u8; ENCODED_KEYSPACE_PREFIX_LEN],
    pub txn_left_bound: [u8; ENCODED_KEYSPACE_PREFIX_LEN],
    pub txn_right_bound: [u8; ENCODED_KEYSPACE_PREFIX_LEN],
}
/// Encoding follows Go MakeRegionBound; ID admission is deliberately a separate operation.
pub fn make_region_bound(id: u32) -> RegionBound {
    RegionBound {
        raw_left_bound: encode_keyspace_boundary(RAW_KEYSPACE_MODE_PREFIX, id),
        raw_right_bound: encode_keyspace_boundary(RAW_KEYSPACE_MODE_PREFIX, id.wrapping_add(1)),
        txn_left_bound: encode_keyspace_boundary(TXN_KEYSPACE_MODE_PREFIX, id),
        txn_right_bound: encode_keyspace_boundary(TXN_KEYSPACE_MODE_PREFIX, id.wrapping_add(1)),
    }
}
pub fn make_key_ranges(id: u32, key_type: &str) -> Value {
    let b = make_region_bound(id);
    let (start, end) = BoundType::from_key_type(key_type).bounds(&b);
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    json!([{"start_key":hex(start),"end_key":hex(end)}])
}
pub fn build_label_rule(id: u32, bound: BoundType) -> LabelRule {
    let mut label = RegionLabel::default();
    label.key = REGION_LABEL_KEY.into();
    label.value = id.to_string();
    let mut rule = LabelRule::default();
    rule.id = format!("{REGION_LABEL_ID_PREFIX}{id}");
    rule.index = 0;
    rule.labels = vec![label];
    rule.rule_type = KEY_RANGE.into();
    rule.data = make_key_ranges(id, &bound.to_string());
    rule
}
pub fn make_txn_label_rule(id: u32) -> LabelRule {
    build_label_rule(id, BoundType::Txn)
}
/// Parse Go's looser identity contract, not the labeler's stricter fast-index shape.
pub fn parse_keyspace_id_from_label_rule(rule: Option<&LabelRule>) -> Option<u32> {
    let rule = rule?;
    let id = parse_keyspace_rule_id(&rule.id)?;
    let value = &rule
        .labels
        .iter()
        .find(|l| l.key == REGION_LABEL_KEY)?
        .value;
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    (value.parse::<u32>().ok()? == id).then_some(id)
}
/// Full memcomparable decoding validates padding; the first decoded prefix determines identity.
pub fn extract_keyspace_id(key: &[u8]) -> Option<(u32, BoundType)> {
    if key.is_empty() {
        return Some((MAX_VALID_KEYSPACE_ID, BoundType::Txn));
    }
    let (_, decoded) = decode_bytes(key).ok()?;
    let (mode, id) = parse_keyspace_prefix(&decoded).ok()?;
    Some((
        id,
        if mode == RAW_KEYSPACE_MODE_PREFIX {
            BoundType::Raw
        } else {
            BoundType::Txn
        },
    ))
}
