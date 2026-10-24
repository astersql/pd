// Copyright 2026 AsterSQL.
use std::{collections::BTreeMap, sync::Arc};

use astersql_pd_schedule_rangelist::{Builder, List};

use crate::{KeyspaceRuleIndex, LabelRule, Timestamp, merge_split_keys};

/// Authoritative rule snapshot partitioned into generic and sparse keyspace views.
/// Callers validate rules before insertion; mutations publish before returning.
pub struct LabelRuleIndex {
    rules: BTreeMap<String, Arc<LabelRule>>,
    keyspaces: KeyspaceRuleIndex,
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
            rules: rules
                .into_iter()
                .map(|r| (r.id.clone(), Arc::new(r)))
                .collect(),
            keyspaces: KeyspaceRuleIndex::default(),
            ranges: Builder::new().build(),
            min_expire: None,
        };
        for rule in index.rules.values() {
            index.keyspaces.add(rule.clone());
        }
        index.build_ranges();
        index
    }
    /// Returns the owned rule without exposing mutable derived range state.
    pub fn rule(&self, id: &str) -> Option<&LabelRule> {
        self.rules.get(id).map(Arc::as_ref)
    }
    /// Iterates authoritative rules; order has no priority meaning.
    pub fn rules(&self) -> impl Iterator<Item = &LabelRule> {
        self.rules.values().map(Arc::as_ref)
    }
    /// Inserts or replaces a validated rule and publishes its new boundaries.
    pub fn set(&mut self, rule: LabelRule) {
        let rule = Arc::new(rule);
        let mut generic_changed = false;
        if let Some(old) = self.rules.get(&rule.id) {
            if self.keyspaces.replace(old, rule.clone()) {
                self.rules.insert(rule.id.clone(), rule);
                return;
            }
            generic_changed = !self.keyspaces.remove(&rule.id, old);
        }
        let canonical = self.keyspaces.add(rule.clone());
        self.rules.insert(rule.id.clone(), rule);
        if !canonical || generic_changed {
            self.build_ranges();
        }
    }
    /// Removes a rule; an absent ID is a no-op.
    pub fn delete(&mut self, id: &str) {
        if let Some(rule) = self.rules.remove(id) {
            if !self.keyspaces.remove(id, &rule) {
                self.build_ranges();
            }
        }
    }
    /// Removes expired labels from one rule and refreshes derived expiry state.
    /// An empty rule remains owned until the caller deletes it.
    pub fn remove_expired_labels(&mut self, id: &str, now: Timestamp) -> bool {
        let changed = self.rules.get_mut(id).is_some_and(|rule| {
            rule.expire_before(now) && Arc::make_mut(rule).check_and_remove_expire_labels(now)
        });
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
        merge_split_keys(
            self.ranges.get_split_keys(start, end),
            self.keyspaces.get_split_keys(start, end),
        )
    }
    /// Returns all active rules for a single segment, including duplicate
    /// occurrences from overlapping ranges of one rule. None means an interior
    /// boundary was crossed; Some(empty) means a valid range with no rule.
    /// Rule order is unspecified, and larger rule.index has higher priority.
    pub fn get_range_rules(&self, start: &[u8], end: &[u8]) -> Option<Vec<&LabelRule>> {
        let (data, within) = self.ranges.get_data_by_range(start, end);
        if !within {
            return None;
        }
        let (keyspace_rule, within_keyspace) = self.keyspaces.match_rule(start, end);
        if !within_keyspace && self.keyspaces.has_split_key(start, end) {
            return None;
        }
        let mut rules: Vec<_> = data
            .unwrap_or_default()
            .iter()
            .filter_map(|id| self.rules.get(id).map(Arc::as_ref))
            .collect();
        rules.extend(keyspace_rule);
        Some(rules)
    }

    fn build_ranges(&mut self) {
        let mut builder = Builder::new();
        self.min_expire = self.rules.values().filter_map(|r| r.min_expire()).min();
        for rule in self.rules.values() {
            if self.keyspaces.contains(rule) {
                continue;
            }
            for range in rule.key_ranges().unwrap_or_default() {
                builder.add_item(&range.start_key, &range.end_key, rule.id.clone());
            }
        }
        self.ranges = builder.build();
    }
}
