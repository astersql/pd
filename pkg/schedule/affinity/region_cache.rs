// Copyright 2026 AsterSQL.
//! Region identity/version caching and host-driven metadata synchronization.
use crate::{
    group::{Availability, Group, GroupState, RuntimeGroupInfo},
    manager::{Manager, RestoredGroup},
    ranges::{
        LABEL_KEY, LABEL_RULE_ID_PREFIX, extract_key_ranges_from_label_rule, get_label_rule_id,
        parse_affinity_group_id_from_label_rule,
    },
};
use astersql_pd_core::RegionInfo;
use astersql_pd_errs::TypedError;
use astersql_pd_schedule_labeler::{LabelRule, RegionLabeler};
use std::sync::Arc;

#[derive(Debug)]
pub(crate) struct RegionCache {
    region: Arc<RegionInfo>,
    group_id: String,
    version: u64,
    affinity: bool,
}
impl Manager {
    /// Total includes only affinity entries from the current group version.
    pub fn affinity_region_count(&self) -> usize {
        self.groups
            .values()
            .map(|g| g.info.affinity_region_count)
            .sum()
    }
    /// Deletion/merge notifications are idempotent, including stale versions.
    pub fn invalid_cache(&mut self, region_id: u64) {
        if let Some(c) = self.regions.remove(&region_id)
            && let Some(g) = self.groups.get_mut(&c.group_id)
        {
            g.info.region_count -= 1;
            if c.affinity && c.version == g.affinity_version {
                g.info.affinity_region_count -= 1;
            }
        }
    }
    /// Clears membership as well as current affinity, unlike a version reset.
    pub(crate) fn clear_group_cache(&mut self, id: &str) {
        self.regions.retain(|_, c| c.group_id != id);
        if let Some(g) = self.groups.get_mut(id) {
            g.info.region_count = 0;
            g.info.affinity_region_count = 0;
        }
    }
    /// Read-only query; region snapshots use Arc identity like Go RegionInfo pointers.
    pub fn get_region_affinity_group_state(
        &self,
        labels: &RegionLabeler,
        region: Option<&Arc<RegionInfo>>,
        now: u64,
    ) -> (Option<GroupState>, bool) {
        let Some(r) = region else {
            return (None, false);
        };
        if self.groups.is_empty() {
            return (None, false);
        }
        if let Some(c) = self.regions.get(&r.meta().id)
            && let Some(g) = self.groups.get(&c.group_id)
            && c.version == g.affinity_version
            && Arc::ptr_eq(r, &c.region)
        {
            return (Some(g.info.state(now)), c.affinity);
        }
        let id = labels.get_region_label(r, LABEL_KEY, i128::from(now) * 1_000_000_000);
        if id.is_empty() {
            return (None, false);
        }
        let Some(g) = self.groups.get(&id) else {
            return (None, false);
        };
        let state = g.info.state(now);
        let mut voters: Vec<_> = r.voters().iter().map(|p| p.store_id).collect();
        voters.sort_unstable();
        let affinity = state.affinity_scheduling_allowed
            && r.leader().map_or(0, |p| p.store_id) == state.group.leader_store_id
            && voters == state.group.voter_store_ids;
        (Some(state), affinity)
    }
    /// Saves successful ownership queries; a missing label leaves old membership intact.
    pub fn get_and_cache_region_affinity_group_state(
        &mut self,
        labels: &RegionLabeler,
        region: Option<&Arc<RegionInfo>>,
        now: u64,
    ) -> (Option<GroupState>, bool) {
        let result = self.get_region_affinity_group_state(labels, region, now);
        if let (Some(state), Some(r)) = (&result.0, region) {
            self.invalid_cache(r.meta().id);
            let g = self
                .groups
                .get_mut(&state.group.id)
                .expect("exclusive access preserves group");
            g.info.region_count += 1;
            if result.1 {
                g.info.affinity_region_count += 1;
            }
            self.regions.insert(
                r.meta().id,
                RegionCache {
                    region: Arc::clone(r),
                    group_id: state.group.id.clone(),
                    version: g.affinity_version,
                    affinity: result.1,
                },
            );
        }
        result
    }
    /// Watcher group event. Equal peer layouts are no-ops; timestamps are creation-only.
    pub fn sync_group_from_etcd(&mut self, group: &Group, labels: &RegionLabeler, now: u64) {
        if let Some(g) = self.groups.get_mut(&group.id) {
            if g.info.group.leader_store_id != group.leader_store_id
                || g.info.group.voter_store_ids != group.voter_store_ids
            {
                g.info.group.leader_store_id = group.leader_store_id;
                g.info.group.voter_store_ids = group.voter_store_ids.clone();
                g.info.affinity_region_count = 0;
                g.affinity_version = g.affinity_version.wrapping_add(1);
            }
            return;
        }
        let rule = labels
            .index()
            .rule(&get_label_rule_id(&group.id))
            .cloned()
            .or_else(|| self.label_rule_buffer.get(&group.id).cloned());
        let mut info = RuntimeGroupInfo::default();
        info.group = group.clone();
        info.set_availability(Availability::Degraded, now);
        self.groups.insert(
            group.id.clone(),
            RestoredGroup {
                info,
                affinity_version: 1,
                label_rule: None,
            },
        );
        if let Some(rule) = rule
            && let Ok(ranges) = extract_key_ranges_from_label_rule(Some(&rule))
            && !ranges.key_ranges.is_empty()
        {
            self.key_ranges.insert(group.id.clone(), ranges.clone());
            let g = self.groups.get_mut(&group.id).unwrap();
            g.info.range_count = ranges.key_ranges.len();
            g.label_rule = Some(rule);
            g.affinity_version += 1;
        }
        self.label_rule_buffer.remove(&group.id);
    }
    pub fn sync_group_delete_from_etcd(&mut self, id: &str) {
        self.clear_group_cache(id);
        self.groups.remove(id);
        self.key_ranges.remove(id);
        self.label_rule_buffer.remove(id);
    }
    /// Buffers label-first events; malformed affinity ranges fail before mutation.
    /// The host synchronizes RegionLabeler separately, matching the Go watcher contract.
    pub fn sync_key_ranges_from_etcd(&mut self, rule: &LabelRule) -> Result<(), TypedError> {
        let Some(id) = parse_affinity_group_id_from_label_rule(Some(rule)) else {
            return Ok(());
        };
        let ranges = extract_key_ranges_from_label_rule(Some(rule))?;
        let Some(g) = self.groups.get_mut(id) else {
            if ranges.key_ranges.is_empty() {
                self.label_rule_buffer.remove(id);
            } else {
                self.label_rule_buffer.insert(id.into(), rule.clone());
            }
            return Ok(());
        };
        self.label_rule_buffer.remove(id);
        g.info.range_count = ranges.key_ranges.len();
        g.info.affinity_region_count = 0;
        g.affinity_version = g.affinity_version.wrapping_add(1);
        g.label_rule = Some(rule.clone());
        if ranges.key_ranges.is_empty() {
            self.key_ranges.remove(id);
        } else {
            self.key_ranges.insert(id.into(), ranges);
        }
        Ok(())
    }
    pub fn sync_key_ranges_delete_from_etcd(&mut self, rule_id: &str) {
        let Some(id) = rule_id
            .strip_prefix(LABEL_RULE_ID_PREFIX)
            .filter(|id| !id.is_empty())
        else {
            return;
        };
        self.key_ranges.remove(id);
        self.label_rule_buffer.remove(id);
        self.clear_group_cache(id);
        if let Some(g) = self.groups.get_mut(id) {
            g.info.range_count = 0;
            g.label_rule = None;
            g.affinity_version = g.affinity_version.wrapping_add(1);
        }
    }
}
