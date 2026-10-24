// Copyright 2026 AsterSQL.
use std::collections::BTreeMap;

use astersql_pd_schedule_rangelist::{Builder, List};

use crate::{LabelRule, Timestamp};

/// Owned generic rule snapshot. Callers validate rules before inserting them.
/// Mutations publish a complete range view before returning. Keyspace acceleration
/// is intentionally left to the keyspace index, without changing generic semantics.
pub struct LabelRuleIndex {
    rules: BTreeMap<String, LabelRule>,
    ranges: List<String>,
    min_expire: Option<Timestamp>,
}
impl Default for LabelRuleIndex {
    fn default() -> Self {
        Self::new(std::iter::empty())
    }
}
impl LabelRuleIndex {
    /// Builds an authoritative snapshot; later entries replace duplicate IDs.
    pub fn new(rules: impl IntoIterator<Item = LabelRule>) -> Self {
        let mut index = Self {
            rules: rules.into_iter().map(|r| (r.id.clone(), r)).collect(),
            ranges: Builder::new().build(),
            min_expire: None,
        };
        index.build_ranges();
        index
    }
    /// Returns the owned rule without exposing mutable derived range state.
    pub fn rule(&self, id: &str) -> Option<&LabelRule> {
        self.rules.get(id)
    }
    /// Iterates authoritative rules; order has no priority meaning.
    pub fn rules(&self) -> impl Iterator<Item = &LabelRule> {
        self.rules.values()
    }
    /// Inserts or replaces a validated rule and publishes its new boundaries.
    pub fn set(&mut self, rule: LabelRule) {
        self.rules.insert(rule.id.clone(), rule);
        self.build_ranges();
    }
    /// Removes a rule; an absent ID is a no-op.
    pub fn delete(&mut self, id: &str) {
        if self.rules.remove(id).is_some() {
            self.build_ranges();
        }
    }
    /// Removes expired labels from one rule and refreshes derived expiry state.
    /// An empty rule remains owned until the caller deletes it.
    pub fn remove_expired_labels(&mut self, id: &str, now: Timestamp) -> bool {
        let changed = self
            .rules
            .get_mut(id)
            .is_some_and(|rule| rule.check_and_remove_expire_labels(now));
        if changed {
            self.build_ranges();
        }
        changed
    }
    /// Earliest expiring label in the snapshot, absent for permanent labels.
    pub fn min_expire(&self) -> Option<Timestamp> {
        self.min_expire
    }
    /// Returns sorted, unique boundaries strictly inside (start, end).
    /// Empty end denotes infinity.
    pub fn get_split_keys(&self, start: &[u8], end: &[u8]) -> Vec<&[u8]> {
        self.ranges.get_split_keys(start, end)
    }
    /// Returns all active rules for a single segment, including duplicate
    /// occurrences from overlapping ranges of one rule. None means an interior
    /// boundary was crossed; Some(empty) means a valid range with no rule.
    /// Rule order is unspecified, and larger rule.index has higher priority.
    pub fn get_range_rules(&self, start: &[u8], end: &[u8]) -> Option<Vec<&LabelRule>> {
        let (data, within) = self.ranges.get_data_by_range(start, end);
        within.then(|| {
            data.unwrap_or_default()
                .iter()
                .filter_map(|id| self.rules.get(id))
                .collect()
        })
    }
    fn build_ranges(&mut self) {
        let mut builder = Builder::new();
        self.min_expire = self.rules.values().filter_map(LabelRule::min_expire).min();
        for rule in self.rules.values() {
            for range in rule.key_ranges().unwrap_or_default() {
                builder.add_item(&range.start_key, &range.end_key, rule.id.clone());
            }
        }
        self.ranges = builder.build();
    }
}
