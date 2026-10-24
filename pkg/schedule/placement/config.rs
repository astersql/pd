// Copyright 2026 AsterSQL.
//! Owned configuration snapshots and staged rule mutations. No persistence or clock.
use crate::{Rule, RuleGroup};
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::Arc;

/// In-memory placement configuration. Shared rules are immutable snapshots.
#[derive(Clone, Debug, Default)]
pub struct RuleConfig {
    rules: BTreeMap<[String; 2], Arc<Rule>>,
    groups: BTreeMap<String, RuleGroup>,
    may_restrict_store_load: [bool; 2],
}
impl RuleConfig {
    pub fn rules(&self) -> impl Iterator<Item = &Arc<Rule>> {
        self.rules.values()
    }
    pub fn groups(&self) -> &BTreeMap<String, RuleGroup> {
        &self.groups
    }
    pub fn get_rule(&self, group: &str, id: &str) -> Option<&Arc<Rule>> {
        self.rules.get(&[group.into(), id.into()])
    }
    pub fn get_group(&self, id: &str) -> RuleGroup {
        self.groups.get(id).cloned().unwrap_or_else(|| RuleGroup {
            id: id.into(),
            ..Default::default()
        })
    }
    pub fn set_rule(&mut self, rule: Rule) {
        self.rules.insert(rule.key(), Arc::new(rule));
    }
    pub fn set_group(&mut self, group: RuleGroup) {
        self.groups.insert(group.id.clone(), group);
    }
    pub fn may_restrict_store_load(&self, is_tikv: bool) -> bool {
        self.may_restrict_store_load[usize::from(!is_tikv)]
    }
    pub fn adjust(&mut self) {
        self.groups.retain(|_, g| !g.is_default());
        for rule in self.rules.values_mut() {
            let group = self
                .groups
                .entry(rule.group_id.clone())
                .or_insert_with(|| RuleGroup {
                    id: rule.group_id.clone(),
                    ..Default::default()
                });
            let rule = Arc::make_mut(rule);
            rule.group = Some(group.clone());
        }
        self.refresh_engine_summary();
    }
    fn refresh_engine_summary(&mut self) {
        self.may_restrict_store_load = [false; 2];
        for rule in self
            .rules
            .values()
            .filter(|r| !r.label_constraints.is_empty())
        {
            self.may_restrict_store_load[0] |= rule_may_restrict_engine(rule, &["", "tikv"]);
            self.may_restrict_store_load[1] |= rule_may_restrict_engine(rule, &["tiflash"]);
        }
    }
    pub fn begin_patch(&self) -> RuleConfigPatch {
        RuleConfigPatch {
            original: self.clone(),
            rules: BTreeMap::new(),
            groups: BTreeMap::new(),
            adjusted: None,
        }
    }
}
/// Whether constraints restrict any of the engine representations supplied.
pub fn rule_may_restrict_engine(rule: &Rule, values: &[&str]) -> bool {
    let other = rule.label_constraints.iter().any(|c| c.key != "engine");
    let engine: Vec<_> = rule
        .label_constraints
        .iter()
        .filter(|c| c.key == "engine")
        .collect();
    let matched = values
        .iter()
        .filter(|v| (v.is_empty() || !engine.is_empty()) && engine.iter().all(|c| c.match_value(v)))
        .count();
    matched > 0 && (other || matched < values.len())
}
fn json_equals<T: Serialize>(a: &T, b: &T) -> bool {
    serde_json::to_vec(a).expect("configuration serializes")
        == serde_json::to_vec(b).expect("configuration serializes")
}
/// A patch owns its base snapshot; adjust and commit never mutate previous readers.
#[derive(Debug)]
pub struct RuleConfigPatch {
    original: RuleConfig,
    rules: BTreeMap<[String; 2], Option<Arc<Rule>>>,
    groups: BTreeMap<String, RuleGroup>,
    adjusted: Option<RuleConfig>,
}
impl RuleConfigPatch {
    pub fn set_rule(&mut self, rule: Rule) {
        self.adjusted = None;
        self.rules.insert(rule.key(), Some(Arc::new(rule)));
    }
    pub fn delete_rule(&mut self, group: &str, id: &str) {
        self.adjusted = None;
        self.rules.insert([group.into(), id.into()], None);
    }
    pub fn set_group(&mut self, group: RuleGroup) {
        self.adjusted = None;
        self.groups.insert(group.id.clone(), group);
    }
    pub fn delete_group(&mut self, id: &str) {
        self.set_group(RuleGroup {
            id: id.into(),
            ..Default::default()
        });
    }
    pub fn get_group(&self, id: &str) -> RuleGroup {
        self.groups
            .get(id)
            .cloned()
            .unwrap_or_else(|| self.original.get_group(id))
    }
    pub fn mutation_counts(&self) -> (usize, usize) {
        (self.rules.len(), self.groups.len())
    }
    pub(crate) fn rule_mutations(
        &self,
    ) -> impl Iterator<Item = (&[String; 2], &Option<Arc<Rule>>)> {
        self.rules.iter()
    }
    pub(crate) fn group_mutations(&self) -> impl Iterator<Item = (&String, &RuleGroup)> {
        self.groups.iter()
    }
    fn merged(&self) -> RuleConfig {
        let mut result = self.original.clone();
        for (key, rule) in &self.rules {
            if let Some(rule) = rule {
                result.rules.insert(key.clone(), rule.clone());
            } else {
                result.rules.remove(key);
            }
        }
        result.groups.extend(self.groups.clone());
        result
    }
    /// Prepare a view for rule-list validation, including explicit default groups.
    pub fn adjust(&mut self) -> &RuleConfig {
        let mut result = self.merged();
        for rule in result.rules.values_mut() {
            let group = self.get_group(&rule.group_id);
            Arc::make_mut(rule).group = Some(group);
        }
        result.refresh_engine_summary();
        self.adjusted = Some(result);
        self.adjusted.as_ref().expect("adjusted view")
    }
    /// Remove mutations equal in serialized JSON, ignoring decoded keys/runtime groups.
    pub fn trim(&mut self) {
        self.rules.retain(|key, rule| {
            !json_equals(
                &rule.as_deref(),
                &self.original.rules.get(key).map(AsRef::as_ref),
            )
        });
        self.groups
            .retain(|id, group| !json_equals(group, &self.original.get_group(id)));
        self.adjusted = None;
    }
    /// Commit using caller-supplied Unix seconds. Existing creation times survive updates.
    pub fn commit(self, now_unix_seconds: u64) -> RuleConfig {
        let mut result = self.merged();
        for (key, rule) in self.rules {
            if let Some(rule) = rule {
                let mut rule = rule.as_ref().clone();
                if let Some(old) = self.original.rules.get(&key) {
                    rule.version = old.version.wrapping_add(1);
                    rule.create_timestamp = old.create_timestamp;
                } else {
                    rule.version = 0;
                    rule.create_timestamp = now_unix_seconds;
                }
                result.rules.insert(key, Arc::new(rule));
            }
        }
        result.adjust();
        result
    }
}
