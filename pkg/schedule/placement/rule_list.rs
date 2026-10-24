// Copyright 2026 AsterSQL.
use crate::{PeerRoleType, Rule};
use astersql_pd_schedule_rangelist::{Builder, List};
use std::{cmp::Ordering, fmt, sync::Arc};

/// Compare by group priority, group ID, rule priority, then rule ID.
pub fn compare_rule(a: &Rule, b: &Rule) -> Ordering {
    (a.group_index(), &a.group_id, a.index, &a.id).cmp(&(
        b.group_index(),
        &b.group_id,
        b.index,
        &b.id,
    ))
}
/// Select applicable rules from an already sorted list, retaining shared identity.
pub fn prepare_rules_for_apply(rules: &[Arc<Rule>]) -> Vec<Arc<Rule>> {
    let mut selected = Vec::new();
    let mut j = 0;
    for i in 1..rules.len() {
        if rules[j].group_id != rules[i].group_id {
            if rules[i].group.as_ref().is_some_and(|g| g.override_) {
                selected.clear();
            } else {
                selected.extend_from_slice(&rules[j..i]);
            }
            j = i;
        }
        if rules[i].override_ {
            j = i;
        }
    }
    selected.extend_from_slice(&rules[j..]);
    selected
}
/// Validate the selected raft replica roles, as Go checkApplyRules does.
pub fn check_apply_rules(rules: &[Arc<Rule>]) -> Result<(), &'static str> {
    let (mut leaders, mut voters) = (0i64, 0i64);
    for rule in rules {
        match rule.role {
            PeerRoleType::Leader => leaders = leaders.wrapping_add(rule.count),
            PeerRoleType::Voter => voters = voters.wrapping_add(rule.count),
            _ => {}
        }
        if leaders > 1 {
            return Err("multiple leader replicas");
        }
    }
    if leaders.wrapping_add(voters) < 1 {
        return Err("needs at least one leader or voter");
    }
    Ok(())
}
/// Build failure with the stable PD error code and range-specific reason.
#[derive(Debug, PartialEq, Eq)]
pub struct BuildRuleListError(pub String);
impl BuildRuleListError {
    pub fn code(&self) -> &'static str {
        "PD:placement:ErrBuildRuleList"
    }
}
impl fmt::Display for BuildRuleListError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}]build rule list failed, {}", self.code(), self.0)
    }
}
impl std::error::Error for BuildRuleListError {}
/// Sorted rule coverage and precomputed selections at every range boundary.
#[derive(Debug)]
pub struct RuleList {
    ranges: List<Arc<Rule>>,
    apply: Vec<Vec<Arc<Rule>>>,
}
impl RuleList {
    /// Build complete coverage; the input rules must already have decoded ranges.
    pub fn build(rules: impl IntoIterator<Item = Arc<Rule>>) -> Result<Self, BuildRuleListError> {
        let mut builder = Builder::new();
        builder.set_compare_func(|a: &Arc<Rule>, b: &Arc<Rule>| compare_rule(a, b));
        for rule in rules {
            builder.add_item(&rule.start_key, &rule.end_key, Arc::clone(&rule));
        }
        let ranges = builder.build();
        if ranges.is_empty() {
            return Err(BuildRuleListError("no rule left".into()));
        }
        let mut apply = Vec::with_capacity(ranges.len());
        for i in 0..ranges.len() {
            let (start, data) = ranges.get(i);
            let end = if i + 1 < ranges.len() {
                ranges.get(i + 1).0
            } else {
                &[]
            };
            let selected = prepare_rules_for_apply(data);
            let error = if data.is_empty() {
                Some("no rule")
            } else {
                check_apply_rules(&selected).err()
            };
            if let Some(error) = error {
                let hex = |key: &[u8]| key.iter().map(|b| format!("{b:02X}")).collect::<String>();
                return Err(BuildRuleListError(format!(
                    "{error} for range {{{}, {}}}",
                    hex(start),
                    hex(end)
                )));
            }
            apply.push(selected);
        }
        Ok(Self { ranges, apply })
    }
    /// All matching rules at a key, before override selection.
    pub fn get_rules_by_key(&self, key: &[u8]) -> &[Arc<Rule>] {
        self.ranges.get_data_by_key(key).1.unwrap_or(&[])
    }
    /// Selected rules only if the entire region lies within one segment.
    pub fn get_rules_for_apply_range(&self, start: &[u8], end: &[u8]) -> &[Arc<Rule>] {
        let (i, data) = self.ranges.get_data(start, end);
        if i < 0 || data.is_none_or(|v| v.is_empty()) {
            &[]
        } else {
            &self.apply[i as usize]
        }
    }
}
