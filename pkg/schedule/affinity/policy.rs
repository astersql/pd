// Copyright 2026 AsterSQL.
//! Synchronous availability policy. Hosts supply store/config snapshots and clocks.
use crate::{group::Availability, manager::Manager};
use astersql_pd_core::StoreInfo;
use astersql_pd_errs::{TypedError, catalog::ERR_AFFINITY_GROUP_CONTENT};
use astersql_pd_schedule_config::{REJECT_LEADER, SharedConfigProvider};
use std::collections::BTreeMap;

/// Severity ordering is independent from the first-match store predicate order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StoreCondition {
    Available,
    LeaderEvicted,
    Busy,
    Disconnected,
    Preparing,
    LowSpace,
    Down,
    Removing,
    Removed,
}
impl StoreCondition {
    pub fn group_availability(self) -> Availability {
        match self {
            Self::Available => Availability::Available,
            Self::Down | Self::Removing | Self::Removed => Availability::Expired,
            _ => Availability::Degraded,
        }
    }
    pub fn affects_leader_only(self) -> bool {
        matches!(self, Self::LeaderEvicted | Self::Busy)
    }
}
impl std::fmt::Display for StoreCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Available => "available",
            Self::LeaderEvicted => "leader-evicted",
            Self::Busy => "busy",
            Self::Disconnected => "disconnected",
            Self::Preparing => "preparing",
            Self::LowSpace => "low-space",
            Self::Down => "down",
            Self::Removing => "removing",
            Self::Removed => "removed",
        })
    }
}
/// Collect first matching predicate, preserving Go's priority (eviction before busy).
pub fn collect_unavailable_stores<C: SharedConfigProvider + ?Sized>(
    stores: &[StoreInfo],
    conf: &C,
    now: i128,
) -> BTreeMap<u64, StoreCondition> {
    let mut result = BTreeMap::new();
    for s in stores {
        let condition = if s.is_removed() || s.is_physically_destroyed() {
            StoreCondition::Removed
        } else if s.is_removing() {
            StoreCondition::Removing
        } else if s.is_unhealthy(now) {
            StoreCondition::Down
        } else if !s.allow_leader_transfer_in()
            || conf.check_label_property(REJECT_LEADER, s.labels())
            || s.evicted_as_slow_store()
            || s.evicted_as_stopping_store()
            || s.is_evicted_as_slow_trend()
        {
            StoreCondition::LeaderEvicted
        } else if s.is_busy() {
            StoreCondition::Busy
        } else if s.is_disconnected(now) {
            StoreCondition::Disconnected
        } else if s.is_low_space(conf.get_low_space_ratio()) {
            StoreCondition::LowSpace
        } else if s.is_preparing() {
            StoreCondition::Preparing
        } else {
            continue;
        };
        result.insert(s.id(), condition);
    }
    result
}
/// Learners and leader-only conditions on followers do not affect availability.
pub fn calc_group_availability(
    stores: &BTreeMap<u64, StoreCondition>,
    leader: u64,
    voters: &[u64],
) -> Availability {
    voters
        .iter()
        .filter_map(|id| {
            stores
                .get(id)
                .filter(|c| !c.affects_leader_only() || *id == leader)
        })
        .copied()
        .max()
        .unwrap_or(StoreCondition::Available)
        .group_availability()
}
impl Manager {
    /// One host-driven health check; no private clock, task, or runtime is created.
    pub fn check_groups_availability<C: SharedConfigProvider + ?Sized>(
        &mut self,
        stores: &[StoreInfo],
        conf: &C,
        now_unix_nanos: i128,
        now_unix_seconds: u64,
    ) -> bool {
        if self.groups.is_empty() {
            return false;
        }
        self.apply_store_availability(
            collect_unavailable_stores(stores, conf, now_unix_nanos),
            now_unix_seconds,
        )
    }

    /// Explicit changes reset affinity counts/version even when the state is unchanged.
    pub fn update_group_availability(&mut self, id: &str, availability: Availability, now: u64) {
        if let Some(g) = self.groups.get_mut(id) {
            g.info.set_availability(availability, now);
            g.info.affinity_region_count = 0;
            g.affinity_version = g.affinity_version.wrapping_add(1);
        }
    }
    pub fn expire_affinity_group(&mut self, id: &str, now: u64) {
        self.update_group_availability(id, Availability::Expired, now);
    }
    pub fn degrade_affinity_group(&mut self, id: &str, now: u64) {
        self.update_group_availability(id, Availability::Degraded, now);
    }
    pub fn restore_affinity_group(&mut self, id: &str, now: u64) {
        self.update_group_availability(id, Availability::Available, now);
    }
    /// Only a changed store snapshot triggers transitions. Expired groups stay expired.
    pub fn apply_store_availability(
        &mut self,
        stores: BTreeMap<u64, StoreCondition>,
        now: u64,
    ) -> bool {
        if self.unavailable_stores == stores {
            return false;
        }
        let changes: Vec<_> = self
            .groups
            .iter()
            .filter_map(|(id, g)| {
                let old = g.info.availability(now);
                if old == Availability::Expired {
                    return None;
                }
                let new = calc_group_availability(
                    &stores,
                    g.info.group.leader_store_id,
                    &g.info.group.voter_store_ids,
                );
                (old != new).then(|| (id.clone(), new))
            })
            .collect();
        self.unavailable_stores = stores;
        for (id, new) in changes {
            self.update_group_availability(&id, new, now);
        }
        true
    }
    /// Returns the first unavailable voter in caller order, as the Go content error.
    pub fn check_has_unavailable_store(
        &self,
        leader: u64,
        voters: &[u64],
    ) -> Result<(), TypedError> {
        for id in voters {
            if let Some(c) = self.unavailable_stores.get(id)
                && (!c.affects_leader_only() || *id == leader)
            {
                return Err(ERR_AFFINITY_GROUP_CONTENT.instantiate([format!("store {id} is {c}")]));
            }
        }
        Ok(())
    }
}
