// Copyright 2026 AsterSQL.

use astersql_pd_errs::{
    TypedError,
    catalog::{ERR_HEX_DECODING_STRING, ERR_REGION_RULE_CONTENT},
};
use astersql_pd_typeutil::Duration;
use chrono::{Datelike, NaiveDate, Timelike};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{error::Error, fmt};

/// The supported region label range rule type.
pub const KEY_RANGE: &str = "key-range";

/// A deterministic UTC instant in signed Unix nanoseconds.
pub type Timestamp = i128;

/// JSON decoding errors remain distinct from PD content and hex errors.
#[derive(Debug)]
pub enum RuleError {
    Json(serde_json::Error),
    Domain(TypedError),
}
impl fmt::Display for RuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(e) => e.fmt(f),
            Self::Domain(e) => e.fmt(f),
        }
    }
}
impl Error for RuleError {}
impl RuleError {
    /// Returns the stable PD code, or None for JSON syntax/type errors.
    pub fn code(&self) -> Option<&'static str> {
        match self {
            Self::Json(_) => None,
            Self::Domain(e) => Some(e.code()),
        }
    }
}
fn content(message: impl Into<String>) -> RuleError {
    RuleError::Domain(ERR_REGION_RULE_CONTENT.instantiate([message.into()]))
}

/// A region label with an optional TTL; derived expiry is never serialized.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct RegionLabel {
    pub key: String,
    pub value: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub ttl: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub start_at: String,
    #[serde(skip)]
    expire: Option<Timestamp>,
}
impl RegionLabel {
    /// Returns the derived expiry, absent when TTL is empty.
    pub fn expires_at(&self) -> Option<Timestamp> {
        self.expire
    }
    /// Equality with the expiration instant is still live, as in Go Before.
    pub fn expire_before(&self, now: Timestamp) -> bool {
        self.expire.is_some_and(|e| e < now)
    }
    /// Derives expiry, using now only when start_at is absent.
    pub fn check_and_adjust_expire(&mut self, now: Timestamp) -> Result<(), RuleError> {
        if self.ttl.is_empty() {
            self.expire = None;
            return Ok(());
        }
        let ttl: Duration = self
            .ttl
            .parse()
            .map_err(|_| content("region label with invalid ttl info: invalid duration"))?;
        let start = if self.start_at.is_empty() {
            self.start_at = format_unix_date(now)?;
            now
        } else {
            parse_unix_date(&self.start_at)
                .ok_or_else(|| content("region label with invalid ttl info: invalid start_at"))?
        };
        self.expire = Some(start + i128::from(ttl.as_nanos()));
        Ok(())
    }
}
impl fmt::Display for RegionLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "key: {}, value: {}", self.key, self.value)
    }
}

/// Decoded byte boundaries alongside their original hexadecimal spelling.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct KeyRangeRule {
    #[serde(skip)]
    pub start_key: Vec<u8>,
    #[serde(rename = "start_key")]
    pub start_key_hex: String,
    #[serde(skip)]
    pub end_key: Vec<u8>,
    #[serde(rename = "end_key")]
    pub end_key_hex: String,
}

/// Rule JSON and its validated derived range/expiry state.
#[derive(Clone, Debug, Default, Serialize)]
pub struct LabelRule {
    pub id: String,
    pub index: i64,
    pub labels: Vec<RegionLabel>,
    pub rule_type: String,
    pub data: Value,
    #[serde(skip)]
    ranges: Option<Vec<KeyRangeRule>>,
    #[serde(skip)]
    min_expire: Option<Timestamp>,
}
impl LabelRule {
    /// Decodes without semantic checks or derived state, for restoration callers.
    pub fn from_json_without_check(data: &[u8]) -> Result<Self, RuleError> {
        serde_json::from_slice(data).map_err(RuleError::Json)
    }
    /// Decodes and validates, removing labels strictly older than now.
    pub fn from_json(data: &[u8], now: Timestamp) -> Result<Self, RuleError> {
        let mut rule = Self::from_json_without_check(data)?;
        rule.check_and_adjust(now)?;
        Ok(rule)
    }
    /// Returns validated ranges only for the supported rule type.
    pub fn key_ranges(&self) -> Option<&[KeyRangeRule]> {
        if self.rule_type == KEY_RANGE {
            self.ranges.as_deref()
        } else {
            None
        }
    }
    /// Returns the minimum expiry among remaining labels.
    pub fn min_expire(&self) -> Option<Timestamp> {
        self.min_expire
    }
    /// Reports whether any expiring label is strictly older than now.
    pub fn expire_before(&self, now: Timestamp) -> bool {
        self.min_expire.is_some_and(|e| e < now)
    }
    /// Removes expired labels, preserving order, and recomputes earliest expiry.
    pub fn check_and_remove_expire_labels(&mut self, now: Timestamp) -> bool {
        let before = self.labels.len();
        self.labels.retain(|l| !l.expire_before(now));
        self.min_expire = self.labels.iter().filter_map(|l| l.expire).min();
        before != self.labels.len()
    }
    /// Applies Go validation order and refreshes derived fields.
    pub fn check_and_adjust(&mut self, now: Timestamp) -> Result<(), RuleError> {
        if self.id.is_empty() {
            return Err(content("empty rule id"));
        }
        if self.labels.is_empty() {
            return Err(content("no region labels"));
        }
        for l in &mut self.labels {
            if l.key.is_empty() {
                return Err(content("empty region label key"));
            }
            if l.value.is_empty() {
                return Err(content("empty region label value"));
            }
            l.check_and_adjust_expire(now)?;
        }
        self.check_and_remove_expire_labels(now);
        if self.labels.is_empty() {
            return Err(content("region label with expired ttl"));
        }
        if self.rule_type != KEY_RANGE {
            return Err(content(format!("invalid rule type: {}", self.rule_type)));
        }
        let data = self
            .data
            .as_array()
            .ok_or_else(|| content("invalid rule type"))?;
        if data.is_empty() {
            return Err(content("no key ranges"));
        }
        let mut ranges = Vec::with_capacity(data.len());
        for value in data {
            let object = value
                .as_object()
                .ok_or_else(|| content("invalid rule type"))?;
            let start = object
                .get("start_key")
                .and_then(Value::as_str)
                .ok_or_else(|| content("invalid startKey type"))?;
            let end = object
                .get("end_key")
                .and_then(Value::as_str)
                .ok_or_else(|| content("invalid endKey type"))?;
            let start_key = decode_hex(start)?;
            let end_key = decode_hex(end)?;
            if !end_key.is_empty() && end_key <= start_key {
                return Err(content("endKey should be greater than startKey"));
            }
            ranges.push(KeyRangeRule {
                start_key,
                start_key_hex: start.into(),
                end_key,
                end_key_hex: end.into(),
            });
        }
        self.ranges = Some(ranges);
        Ok(())
    }
}
fn decode_hex(text: &str) -> Result<Vec<u8>, RuleError> {
    let invalid = || RuleError::Domain(ERR_HEX_DECODING_STRING.instantiate([text]));
    if text.len() % 2 != 0 {
        return Err(invalid());
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|c| {
            let digit = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
            Ok(digit(c[0]).ok_or_else(invalid)? * 16 + digit(c[1]).ok_or_else(invalid)?)
        })
        .collect()
}

fn parse_unix_date(text: &str) -> Option<Timestamp> {
    let p: Vec<_> = text.split_whitespace().collect();
    if p.len() != 6 || !["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].contains(&p[0]) {
        return None;
    }
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .iter()
    .position(|m| *m == p[1])?
        + 1;
    let clock: Vec<_> = p[3].split(':').collect();
    if clock.len() != 3 {
        return None;
    }
    let (sec, frac) = clock[2].split_once(['.', ',']).unwrap_or((clock[2], ""));
    if !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if p[5].len() != 4
        || !p[5].bytes().all(|c| c.is_ascii_digit())
        || p[2].len() > 2
        || clock[0].len() > 2
        || clock[1].len() != 2
        || sec.len() != 2
        || (clock[2].contains(['.', ',']) && frac.is_empty())
    {
        return None;
    }
    let ns = format!("{frac:0<9}");
    let date = NaiveDate::from_ymd_opt(p[5].parse().ok()?, month as u32, p[2].parse().ok()?)?;
    let time = date.and_hms_nano_opt(
        clock[0].parse().ok()?,
        clock[1].parse().ok()?,
        sec.parse().ok()?,
        ns[..9].parse().ok()?,
    )?;
    // Go time.Parse fabricates unknown abbreviations with offset zero.
    let zone = p[4];
    let signed_hours = |text: &str| -> Option<i64> {
        if !text.starts_with(['+', '-'])
            || text.len() < 2
            || !text[1..].bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let hours: i64 = text.parse().ok()?;
        (hours.abs() <= 23).then_some(hours)
    };
    let offset = if let Some(suffix) = zone.strip_prefix("GMT").filter(|s| !s.is_empty()) {
        signed_hours(suffix)? * 3600
    } else {
        let upper = zone.bytes().all(|c| c.is_ascii_uppercase());
        let valid = zone == "ChST"
            || zone == "MeST"
            || zone == "WITA"
            || (upper
                && (zone.len() == 3
                    || ((zone.len() == 4 || zone.len() == 5) && zone.ends_with('T'))))
            || signed_hours(zone).is_some();
        if !valid {
            return None;
        }
        0
    };
    Some(
        (i128::from(time.and_utc().timestamp()) - i128::from(offset)) * 1_000_000_000
            + i128::from(time.nanosecond()),
    )
}
fn format_unix_date(now: Timestamp) -> Result<String, RuleError> {
    let seconds =
        i64::try_from(now.div_euclid(1_000_000_000)).map_err(|_| content("time out of range"))?;
    let time =
        chrono::DateTime::from_timestamp(seconds, 0).ok_or_else(|| content("time out of range"))?;
    let weekday = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
        [time.weekday().num_days_from_monday() as usize];
    Ok(format!(
        "{} {} {:2} {:02}:{:02}:{:02} UTC {:04}",
        weekday,
        time.format("%b"),
        time.day(),
        time.hour(),
        time.minute(),
        time.second(),
        time.year()
    ))
}

// Map visitors preserve duplicate field order, including inside label arrays.
impl<'de> Deserialize<'de> for RegionLabel {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = RegionLabel;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a label object or null")
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(RegionLabel::default())
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Self::Value, M::Error> {
                let mut label = RegionLabel::default();
                while let Some(key) = map.next_key::<String>()? {
                    let target = match key.to_ascii_lowercase().as_str() {
                        "key" => &mut label.key,
                        "value" => &mut label.value,
                        "ttl" => &mut label.ttl,
                        "start_at" => &mut label.start_at,
                        _ => {
                            map.next_value::<serde::de::IgnoredAny>()?;
                            continue;
                        }
                    };
                    if let Some(value) = map.next_value::<Option<String>>()? {
                        *target = value;
                    }
                }
                Ok(label)
            }
        }
        d.deserialize_any(Visitor)
    }
}
impl<'de> Deserialize<'de> for LabelRule {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = LabelRule;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a rule object or null")
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(LabelRule::default())
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Self::Value, M::Error> {
                let mut rule = LabelRule::default();
                while let Some(key) = map.next_key::<String>()? {
                    match key.to_ascii_lowercase().as_str() {
                        "id" => {
                            if let Some(v) = map.next_value::<Option<String>>()? {
                                rule.id = v;
                            }
                        }
                        "index" => {
                            if let Some(v) = map.next_value::<Option<i64>>()? {
                                rule.index = v;
                            }
                        }
                        "labels" => {
                            rule.labels = map
                                .next_value::<Option<Vec<RegionLabel>>>()?
                                .unwrap_or_default()
                        }
                        "rule_type" => {
                            if let Some(v) = map.next_value::<Option<String>>()? {
                                rule.rule_type = v;
                            }
                        }
                        "data" => rule.data = map.next_value()?,
                        _ => {
                            map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(rule)
            }
        }
        d.deserialize_any(Visitor)
    }
}
