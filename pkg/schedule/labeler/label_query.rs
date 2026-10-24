// Copyright 2026 AsterSQL.
use crate::{LabelRule, RegionLabel, RegionLabeler, Timestamp};
use astersql_pd_core::RegionInfo;
use std::collections::BTreeMap;
impl RegionLabeler {
    /// Resolve a key using the highest priority live rule at the supplied instant.
    pub fn get_region_label(&self, region: &RegionInfo, key: &str, now: Timestamp) -> String {
        let mut value = String::new();
        let mut index = -1;
        let range = &region.meta().key_range;
        for rule in self
            .index
            .get_range_rules(&range.start_key, &range.end_key)
            .unwrap_or_default()
        {
            if rule.index <= index && !value.is_empty() {
                continue;
            }
            for label in &rule.labels {
                if label.key == key && !label.expire_before(now) {
                    value = label.value.clone();
                    index = rule.index;
                }
            }
        }
        value
    }
    /// Resolve every key; returned labels contain only key/value, as in Go.
    pub fn get_region_labels(&self, region: &RegionInfo, now: Timestamp) -> Vec<RegionLabel> {
        let mut labels: BTreeMap<String, (String, i64)> = BTreeMap::new();
        let range = &region.meta().key_range;
        for rule in self
            .index
            .get_range_rules(&range.start_key, &range.end_key)
            .unwrap_or_default()
        {
            for label in &rule.labels {
                if !label.expire_before(now)
                    && labels.get(&label.key).is_none_or(|(_, i)| *i < rule.index)
                {
                    labels.insert(label.key.clone(), (label.value.clone(), rule.index));
                }
            }
        }
        labels
            .into_iter()
            .map(|(key, (value, _))| {
                let mut label = RegionLabel::default();
                label.key = key;
                label.value = value;
                label
            })
            .collect()
    }
    /// Whether the schedule option equals deny, ignoring ASCII case.
    pub fn schedule_disabled(&self, region: &RegionInfo, now: Timestamp) -> bool {
        self.get_region_label(region, "schedule", now)
            .eq_ignore_ascii_case("deny")
    }
    /// Return live snapshots in requested ID order, preserving duplicate IDs.
    pub fn get_label_rules(&self, ids: &[&str], now: Timestamp) -> Vec<LabelRule> {
        ids.iter()
            .filter_map(|id| self.get_label_rule(id, now))
            .collect()
    }
    /// Return a filtered snapshot without changing the authoritative index.
    pub fn get_label_rule(&self, id: &str, now: Timestamp) -> Option<LabelRule> {
        let mut rule = self.index.rule(id)?.clone();
        rule.check_and_remove_expire_labels(now);
        (!rule.labels.is_empty()).then_some(rule)
    }
    /// Return all live rules; order has no semantic meaning.
    pub fn get_all_label_rules(&self, now: Timestamp) -> Vec<LabelRule> {
        self.index
            .rules()
            .filter_map(|r| self.get_label_rule(&r.id, now))
            .collect()
    }
    /// Visit raw rules, including expired labels, until the visitor returns false.
    pub fn iterate_label_rules(&self, mut visitor: impl FnMut(&LabelRule) -> bool) {
        for rule in self.index.rules() {
            if !visitor(rule) {
                break;
            }
        }
    }
}
