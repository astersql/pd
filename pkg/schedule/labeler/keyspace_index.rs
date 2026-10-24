// Copyright 2026 AsterSQL.
use std::{collections::BTreeMap, sync::Arc};

use astersql_pd_codec::{decode_keyspace_key, encode_keyspace_boundary, keyspace_modes};

use crate::{KEY_RANGE, LabelRule};

/// Largest supported 24-bit keyspace identifier.
pub const MAX_KEYSPACE_ID: u32 = 0xff_ffff;
const BOUNDARY_COUNT: u32 = MAX_KEYSPACE_ID + 2;

/// Parses the canonical decimal suffix without signs or leading zeroes.
pub fn parse_keyspace_rule_id(text: &str) -> Option<u32> {
    let suffix = text.strip_prefix("keyspaces/")?;
    if suffix.is_empty()
        || !suffix.bytes().all(|b| b.is_ascii_digit())
        || (suffix.len() > 1 && suffix.starts_with('0'))
    {
        return None;
    }
    suffix
        .parse::<u32>()
        .ok()
        .filter(|id| *id <= MAX_KEYSPACE_ID)
}

fn canonical_ranges(rule: &LabelRule) -> Option<([(u8, u32); 2], usize)> {
    let id = parse_keyspace_rule_id(&rule.id)?;
    if rule.index != 0 || rule.rule_type != KEY_RANGE || rule.labels.len() != 1 {
        return None;
    }
    let label = &rule.labels[0];
    if label.key != "id"
        || label.value != rule.id.strip_prefix("keyspaces/")?
        || !label.ttl.is_empty()
        || !label.start_at.is_empty()
        || label.expires_at().is_some()
    {
        return None;
    }
    let ranges = rule.key_ranges()?;
    if ranges.is_empty() || ranges.len() > 2 {
        return None;
    }
    let mut result = [(0, 0); 2];
    for (position, range) in ranges.iter().enumerate() {
        let mode = keyspace_modes().into_iter().find(|mode| {
            range.start_key == encode_keyspace_boundary(*mode, id)
                && range.end_key == encode_keyspace_boundary(*mode, id + 1)
        })?;
        if result[..position].contains(&(mode, id)) {
            return None;
        }
        result[position] = (mode, id);
    }
    Some((result, ranges.len()))
}

struct Slot {
    rule: Arc<LabelRule>,
    start: [u8; 9],
    end: [u8; 9],
}
/// Sparse ordered slots with shared ownership of canonical rule snapshots.
/// Arc identity corresponds to Go pointer ownership; equal rule IDs alone do not.
#[derive(Default)]
pub struct KeyspaceRuleIndex {
    slots: BTreeMap<(u8, u32), Slot>,
}
impl KeyspaceRuleIndex {
    /// Reports whether either API mode contains a rule.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
    /// Adds all canonical slots atomically, rejecting any collision.
    pub fn add(&mut self, rule: Arc<LabelRule>) -> bool {
        let Some((ranges, count)) = canonical_ranges(&rule) else {
            return false;
        };
        let ranges = &ranges[..count];
        if ranges.iter().any(|r| self.slots.contains_key(r)) {
            return false;
        }
        for &(mode, id) in ranges {
            self.slots.insert(
                (mode, id),
                Slot {
                    rule: rule.clone(),
                    start: encode_keyspace_boundary(mode, id),
                    end: encode_keyspace_boundary(mode, id + 1),
                },
            );
        }
        true
    }
    /// Replaces only slots owned by old at the new canonical ID, atomically.
    pub fn replace(&mut self, old: &Arc<LabelRule>, rule: Arc<LabelRule>) -> bool {
        let Some((ranges, count)) = canonical_ranges(&rule) else {
            return false;
        };
        let ranges = &ranges[..count];
        let id = ranges[0].1;
        let owned = |s: &Slot| Arc::ptr_eq(&s.rule, old);
        if !keyspace_modes()
            .iter()
            .any(|m| self.slots.get(&(*m, id)).is_some_and(owned))
            || ranges
                .iter()
                .any(|r| self.slots.get(r).is_some_and(|s| !owned(s)))
        {
            return false;
        }
        for mode in keyspace_modes() {
            let wanted = ranges.contains(&(mode, id));
            if let Some(slot) = self.slots.get_mut(&(mode, id)) {
                if wanted {
                    slot.rule = rule.clone();
                } else if owned(slot) {
                    self.slots.remove(&(mode, id));
                }
            } else if wanted {
                self.slots.insert(
                    (mode, id),
                    Slot {
                        rule: rule.clone(),
                        start: encode_keyspace_boundary(mode, id),
                        end: encode_keyspace_boundary(mode, id + 1),
                    },
                );
            }
        }
        true
    }
    /// Removes owned slots by canonical ID even if the old rule was mutated.
    pub fn remove(&mut self, rule_id: &str, rule: &Arc<LabelRule>) -> bool {
        let Some(id) = parse_keyspace_rule_id(rule_id) else {
            return false;
        };
        let mut removed = false;
        for mode in keyspace_modes() {
            if self
                .slots
                .get(&(mode, id))
                .is_some_and(|s| Arc::ptr_eq(&s.rule, rule))
            {
                self.slots.remove(&(mode, id));
                removed = true;
            }
        }
        removed
    }
    /// Checks canonical shape and identity for every requested slot.
    pub fn contains(&self, rule: &Arc<LabelRule>) -> bool {
        canonical_ranges(rule).is_some_and(|(ranges, count)| {
            ranges[..count].iter().all(|r| {
                self.slots
                    .get(r)
                    .is_some_and(|s| Arc::ptr_eq(&s.rule, rule))
            })
        })
    }
    /// Returns the rule covering a whole interval, including incomplete prefixes.
    pub fn get_rule(&self, start: &[u8], end: &[u8]) -> Option<&LabelRule> {
        self.match_rule(start, end).0
    }
    /// Returns both a rule and whether the interval stays inside one keyspace.
    pub fn match_rule(&self, start: &[u8], end: &[u8]) -> (Option<&LabelRule>, bool) {
        if end.is_empty() {
            return (None, false);
        }
        let decoded = decode_keyspace_key(start).ok();
        let interval = decoded.or_else(|| {
            keyspace_modes().into_iter().find_map(|mode| {
                let upper = boundary_bound(mode, start, true);
                (upper > 0 && upper < BOUNDARY_COUNT).then(|| (mode, upper - 1))
            })
        });
        let Some((mode, id)) = interval else {
            return (None, false);
        };
        let same_prefix = decoded.is_some() && end.len() >= 4 && start[..4] == end[..4];
        if !same_prefix && end > encode_keyspace_boundary(mode, id + 1).as_slice() {
            return (None, false);
        }
        (self.slots.get(&(mode, id)).map(|s| s.rule.as_ref()), true)
    }
    fn boundaries<'a>(&'a self, start: &'a [u8], end: &'a [u8]) -> impl Iterator<Item = &'a [u8]> {
        keyspace_modes().into_iter().flat_map(move |mode| {
            let lo = boundary_bound(mode, start, true);
            let hi = if end.is_empty() {
                BOUNDARY_COUNT
            } else {
                boundary_bound(mode, end, false)
            };
            let first = lo.saturating_sub(1);
            let last = hi.max(first);
            self.slots
                .range((mode, first)..(mode, last))
                .flat_map(|(_, s)| [s.start.as_slice(), s.end.as_slice()])
                .filter(move |k| *k > start && (end.is_empty() || *k < end))
        })
    }
    /// Reports an indexed boundary strictly inside the query interval.
    pub fn has_split_key(&self, start: &[u8], end: &[u8]) -> bool {
        self.boundaries(start, end).next().is_some()
    }
    /// Returns sorted unique keyspace boundaries strictly inside the interval.
    pub fn get_split_keys<'a>(&'a self, start: &[u8], end: &[u8]) -> Vec<&'a [u8]> {
        // Collect slot references independently of the query's lifetime.
        let mut keys = Vec::new();
        for mode in keyspace_modes() {
            let lo = boundary_bound(mode, start, true).saturating_sub(1);
            let hi = if end.is_empty() {
                BOUNDARY_COUNT
            } else {
                boundary_bound(mode, end, false)
            };
            for (_, s) in self.slots.range((mode, lo)..(mode, hi.max(lo))) {
                for k in [s.start.as_slice(), s.end.as_slice()] {
                    if k > start && (end.is_empty() || k < end) && keys.last().copied() != Some(k) {
                        keys.push(k);
                    }
                }
            }
        }
        keys
    }
}
fn boundary_bound(mode: u8, key: &[u8], upper: bool) -> u32 {
    let first = encode_keyspace_boundary(mode, 0);
    if first.as_slice() > key || (first.as_slice() == key && !upper) {
        return 0;
    }
    let fence = encode_keyspace_boundary(mode, MAX_KEYSPACE_ID + 1);
    if fence.as_slice() < key || (fence.as_slice() == key && upper) {
        return BOUNDARY_COUNT;
    }
    if fence.as_slice() == key || key[0] != mode {
        return MAX_KEYSPACE_ID + 1;
    }
    let id = u32::from_be_bytes([
        0,
        *key.get(1).unwrap_or(&0),
        *key.get(2).unwrap_or(&0),
        *key.get(3).unwrap_or(&0),
    ]);
    let b = encode_keyspace_boundary(mode, id);
    if b.as_slice() > key || (b.as_slice() == key && !upper) {
        id
    } else {
        id + 1
    }
}

/// Merges sorted boundary lists, removing duplicates when both are populated.
pub fn merge_split_keys<'a>(left: Vec<&'a [u8]>, right: Vec<&'a [u8]>) -> Vec<&'a [u8]> {
    if left.is_empty() {
        return right;
    }
    if right.is_empty() {
        return left;
    }
    let mut merged = Vec::with_capacity(left.len() + right.len());
    let (mut l, mut r) = (left.into_iter().peekable(), right.into_iter().peekable());
    while l.peek().is_some() || r.peek().is_some() {
        let next = match (l.peek(), r.peek()) {
            (Some(a), Some(b)) if a <= b => l.next(),
            (Some(_), Some(_)) => r.next(),
            (Some(_), None) => l.next(),
            _ => r.next(),
        }
        .unwrap();
        if merged.last().copied() != Some(next) {
            merged.push(next);
        }
    }
    merged
}
