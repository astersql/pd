// Copyright 2026 AsterSQL.
//! Staged bulk mutations, persisted in endpoint-sized transactions before publication.
use crate::{
    GroupBundle, Rule, RuleConfigPatch, RuleGroup, RuleList, RuleManager, RuleManagerError,
};
use astersql_pd_core::StoreInfo;
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_storage_endpoint::EndpointError;

/// Unknown actions are ignored, matching the Go switch's default behavior.
#[derive(Clone, Debug)]
pub struct RuleOp {
    pub rule: Rule,
    pub action: String,
    pub delete_by_id_prefix: bool,
}
impl RuleManager {
    pub fn begin_patch(&self) -> RuleConfigPatch {
        self.config.begin_patch()
    }
    /// Callers must serialize begin/commit with other mutations via their manager owner.
    pub async fn try_commit_patch(
        &mut self,
        mut patch: RuleConfigPatch,
        now: u64,
    ) -> Result<(), RuleManagerError> {
        RuleList::build(patch.adjust().rules().cloned())?;
        patch.trim();
        // Rules precede groups; Go map iteration order within each kind is unspecified.
        let rules: Vec<_> = patch.rule_mutations().collect();
        let groups: Vec<_> = patch.group_mutations().collect();
        let len = rules.len() + groups.len();
        for start in (0..len).step_by(120) {
            self.storage
                .run_in_txn(async |txn| {
                    for i in start..(start + 120).min(len) {
                        if i < rules.len() {
                            let (key, rule) = rules[i];
                            if let Some(rule) = rule {
                                self.storage.save_rule(
                                    txn,
                                    &self.paths,
                                    &rule.store_key(),
                                    rule.as_ref(),
                                )?;
                            } else {
                                let r = Rule {
                                    group_id: key[0].clone(),
                                    id: key[1].clone(),
                                    ..Default::default()
                                };
                                self.storage.delete_rule(txn, &self.paths, &r.store_key())?;
                            }
                        } else {
                            let (id, group) = groups[i - rules.len()];
                            if group.is_default() {
                                self.storage.delete_rule_group(txn, &self.paths, id)?;
                            } else {
                                self.storage.save_rule_group(txn, &self.paths, id, group)?;
                            }
                        }
                    }
                    Ok::<_, EndpointError>(())
                })
                .await?;
        }
        self.config = patch.commit(now);
        self.list =
            Some(RuleList::build(self.config.rules().cloned()).expect("validated coverage"));
        Ok(())
    }
    pub async fn set_rules<C: SharedConfigProvider + ?Sized>(
        &mut self,
        rules: Vec<Rule>,
        now: u64,
        conf: &C,
        stores: &[StoreInfo],
    ) -> Result<(), RuleManagerError> {
        let mut patch = self.begin_patch();
        for mut rule in rules {
            self.adjust_rule(&mut rule, conf, stores)?;
            patch.set_rule(rule);
        }
        self.try_commit_patch(patch, now).await
    }
    pub async fn batch<C: SharedConfigProvider + ?Sized>(
        &mut self,
        mut todo: Vec<RuleOp>,
        now: u64,
        conf: &C,
        stores: &[StoreInfo],
    ) -> Result<(), RuleManagerError> {
        for op in &mut todo {
            if op.action == "add" {
                self.adjust_rule(&mut op.rule, conf, stores)?;
            }
        }
        let mut patch = self.begin_patch();
        for op in todo {
            match op.action.as_str() {
                "add" => patch.set_rule(op.rule),
                "del" if op.delete_by_id_prefix => {
                    // Prefix selection is from the original config, never earlier additions.
                    for r in self.config.rules() {
                        if r.group_id == op.rule.group_id && r.id.starts_with(&op.rule.id) {
                            patch.delete_rule(&r.group_id, &r.id);
                        }
                    }
                }
                "del" => patch.delete_rule(&op.rule.group_id, &op.rule.id),
                _ => {}
            }
        }
        self.try_commit_patch(patch, now).await
    }
    pub async fn set_rule_group(
        &mut self,
        group: RuleGroup,
        now: u64,
    ) -> Result<(), RuleManagerError> {
        let mut p = self.begin_patch();
        p.set_group(group);
        self.try_commit_patch(p, now).await
    }
    pub async fn delete_rule_group(&mut self, id: &str, now: u64) -> Result<(), RuleManagerError> {
        let mut p = self.begin_patch();
        p.delete_group(id);
        self.try_commit_patch(p, now).await
    }
    fn bundle_patch<C: SharedConfigProvider + ?Sized>(
        &self,
        p: &mut RuleConfigPatch,
        bundle: GroupBundle,
        conf: &C,
        stores: &[StoreInfo],
    ) -> Result<(), RuleManagerError> {
        p.set_group(RuleGroup {
            id: bundle.id.clone(),
            index: bundle.index,
            override_: bundle.override_,
        });
        for rule in bundle.rules.unwrap_or_default() {
            let mut rule = rule.ok_or_else(|| RuleManagerError::Content("null rule".into()))?;
            if !bundle.id.is_empty() {
                if rule.group_id.is_empty() {
                    rule.group_id = bundle.id.clone();
                } else if rule.group_id != bundle.id {
                    return Err(RuleManagerError::Content(format!(
                        "rule group {} does not match group ID {}",
                        rule.group_id, bundle.id
                    )));
                }
            }
            self.adjust_rule(&mut rule, conf, stores)?;
            p.set_rule(rule);
        }
        Ok(())
    }
    pub async fn set_all_group_bundles<C: SharedConfigProvider + ?Sized>(
        &mut self,
        bundles: Vec<GroupBundle>,
        override_: bool,
        now: u64,
        conf: &C,
        stores: &[StoreInfo],
    ) -> Result<(), RuleManagerError> {
        let mut p = self.begin_patch();
        for r in self.config.rules() {
            if override_ || bundles.iter().any(|g| g.id == r.group_id) {
                p.delete_rule(&r.group_id, &r.id);
            }
        }
        for id in self.config.groups().keys() {
            if override_ || bundles.iter().any(|g| &g.id == id) {
                p.delete_group(id);
            }
        }
        for g in bundles {
            self.bundle_patch(&mut p, g, conf, stores)?;
        }
        self.try_commit_patch(p, now).await
    }
    pub async fn set_group_bundle<C: SharedConfigProvider + ?Sized>(
        &mut self,
        bundle: GroupBundle,
        now: u64,
        conf: &C,
        stores: &[StoreInfo],
    ) -> Result<(), RuleManagerError> {
        self.set_all_group_bundles(vec![bundle], false, now, conf, stores)
            .await
    }
    pub async fn delete_group_bundle(
        &mut self,
        id: &str,
        regex: bool,
        now: u64,
    ) -> Result<(), RuleManagerError> {
        let re = if regex { Some(go_regexp(id)?) } else { None };
        let matches = |s: &str| re.as_ref().map_or(s == id, |r| r.is_match(s));
        let mut p = self.begin_patch();
        for r in self.config.rules() {
            if matches(&r.group_id) {
                p.delete_rule(&r.group_id, &r.id);
            }
        }
        for id in self.config.groups().keys() {
            if matches(id) {
                p.delete_group(id);
            }
        }
        self.try_commit_patch(p, now).await
    }
}

// Go Perl character classes and word boundaries are ASCII even in UTF-8 patterns.
fn go_regexp(pattern: &str) -> Result<regex::Regex, regex::Error> {
    let mut translated = String::new();
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            translated.push(c);
            continue;
        }
        let Some(c) = chars.next() else {
            translated.push('\\');
            break;
        };
        translated.push_str(match c {
            'd' => "[0-9]",
            'D' => "[^0-9]",
            'w' => "[A-Za-z0-9_]",
            'W' => "[^A-Za-z0-9_]",
            's' => "[\\t\\n\\f\\r ]",
            'S' => "[^\\t\\n\\f\\r ]",
            'b' => "(?-u:\\b)",
            'B' => "(?-u:\\B)",
            _ => {
                translated.push('\\');
                translated.push(c);
                continue;
            }
        });
    }
    regex::Regex::new(&translated)
}
