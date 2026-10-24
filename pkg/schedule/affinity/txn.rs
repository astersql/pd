// Copyright 2026 AsterSQL.
use crate::{
    group::{Availability, Group, InvalidGroupId, RuntimeGroupInfo, validate_group_id},
    manager::{Manager, RestoredGroup},
    ranges::{GroupKeyRanges, get_label_rule_id, make_label_rule, validate_no_overlap},
};
use astersql_pd_errs::{
    TypedError,
    catalog::{ERR_AFFINITY_GROUP_CONTENT, ERR_AFFINITY_GROUP_EXIST, ERR_AFFINITY_GROUP_NOT_FOUND},
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_labeler::{RegionLabeler, RuleError};
use astersql_pd_storage_endpoint::{EndpointError, MAX_BATCH_OPS, StorageEndpoint};
use std::collections::BTreeSet;

/// Validation and backend errors retain their canonical identity.
#[derive(Debug, thiserror::Error)]
pub enum TransactionError {
    #[error("label rule not found for group {0}")]
    MissingLabel(String),
    #[error(transparent)]
    Content(#[from] TypedError),
    #[error(transparent)]
    InvalidId(#[from] InvalidGroupId),
    #[error(transparent)]
    Label(#[from] RuleError),
    #[error(transparent)]
    Storage(#[from] astersql_pd_storage_kv::CallbackTxnError<EndpointError>),
}
impl Manager {
    /// Creates groups and their labels; publishes only after every storage batch succeeds.
    pub async fn create_affinity_groups(
        &mut self,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
        labels: &mut RegionLabeler,
        changes: &[GroupKeyRanges],
        now: u64,
    ) -> Result<(), TransactionError> {
        self.create_groups(storage, paths, labels, changes, now, false)
            .await
    }
    /// Validates IDs before skipping groups already present in memory.
    pub async fn create_affinity_groups_if_not_exists(
        &mut self,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
        labels: &mut RegionLabeler,
        changes: &[GroupKeyRanges],
        now: u64,
    ) -> Result<(), TransactionError> {
        self.create_groups(storage, paths, labels, changes, now, true)
            .await
    }
    async fn create_groups(
        &mut self,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
        labels: &mut RegionLabeler,
        changes: &[GroupKeyRanges],
        now: u64,
        skip: bool,
    ) -> Result<(), TransactionError> {
        for c in changes {
            validate_group_id(&c.group_id)?;
        }
        let filtered = changes
            .iter()
            .filter(|c| !skip || !self.groups.contains_key(&c.group_id))
            .collect::<Vec<_>>();
        if !skip {
            for c in &filtered {
                if self.groups.contains_key(&c.group_id) {
                    return Err(ERR_AFFINITY_GROUP_EXIST.instantiate([&c.group_id]).into());
                }
            }
        }
        if filtered.is_empty() {
            return Ok(());
        }
        let mut ranges = filtered.iter().map(|c| (*c).clone()).collect::<Vec<_>>();
        ranges.extend(self.key_ranges.values().cloned());
        validate_no_overlap(&ranges)?;
        let mut plan = labels.new_plan();
        for c in &filtered {
            if !c.key_ranges.is_empty() {
                let mut rule = make_label_rule(c);
                rule.check_and_adjust(i128::from(now) * 1_000_000_000)?;
                plan.set_label_rule(rule, i128::from(now) * 1_000_000_000)
                    .expect("already validated rule");
            }
        }
        let groups = filtered
            .iter()
            .map(|c| Group {
                id: c.group_id.clone(),
                create_timestamp: now,
                ..Default::default()
            })
            .collect::<Vec<_>>();
        // Go orders all label operations before group operations. Preserve chunk boundaries.
        let count = plan.commit_ops().len() + groups.len();
        for start in (0..count).step_by(MAX_BATCH_OPS) {
            storage
                .run_in_txn(async |txn| {
                    for i in start..(start + MAX_BATCH_OPS).min(count) {
                        if i < plan.commit_ops().len() {
                            plan.commit_ops()[i].buffer(txn, storage, paths)?;
                        } else {
                            let g = &groups[i - plan.commit_ops().len()];
                            storage.save_affinity_group(txn, paths, &g.id, g)?;
                        }
                    }
                    Ok::<_, EndpointError>(())
                })
                .await?;
        }
        plan.apply(labels);
        for c in &filtered {
            if c.key_ranges.is_empty() {
                self.key_ranges.remove(&c.group_id);
            } else {
                self.key_ranges.insert(c.group_id.clone(), (*c).clone());
            }
        }
        for (c, g) in filtered.iter().zip(groups) {
            // Duplicate new IDs are accepted by Go. Initialization keeps the first group.
            let entry = self.groups.entry(g.id.clone()).or_insert_with(|| {
                let mut info = RuntimeGroupInfo::default();
                info.group = g;
                info.set_availability(Availability::Degraded, now);
                RestoredGroup {
                    info,
                    affinity_version: 1,
                    label_rule: None,
                }
            });
            entry.label_rule = if c.key_ranges.is_empty() {
                None
            } else {
                labels
                    .index()
                    .rule(&get_label_rule_id(&c.group_id))
                    .cloned()
            };
            entry.info.range_count = c.key_ranges.len();
            entry.info.affinity_region_count = 0;
            entry.affinity_version += 1;
        }
        Ok(())
    }
    /// Deduplicates IDs; force tolerates unknown IDs and removes their leftover labels.
    pub async fn delete_affinity_groups(
        &mut self,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
        labels: &mut RegionLabeler,
        ids: &[String],
        force: bool,
    ) -> Result<(), TransactionError> {
        if ids.is_empty() {
            return Err(ERR_AFFINITY_GROUP_CONTENT
                .instantiate(["no group ids provided"])
                .into());
        }
        let mut seen = BTreeSet::new();
        let mut unique = Vec::new();
        for id in ids {
            if seen.insert(id) {
                if !force
                    && self
                        .key_ranges
                        .get(id)
                        .is_some_and(|r| !r.key_ranges.is_empty())
                {
                    return Err(ERR_AFFINITY_GROUP_CONTENT
                        .instantiate([format!(
                            "affinity group {id} has key ranges, use force=true to delete"
                        )])
                        .into());
                }
                unique.push(id);
            }
        }
        if !force {
            for id in &unique {
                if !self.groups.contains_key(*id) {
                    return Err(ERR_AFFINITY_GROUP_NOT_FOUND
                        .instantiate([id.as_str()])
                        .into());
                }
            }
        }
        let mut plan = labels.new_plan();
        for id in &unique {
            plan.delete_label_rule(get_label_rule_id(id))
                .expect("delete-only plan is valid");
        }
        let count = plan.commit_ops().len() + unique.len();
        for start in (0..count).step_by(MAX_BATCH_OPS) {
            storage
                .run_in_txn(async |txn| {
                    for i in start..(start + MAX_BATCH_OPS).min(count) {
                        if let Some(op) = plan.commit_ops().get(i) {
                            op.buffer(txn, storage, paths)?;
                        } else {
                            storage.delete_affinity_group(
                                txn,
                                paths,
                                unique[i - plan.commit_ops().len()],
                            )?;
                        }
                    }
                    Ok::<_, EndpointError>(())
                })
                .await?;
        }
        plan.apply(labels);
        for id in unique {
            self.key_ranges.remove(id);
            self.clear_group_cache(id);
            self.label_rule_buffer.remove(id);
            self.groups.remove(id);
        }
        Ok(())
    }
}
