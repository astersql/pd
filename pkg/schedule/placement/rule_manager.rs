// Copyright 2026 AsterSQL.
//! Rule lifecycle over the canonical storage endpoint. Callers serialize mutations.
use crate::{
    PeerRoleType, Rule, RuleConfig, RuleConfigPatch, RuleGroup, RuleList, match_label_constraints,
};
use astersql_pd_core::StoreInfo;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_storage_endpoint::{EndpointError, StorageEndpoint};
use astersql_pd_storage_kv::CallbackTxnError;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum RuleManagerError {
    #[error("[PD:placement:ErrRuleContent]invalid rule content, {0}")]
    Content(String),
    #[error("[PD:hex:ErrHexDecodingString]decode string {0} error")]
    Hex(String),
    #[error("invalid group regexp: {0}")]
    Regex(#[from] regex::Error),
    #[error(transparent)]
    Build(#[from] crate::BuildRuleListError),
    #[error(transparent)]
    Storage(#[from] EndpointError),
    #[error(transparent)]
    Transaction(#[from] CallbackTxnError<EndpointError>),
}
/// Owns immutable configuration snapshots; no private runtime, clock or background work.
/// `&mut self` makes the host's mutation serialization explicit.
pub struct RuleManager {
    pub(crate) storage: Arc<StorageEndpoint>,
    pub(crate) paths: ClusterPaths,
    pub(crate) config: RuleConfig,
    pub(crate) list: Option<RuleList>,
    pub(crate) cache: crate::RegionRuleFitCacheManager,
    initialized: bool,
    key_type: String,
}
impl RuleManager {
    pub fn new(storage: Arc<StorageEndpoint>, paths: ClusterPaths) -> Self {
        Self {
            storage,
            paths,
            config: RuleConfig::default(),
            list: None,
            cache: Default::default(),
            initialized: false,
            key_type: String::new(),
        }
    }
    pub fn set_key_type(&mut self, key_type: &str) {
        self.key_type = key_type.into();
    }
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
    pub fn config(&self) -> &RuleConfig {
        &self.config
    }
    pub fn rule_list(&self) -> Option<&RuleList> {
        self.list.as_ref()
    }
    pub fn get_rule(&self, group: &str, id: &str) -> Option<Rule> {
        self.config.get_rule(group, id).map(|r| r.as_ref().clone())
    }
    /// Validate decoded ranges, role, witness limits and optional current store labels.
    pub fn adjust_rule<C: SharedConfigProvider + ?Sized>(
        &self,
        rule: &mut Rule,
        conf: &C,
        stores: &[StoreInfo],
    ) -> Result<(), RuleManagerError> {
        fn hex(s: &str) -> Result<Vec<u8>, RuleManagerError> {
            if !s.len().is_multiple_of(2) {
                return Err(RuleManagerError::Hex(s.into()));
            }
            s.as_bytes()
                .chunks_exact(2)
                .map(|p| {
                    let digit = |b: u8| (b as char).to_digit(16);
                    digit(p[0])
                        .zip(digit(p[1]))
                        .map(|(a, b)| (a * 16 + b) as u8)
                        .ok_or_else(|| RuleManagerError::Hex(s.into()))
                })
                .collect()
        }
        rule.start_key = hex(&rule.start_key_hex)?;
        rule.end_key = hex(&rule.end_key_hex)?;
        let error = |s: &str| RuleManagerError::Content(s.into());
        if !rule.end_key.is_empty() && rule.end_key <= rule.start_key {
            return Err(error("endKey should be greater than startKey"));
        }
        if self.key_type == "table" || self.key_type == "txn" {
            for key in [&rule.start_key, &rule.end_key] {
                if !key.is_empty() && astersql_pd_codec::decode_bytes(key).is_err() {
                    return Err(error("key should be encoded in configured mode"));
                }
            }
        }
        if rule.group_id.is_empty() {
            return Err(error("group ID should not be empty"));
        }
        if rule.id.is_empty() {
            return Err(error("ID should not be empty"));
        }
        if !rule.role.is_valid() {
            return Err(error(&format!(
                "invalid role {}",
                String::from(rule.role.clone())
            )));
        }
        if rule.count <= 0 {
            return Err(error(&format!("invalid count {}", rule.count)));
        }
        if rule.role == PeerRoleType::Leader && rule.count > 1 {
            return Err(error(&format!(
                "define multiple leaders by count {}",
                rule.count
            )));
        }
        if rule.is_witness && rule.count > conf.get_max_replicas() / 2 {
            return Err(error(&format!(
                "define too many witness by count {}",
                rule.count
            )));
        }
        for c in &rule.label_constraints {
            if !c.op.is_valid() {
                return Err(error(&format!("invalid op {}", String::from(c.op.clone()))));
            }
            if rule.is_witness && c.key == "engine" && c.values.iter().any(|v| v == "tiflash") {
                return Err(error("witness can't combine with tiflash"));
            }
        }
        if !stores.is_empty()
            && !stores
                .iter()
                .any(|s| match_label_constraints(Some(s), &rule.label_constraints))
        {
            return Err(error(&format!(
                "rule '{}' from rule group '{}' can not match any store",
                rule.id, rule.group_id
            )));
        }
        Ok(())
    }
    /// Load, repair old keys and initialize defaults. Skipped loading performs no I/O.
    pub async fn initialize<C: SharedConfigProvider + ?Sized>(
        &mut self,
        max_replicas: i64,
        labels: &[String],
        isolation: &str,
        skip: bool,
        conf: &C,
        stores: &[StoreInfo],
    ) -> Result<(), RuleManagerError> {
        if self.initialized {
            return Ok(());
        }
        if skip {
            self.initialized = true;
            return Ok(());
        }
        let mut rows = Vec::new();
        let loaded = self
            .storage
            .load_rules(&self.paths, |k, v| rows.push((k.to_vec(), v.to_vec())))
            .await;
        let mut saves = Vec::new();
        let mut deletes = Vec::new();
        for (key, value) in rows {
            let valid = Rule::from_json(&value)
                .ok()
                .and_then(|mut r| self.adjust_rule(&mut r, conf, stores).ok().map(|()| r));
            if let Some(r) = valid.filter(|r| self.config.get_rule(&r.group_id, &r.id).is_none()) {
                if key != r.store_key().as_bytes() {
                    deletes.push(key);
                    saves.push(r.clone());
                }
                self.config.set_rule(r);
            } else {
                deletes.push(key);
            }
        }
        loaded?;
        // Go repairs saves before deletes, in chunks of MaxEtcdTxnOps (120).
        let mut ops: Vec<_> = saves
            .into_iter()
            .map(|r| (r.store_key().into_bytes(), Some(r)))
            .collect();
        ops.extend(deletes.into_iter().map(|k| (k, None)));
        for chunk in ops.chunks(120) {
            self.storage
                .run_in_txn(async |txn| {
                    for (key, rule) in chunk {
                        let name = String::from_utf8_lossy(key);
                        if let Some(rule) = rule {
                            self.storage.save_rule(txn, &self.paths, &name, rule)?;
                        } else {
                            let mut raw = self.paths.rules_path_prefix().into_bytes();
                            raw.extend_from_slice(key);
                            txn.remove(&raw)?;
                        }
                    }
                    Ok::<_, EndpointError>(())
                })
                .await?;
        }
        let mut groups = Vec::new();
        let loaded = self
            .storage
            .load_rule_groups(&self.paths, |_, v| groups.push(v.to_vec()))
            .await;
        for value in groups {
            if let Ok(g) = RuleGroup::from_json(&value) {
                self.config.set_group(g);
            }
        }
        loaded?;
        if self.config.rules().next().is_none() {
            let witness = if conf.is_witness_allowed() && max_replicas >= 3 {
                max_replicas / 2
            } else {
                0
            };
            let mut defaults = vec![Rule {
                group_id: "pd".into(),
                id: "default".into(),
                role: PeerRoleType::Voter,
                count: max_replicas - witness,
                location_labels: labels.to_vec(),
                isolation_level: isolation.into(),
                ..Default::default()
            }];
            if witness > 0 {
                defaults.push(Rule {
                    group_id: "pd".into(),
                    id: "witness".into(),
                    role: PeerRoleType::Voter,
                    count: witness,
                    is_witness: true,
                    location_labels: labels.to_vec(),
                    isolation_level: isolation.into(),
                    ..Default::default()
                });
            }
            self.storage
                .run_in_txn(async |txn| {
                    for r in defaults {
                        self.storage
                            .save_rule(txn, &self.paths, &r.store_key(), &r)?;
                        self.config.set_rule(r);
                    }
                    Ok::<_, EndpointError>(())
                })
                .await?;
        }
        self.config.adjust();
        self.list = Some(RuleList::build(self.config.rules().cloned())?);
        self.initialized = true;
        Ok(())
    }
    /// Persist the trimmed incoming rule before publishing its version/time update.
    /// As in Go, stored version/time are the caller's values, not post-commit values.
    pub async fn set_rule<C: SharedConfigProvider + ?Sized>(
        &mut self,
        mut rule: Rule,
        now: u64,
        conf: &C,
        stores: &[StoreInfo],
    ) -> Result<(), RuleManagerError> {
        self.adjust_rule(&mut rule, conf, stores)?;
        let mut patch = self.config.begin_patch();
        patch.set_rule(rule.clone());
        self.commit_single(patch, rule, now).await
    }
    pub async fn delete_rule(
        &mut self,
        group: &str,
        id: &str,
        now: u64,
    ) -> Result<(), RuleManagerError> {
        let mut patch = self.config.begin_patch();
        patch.delete_rule(group, id);
        let key = Rule {
            group_id: group.into(),
            id: id.into(),
            ..Default::default()
        }
        .store_key();
        self.commit_delete(patch, &key, now).await
    }
    async fn commit_single(
        &mut self,
        mut patch: RuleConfigPatch,
        rule: Rule,
        now: u64,
    ) -> Result<(), RuleManagerError> {
        RuleList::build(patch.adjust().rules().cloned())?;
        patch.trim();
        if patch.mutation_counts().0 > 0 {
            let r = rule;
            self.storage
                .run_in_txn(async |txn| {
                    self.storage.save_rule(txn, &self.paths, &r.store_key(), &r)
                })
                .await?;
        }
        self.config = patch.commit(now);
        // Rebind immutable rule-list entries to committed version/time snapshots.
        self.list =
            Some(RuleList::build(self.config.rules().cloned()).expect("validated rule coverage"));
        Ok(())
    }
    async fn commit_delete(
        &mut self,
        mut patch: RuleConfigPatch,
        key: &str,
        now: u64,
    ) -> Result<(), RuleManagerError> {
        RuleList::build(patch.adjust().rules().cloned())?;
        patch.trim();
        if patch.mutation_counts().0 > 0 {
            self.storage
                .run_in_txn(async |txn| self.storage.delete_rule(txn, &self.paths, key))
                .await?;
        }
        self.config = patch.commit(now);
        // Rebind immutable rule-list entries to committed version/time snapshots.
        self.list =
            Some(RuleList::build(self.config.rules().cloned()).expect("validated rule coverage"));
        Ok(())
    }
}
