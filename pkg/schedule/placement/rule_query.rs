// Copyright 2026 AsterSQL.
//! Management views over the same immutable snapshots used by region fitting.
use crate::{GroupBundle, Rule, RuleGroup, RuleManager, compare_rule};
use astersql_pd_core::RegionInfo;
use std::sync::Arc;

impl RuleManager {
    /// Owned Go-style clones: runtime group priority is cleared before sorting.
    pub fn get_all_rules(&self) -> Vec<Rule> {
        let mut rules: Vec<_> = self.config.rules().map(|r| r.as_ref().clone()).collect();
        rules.sort_by(compare_rule);
        rules
    }
    pub fn get_rules_count(&self) -> usize {
        self.config.rules().count()
    }
    pub fn get_groups_count(&self) -> usize {
        self.config.groups().len()
    }
    pub fn may_restrict_store_load(&self, is_tikv: bool) -> bool {
        self.config.may_restrict_store_load(is_tikv)
    }
    pub fn get_rules_by_group(&self, group: &str) -> Vec<Rule> {
        self.get_all_rules()
            .into_iter()
            .filter(|r| r.group_id == group)
            .collect()
    }
    pub fn get_rules_by_key(&self, key: &[u8]) -> Vec<Rule> {
        self.list
            .as_ref()
            .map(|l| {
                l.get_rules_by_key(key)
                    .iter()
                    .map(|r| r.as_ref().clone())
                    .collect()
            })
            .unwrap_or_default()
    }
    /// Immutable shared rules, selected only for a range wholly within one segment.
    pub fn get_rules_for_apply_range(&self, start: &[u8], end: &[u8]) -> Vec<Arc<Rule>> {
        self.list
            .as_ref()
            .map(|l| l.get_rules_for_apply_range(start, end).to_vec())
            .unwrap_or_default()
    }
    pub fn get_rules_for_apply_region(&self, region: &RegionInfo) -> Vec<Arc<Rule>> {
        self.get_rules_for_apply_range(
            &region.meta().key_range.start_key,
            &region.meta().key_range.end_key,
        )
    }
    /// Split boundaries strictly inside (start, end); empty end means infinity.
    pub fn get_split_keys(&self, start: &[u8], end: &[u8]) -> Vec<Vec<u8>> {
        self.list
            .as_ref()
            .map(|l| l.get_split_keys(start, end))
            .unwrap_or_default()
    }
    /// Storage-key order, excluding implicit/default groups. Rules retain snapshot identity.
    pub fn get_rule_config_for_reconcile(&self) -> (Vec<Arc<Rule>>, Vec<RuleGroup>) {
        (
            self.config.rules().cloned().collect(),
            self.config
                .groups()
                .values()
                .filter(|g| !g.is_default())
                .cloned()
                .collect(),
        )
    }
    pub fn get_rule_group(&self, id: &str) -> Option<RuleGroup> {
        self.config.groups().get(id).cloned()
    }
    pub fn get_rule_groups(&self) -> Vec<RuleGroup> {
        let mut groups: Vec<_> = self.config.groups().values().cloned().collect();
        groups.sort_by(|a, b| (a.index, &a.id).cmp(&(b.index, &b.id)));
        groups
    }
    pub fn get_group_bundle(&self, id: &str) -> GroupBundle {
        let group = self.get_rule_group(id);
        GroupBundle {
            id: id.into(),
            index: group.as_ref().map_or(0, |g| g.index),
            override_: group.as_ref().is_some_and(|g| g.override_),
            rules: group.and_then(|_| {
                // Go bundles expose original rules, unlike GetRulesByGroup clones.
                let mut rules: Vec<_> = self
                    .config
                    .rules()
                    .filter(|r| r.group_id == id)
                    .map(|r| {
                        let mut snapshot = r.as_ref().clone();
                        snapshot.group = r.group.clone();
                        snapshot
                    })
                    .collect();
                rules.sort_by(compare_rule);
                (!rules.is_empty()).then(|| rules.into_iter().map(Some).collect())
            }),
        }
    }
    pub fn get_all_group_bundles(&self) -> Vec<GroupBundle> {
        self.get_rule_groups()
            .iter()
            .map(|g| self.get_group_bundle(&g.id))
            .collect()
    }
}
