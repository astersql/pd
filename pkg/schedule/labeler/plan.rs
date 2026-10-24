// Copyright 2026 AsterSQL.
use std::collections::BTreeMap;

use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_endpoint::{EndpointError, MAX_BATCH_OPS, StorageEndpoint};
use astersql_pd_storage_kv::CallbackTxn;

use crate::{LabelRule, LabelerError, RegionLabeler, RuleError, Timestamp};

/// A composable persistence operation, independent of protocol types.
#[derive(Clone, Debug)]
pub enum LabelCommitOp {
    Set(LabelRule),
    Delete(String),
}
impl LabelCommitOp {
    /// Buffer into a caller-owned transaction, allowing composition with other modules.
    pub fn buffer(
        &self,
        txn: &mut CallbackTxn<'_>,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), EndpointError> {
        match self {
            Self::Set(rule) => storage.save_region_rule(txn, paths, &rule.id, rule),
            Self::Delete(id) => storage.delete_region_rule(txn, paths, id),
        }
    }
}

/// Ordered operations with sticky validation failure, matching Go Plan.
#[derive(Default, Debug)]
pub struct LabelPlan {
    ops: Vec<LabelCommitOp>,
    error: Option<RuleError>,
}
impl LabelPlan {
    pub fn new() -> Self {
        Self::default()
    }
    /// Once validation fails, every later addition returns the same error.
    pub fn set_label_rule(
        &mut self,
        mut rule: LabelRule,
        now: Timestamp,
    ) -> Result<(), &RuleError> {
        if self.error.is_none() {
            match rule.check_and_adjust(now) {
                Ok(()) => self.ops.push(LabelCommitOp::Set(rule)),
                Err(error) => self.error = Some(error),
            }
        }
        self.error.as_ref().map_or(Ok(()), Err)
    }
    /// Missing IDs are allowed; persistence and memory deletion are idempotent.
    pub fn delete_label_rule(&mut self, id: impl Into<String>) -> Result<(), &RuleError> {
        if let Some(error) = &self.error {
            return Err(error);
        }
        self.ops.push(LabelCommitOp::Delete(id.into()));
        Ok(())
    }
    pub fn error(&self) -> Option<&RuleError> {
        self.error.as_ref()
    }
    /// An invalid plan exposes no operations, even those added before the error.
    pub fn commit_ops(&self) -> &[LabelCommitOp] {
        if self.error.is_some() { &[] } else { &self.ops }
    }
    /// Explicit Go-compatible publication. Caller must first commit all operations.
    pub fn apply(&self, owner: &mut RegionLabeler) {
        for op in self.commit_ops() {
            match op {
                LabelCommitOp::Set(rule) => {
                    owner.index.set(rule.clone());
                }
                LabelCommitOp::Delete(id) => {
                    owner.index.delete(id);
                }
            }
        }
    }
}

/// Bulk update: validate every set, last duplicate wins, sets override deletes.
#[derive(Default, Debug)]
pub struct LabelRulePatch {
    pub set_rules: Vec<LabelRule>,
    pub delete_rules: Vec<String>,
}
impl RegionLabeler {
    /// Create an independent plan; no state changes occur until explicit publication.
    pub fn new_plan(&self) -> LabelPlan {
        LabelPlan::new()
    }

    /// Execute an ordered plan in one transaction, then publish. Invalid plans are inert.
    pub async fn commit_plan(&mut self, plan: &LabelPlan) -> Result<(), LabelerError> {
        if plan.error().is_some() {
            return Ok(());
        }
        self.storage
            .run_in_txn(async |txn| {
                for op in plan.commit_ops() {
                    op.buffer(txn, &self.storage, &self.paths)?;
                }
                Ok::<_, EndpointError>(())
            })
            .await?;
        plan.apply(self);
        Ok(())
    }
    /// Each group of 120 operations is atomic; earlier groups survive later failure.
    /// Memory is published only after every group succeeds. Reload after partial failure.
    pub async fn patch(
        &mut self,
        patch: LabelRulePatch,
        now: Timestamp,
    ) -> Result<(), LabelerError> {
        let mut sets = BTreeMap::new();
        for mut rule in patch.set_rules {
            rule.check_and_adjust(now)?;
            sets.insert(rule.id.clone(), rule);
        }
        let mut ops = Vec::new();
        for id in patch.delete_rules {
            if !sets.contains_key(&id) {
                ops.push(LabelCommitOp::Delete(id));
            }
        }
        ops.extend(sets.into_values().map(LabelCommitOp::Set));
        for chunk in ops.chunks(MAX_BATCH_OPS) {
            self.storage
                .run_in_txn(async |txn| {
                    for op in chunk {
                        op.buffer(txn, &self.storage, &self.paths)?;
                    }
                    Ok::<_, EndpointError>(())
                })
                .await?;
        }
        // Exclusive borrow prevents readers observing partial publication.
        LabelPlan { ops, error: None }.apply(self);
        Ok(())
    }
}
