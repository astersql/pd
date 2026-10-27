// Copyright 2026 AsterSQL.
use astersql_pd_errs::{TypedError, catalog::ERR_REGION_RULE_NOT_FOUND};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::CallbackTxnError;
use astersql_pd_storage_domain::{EndpointError, StorageEndpoint};

use crate::{LabelRule, LabelRuleIndex, RuleError, Timestamp};

/// Validation, persistence, and missing-rule failures remain distinguishable.
#[derive(Debug, thiserror::Error)]
pub enum LabelerError {
    #[error(transparent)]
    Rule(#[from] RuleError),
    #[error(transparent)]
    Load(#[from] EndpointError),
    #[error(transparent)]
    Transaction(#[from] CallbackTxnError<EndpointError>),
    #[error(transparent)]
    NotFound(TypedError),
}

/// Persisted label-rule owner. Exclusive mutation publishes a complete derived index.
/// The host supplies storage and time; this shell starts no background tasks.
pub struct RegionLabeler {
    pub(crate) storage: StorageEndpoint,
    pub(crate) paths: ClusterPaths,
    pub(crate) index: LabelRuleIndex,
}
impl RegionLabeler {
    /// Restore rules before exposing this owner; malformed records are discarded.
    pub async fn load(
        storage: impl Into<StorageEndpoint>,
        paths: ClusterPaths,
        now: Timestamp,
    ) -> Result<Self, LabelerError> {
        let storage = storage.into();
        let mut rules = Vec::new();
        let mut invalid_keys = Vec::new();
        storage
            .load_region_rules(&paths, |suffix, bytes| {
                match LabelRule::from_json(bytes, now) {
                    Ok(rule) => rules.push(rule),
                    Err(_) => {
                        let mut key = paths.region_label_path_prefix().into_bytes();
                        key.extend_from_slice(suffix);
                        invalid_keys.push(key);
                    }
                }
            })
            .await?;
        // Cleanup is one transaction per bad physical key, in scan order. A failure
        // aborts construction; earlier cleanup remains committed, matching Go.
        for key in invalid_keys {
            storage
                .run_in_txn(async |txn| txn.remove(&key).map_err(EndpointError::from))
                .await?;
        }
        Ok(Self {
            storage,
            paths,
            index: LabelRuleIndex::new(rules),
        })
    }
    /// Immutable authoritative snapshot and its published range/keyspace views.
    pub fn index(&self) -> &LabelRuleIndex {
        &self.index
    }
    /// Validate and persist before publishing the new rule and its boundaries.
    pub async fn set_label_rule(
        &mut self,
        mut rule: LabelRule,
        now: Timestamp,
    ) -> Result<(), LabelerError> {
        rule.check_and_adjust(now)?;
        self.set_validated_label_rule(rule).await
    }
    /// Watcher entry point, requiring an already validated and adjusted rule.
    pub async fn set_validated_label_rule(&mut self, rule: LabelRule) -> Result<(), LabelerError> {
        self.storage
            .run_in_txn(async |txn| {
                self.storage
                    .save_region_rule(txn, &self.paths, &rule.id, &rule)
            })
            .await?;
        self.index.set(rule);
        Ok(())
    }
    /// Persist deletion first, then report a missing authoritative rule as in Go.
    pub async fn delete_label_rule(&mut self, id: &str) -> Result<(), LabelerError> {
        self.persist_delete(id).await?;
        if self.index.rule(id).is_none() {
            return Err(LabelerError::NotFound(
                ERR_REGION_RULE_NOT_FOUND.instantiate([id]),
            ));
        }
        self.index.delete(id);
        Ok(())
    }
    /// Watcher deletion is idempotent, including for absent rules.
    pub async fn delete_label_rule_if_present(&mut self, id: &str) -> Result<(), LabelerError> {
        self.persist_delete(id).await?;
        self.index.delete(id);
        Ok(())
    }
    async fn persist_delete(&self, id: &str) -> Result<(), LabelerError> {
        self.storage
            .run_in_txn(async |txn| self.storage.delete_region_rule(txn, &self.paths, id))
            .await?;
        Ok(())
    }
}
