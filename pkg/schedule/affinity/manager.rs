// Copyright 2026 AsterSQL.
use crate::{
    group::{Availability, Group, RuntimeGroupInfo},
    ranges::{
        GroupKeyRanges, extract_key_ranges_from_label_rule,
        parse_affinity_group_id_from_label_rule, validate_no_overlap,
    },
};
use astersql_pd_errs::TypedError;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_labeler::{LabelRule, RegionLabeler};
use astersql_pd_storage_endpoint::{EndpointError, StorageEndpoint};
use std::collections::BTreeMap;

/// Startup failures preserve storage and range-validation errors.
#[derive(Debug, thiserror::Error)]
pub enum RestoreError {
    #[error(transparent)]
    Storage(#[from] EndpointError),
    #[error(transparent)]
    Ranges(#[from] TypedError),
}
/// Restored group metadata. The host owns availability-loop lifecycle.
#[derive(Debug)]
pub struct RestoredGroup {
    pub info: RuntimeGroupInfo,
    pub affinity_version: u64,
    pub label_rule: Option<LabelRule>,
}
/// Startup snapshot, published only after the complete range set is validated.
#[derive(Debug, Default)]
pub struct Manager {
    pub groups: BTreeMap<String, RestoredGroup>,
    pub key_ranges: BTreeMap<String, GroupKeyRanges>,
}
impl Manager {
    /// Loads metadata without starting policy or availability tasks. Time is seconds.
    pub async fn initialize(
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
        labeler: &RegionLabeler,
        now: u64,
    ) -> Result<Self, RestoreError> {
        let mut restored = Self::default();
        storage
            .load_all_affinity_groups(paths, |_, bytes| {
                // Like encoding/json into &Group, null is a valid zero-valued group.
                let Ok(group) = serde_json::from_slice::<Option<Group>>(bytes) else {
                    return;
                };
                let group = group.unwrap_or_default();
                restored.groups.entry(group.id.clone()).or_insert_with(|| {
                    let mut info = RuntimeGroupInfo::default();
                    info.group = group;
                    info.set_availability(Availability::Degraded, now);
                    RestoredGroup {
                        info,
                        affinity_version: 1,
                        label_rule: None,
                    }
                });
            })
            .await?;
        let mut all_ranges = Vec::new();
        for rule in labeler.index().rules() {
            let Some(id) = parse_affinity_group_id_from_label_rule(Some(rule)) else {
                continue;
            };
            let Some(group) = restored.groups.get_mut(id) else {
                continue;
            };
            let Ok(ranges) = extract_key_ranges_from_label_rule(Some(rule)) else {
                continue;
            };
            if !ranges.key_ranges.is_empty() {
                restored.key_ranges.insert(id.into(), ranges.clone());
                all_ranges.push(ranges);
            }
            group.info.range_count = rule.key_ranges().map_or(0, |ranges| ranges.len());
            group.affinity_version += 1;
            group.label_rule = Some(rule.clone());
        }
        validate_no_overlap(&all_ranges)?;
        Ok(restored)
    }
}
