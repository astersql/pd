// Copyright 2026 AsterSQL.
//! Replica deficit inspection with the canonical priority queue and host clock.
use astersql_pd_cache::{PriorityQueue, PriorityQueueItem};
use astersql_pd_core::{RegionInfo, StoreInfo};
use astersql_pd_schedule_config::CheckerConfigProvider;
use astersql_pd_schedule_placement::{PeerRoleType, RegionFit, RuleManager};
use std::sync::{Arc, Mutex};

/// Retry information for an admitted region, independent of its epoch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegionPriorityEntry {
    pub region_id: u64,
    pub attempt: u64,
    pub last_unix_nanos: i128,
}
struct QueuedRegion {
    id: u64,
    retry: Mutex<RegionPriorityEntry>,
}
impl PriorityQueueItem for QueuedRegion {
    fn id(&self) -> u64 {
        self.id
    }
}
/// Thread-safe bounded inspector. The host owns placement, config and time.
pub struct PriorityInspector {
    queue: Mutex<PriorityQueue<QueuedRegion>>,
}
impl Default for PriorityInspector {
    fn default() -> Self {
        Self::new()
    }
}
impl PriorityInspector {
    /// Create an inspector with the Go default capacity of 1280.
    pub fn new() -> Self {
        Self::with_capacity(1280)
    }
    pub(crate) fn with_capacity(capacity: isize) -> Self {
        Self {
            queue: Mutex::new(PriorityQueue::new(capacity)),
        }
    }
    /// Fit real placement rules when enabled; replica mode counts all peers.
    /// Returns the fit for reuse by subsequent checkers, including empty fits.
    pub fn inspect<C: CheckerConfigProvider>(
        &self,
        conf: &C,
        manager: &mut RuleManager,
        stores: &[Arc<StoreInfo>],
        region: &RegionInfo,
        now_unix_nanos: i128,
    ) -> Option<Arc<RegionFit>> {
        let fit = conf
            .is_placement_rules_enabled()
            .then(|| manager.fit_region(stores, region, conf, now_unix_nanos));
        let missing = if let Some(fit) = &fit {
            fit.rule_fits
                .iter()
                .filter(|rf| rf.rule.role != PeerRoleType::Learner)
                .map(|rf| rf.rule.count - rf.peers.len() as i64)
                .sum()
        } else {
            conf.get_max_replicas() - region.meta().peers.len() as i64
        };
        self.update(-missing, region.meta().id, now_unix_nanos);
        fit
    }
    fn update(&self, priority: i64, id: u64, now: i128) {
        let mut queue = self.queue.lock().unwrap();
        if priority >= 0 {
            queue.remove(id);
            return;
        }
        if let Some(entry) = queue.get(id).filter(|e| e.priority == priority) {
            let mut retry = entry.value.retry.lock().unwrap();
            retry.attempt = retry.attempt.saturating_add(1);
            retry.last_unix_nanos = now;
        }
        // Canonical Put retains the old value when an existing ID changes
        // priority. In particular that change does not reset retry state.
        queue.put(
            priority,
            QueuedRegion {
                id,
                retry: Mutex::new(RegionPriorityEntry {
                    region_id: id,
                    attempt: 1,
                    last_unix_nanos: now,
                }),
            },
        );
    }
    /// Strictly after last + attempt * 10 * patrol interval, in priority order.
    pub fn get_priority_regions<C: CheckerConfigProvider>(&self, conf: &C, now: i128) -> Vec<u64> {
        self.queue
            .lock()
            .unwrap()
            .elems()
            .iter()
            .filter_map(|e| {
                let retry = e.value.retry.lock().unwrap();
                let delay = i128::from(retry.attempt)
                    .saturating_mul(10)
                    .saturating_mul(conf.get_patrol_region_interval().as_nanos() as i128);
                (retry.last_unix_nanos.saturating_add(delay) < now).then_some(retry.region_id)
            })
            .collect()
    }
    /// Remove an ID, including a tied ID displaced from sorted traversal.
    pub fn remove_priority_region(&self, id: u64) {
        self.queue.lock().unwrap().remove(id);
    }
    /// Counts sorted priorities, matching Go's priority-only tie equivalence.
    pub fn len(&self) -> usize {
        self.queue.lock().unwrap().len()
    }
    /// Whether sorted traversal contains no region.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Snapshot an ID and retry state without changing its queue position.
    pub fn entry(&self, id: u64) -> Option<(i64, RegionPriorityEntry)> {
        self.queue
            .lock()
            .unwrap()
            .get(id)
            .map(|e| (e.priority, e.value.retry.lock().unwrap().clone()))
    }
}
