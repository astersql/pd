// Copyright 2026 AsterSQL.
use crate::{
    manager::Manager,
    ranges::{
        GroupKeyRanges, apply_remove_ops, extract_key_ranges_from_label_rule, get_label_rule_id,
        make_label_rule, validate_no_overlap,
    },
    txn::TransactionError,
};
use astersql_pd_errs::catalog::ERR_AFFINITY_GROUP_EXIST;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_storage_endpoint::{EndpointError, MAX_BATCH_OPS, StorageEndpoint};
use std::collections::BTreeMap;
impl Manager {
    fn current_ranges(
        &self,
        labels: &RegionLabeler,
        id: &str,
    ) -> Result<GroupKeyRanges, TransactionError> {
        if let Some(r) = self.key_ranges.get(id) {
            return Ok(r.clone());
        }
        if let Some(rule) = labels.index().rule(&get_label_rule_id(id)) {
            return Ok(extract_key_ranges_from_label_rule(Some(rule))?);
        }
        if self.groups.contains_key(id) {
            return Ok(GroupKeyRanges {
                group_id: id.into(),
                key_ranges: vec![],
            });
        }
        Err(TransactionError::MissingLabel(id.into()))
    }
    /// Validates additions before removals and publishes only after all label batches commit.
    /// Time is Unix seconds. Removing ranges invalidates the group's region cache counts.
    pub async fn update_affinity_group_key_ranges(
        &mut self,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
        labels: &mut RegionLabeler,
        adds: &[GroupKeyRanges],
        removes: &[GroupKeyRanges],
        now: u64,
    ) -> Result<(), TransactionError> {
        let mut added = BTreeMap::new();
        let mut removed = BTreeMap::new();
        for op in adds.iter().filter(|o| !o.key_ranges.is_empty()) {
            if added
                .insert(op.group_id.clone(), GroupKeyRanges::default())
                .is_some()
            {
                return Err(ERR_AFFINITY_GROUP_EXIST.instantiate([&op.group_id]).into());
            }
        }
        for op in removes.iter().filter(|o| !o.key_ranges.is_empty()) {
            if removed
                .insert(op.group_id.clone(), GroupKeyRanges::default())
                .is_some()
                || added.contains_key(&op.group_id)
            {
                return Err(ERR_AFFINITY_GROUP_EXIST.instantiate([&op.group_id]).into());
            }
        }
        for op in adds {
            let mut r = self.current_ranges(labels, &op.group_id)?;
            r.key_ranges.extend(op.key_ranges.clone());
            added.insert(op.group_id.clone(), r);
        }
        if !adds.is_empty() {
            let mut all = adds.to_vec();
            all.extend(self.key_ranges.values().cloned());
            validate_no_overlap(&all)?;
        }
        for op in removes {
            removed.insert(
                op.group_id.clone(),
                apply_remove_ops(&self.current_ranges(labels, &op.group_id)?, op)?,
            );
        }
        let mut plan = labels.new_plan();
        let time = i128::from(now) * 1_000_000_000;
        for op in adds {
            let mut rule = make_label_rule(&added[&op.group_id]);
            rule.check_and_adjust(time)?;
            plan.set_label_rule(rule, time).expect("validated rule");
        }
        for op in removes {
            let r = &removed[&op.group_id];
            if r.key_ranges.is_empty() {
                plan.delete_label_rule(get_label_rule_id(&op.group_id))
                    .expect("valid plan");
            } else {
                let mut rule = make_label_rule(r);
                rule.check_and_adjust(time)?;
                plan.set_label_rule(rule, time).expect("validated rule");
            }
        }
        for batch in plan.commit_ops().chunks(MAX_BATCH_OPS) {
            storage
                .run_in_txn(async |txn| {
                    for op in batch {
                        op.buffer(txn, storage, paths)?;
                    }
                    Ok::<_, EndpointError>(())
                })
                .await?;
        }
        plan.apply(labels);
        for (changes, clear) in [(&added, false), (&removed, true)] {
            for (id, r) in changes {
                if clear && r.key_ranges.is_empty() {
                    self.key_ranges.remove(id);
                } else {
                    self.key_ranges.insert(id.clone(), r.clone());
                }
                if let Some(g) = self.groups.get_mut(id) {
                    g.label_rule = labels.index().rule(&get_label_rule_id(id)).cloned();
                    g.info.range_count = r.key_ranges.len();
                    g.info.affinity_region_count = 0;
                    g.affinity_version += 1;
                    if clear {
                        g.info.region_count = 0;
                    }
                }
            }
        }
        Ok(())
    }
}
