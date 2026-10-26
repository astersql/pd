// Copyright 2026 AsterSQL.
//! Range-local store projections over canonical region indexes.
use astersql_pd_core::{
    BasicCluster as CoreCluster, KeyRange, RegionInfo, RegionsInfo, Rule, StoreInfo, StoresInfo,
    get_store_topo_weight,
};
use astersql_pd_schedule_config::ReplicationConfig;
use astersql_pd_schedule_core::BasicCluster;
use astersql_pd_schedule_placement::{PeerRoleType, RuleManager, match_label_constraints};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

/// Borrowed host generation plus an isolated index of intersecting regions.
/// Root observations and store controls stay on the host, matching Go embedding.
/// Only store projections, leader/follower sampling, average size and network
/// score use the range-local snapshot. No host data or configuration is mutated.
pub struct RangeCluster<'a, C: BasicCluster + ?Sized> {
    parent: &'a C,
    local: CoreCluster,
    tolerant_size_ratio: f64,
}
impl<'a, C: BasicCluster + ?Sized> RangeCluster<'a, C> {
    pub fn new(parent: &'a C, start: &[u8], end: &[u8]) -> Self {
        let mut local = CoreCluster::new();
        for r in parent.scan_regions(start, end, -1) {
            let (o, v, b) = local.regions.set_region(r.clone());
            local.regions.update_sub_tree(r, o.as_deref(), &v, b);
        }
        Self {
            parent,
            local,
            tolerant_size_ratio: 0.0,
        }
    }
    /// Access inherited scheduling services without copying their state.
    pub fn parent(&self) -> &'a C {
        self.parent
    }
    pub fn local_regions(&self) -> &RegionsInfo {
        &self.local.regions
    }
    pub fn set_tolerant_size_ratio(&mut self, ratio: f64) {
        self.tolerant_size_ratio = ratio;
    }
    /// The caller supplies the current host configuration's fallback value.
    pub fn get_tolerant_size_ratio(&self, fallback: f64) -> f64 {
        if self.tolerant_size_ratio != 0.0 {
            self.tolerant_size_ratio
        } else {
            fallback
        }
    }
    pub fn update_store_info(&self, store: Arc<StoreInfo>) -> Arc<StoreInfo> {
        let used = store.used_size() as f64 / (1u64 << 20) as f64;
        if used == 0.0 {
            return store;
        }
        let amplification = store.region_size() as f64 / used;
        let stats = self.local.regions.store_stats(store.id());
        let mut raw = store.store_stats();
        // Go truncates to whole MiB before multiplying, with unsigned wrapping.
        raw.used_size = ((stats.region_size as f64 / amplification) as u64).wrapping_mul(1 << 20);
        raw.available = store.capacity().wrapping_sub(raw.used_size);
        let mut next = store.as_ref().clone();
        next.set_new_store_stats(raw);
        next.set_leader_count(stats.leader as i64);
        next.set_region_count(stats.region as i64);
        next.set_pending_peer_count(stats.pending as i64);
        next.set_leader_size(stats.leader_size);
        next.set_region_size(stats.region_size);
        Arc::new(next)
    }
}
impl<C: BasicCluster + ?Sized> BasicCluster for RangeCluster<'_, C> {
    fn stores(&self) -> &StoresInfo {
        self.parent.stores()
    }
    fn regions(&self) -> &RegionsInfo {
        self.parent.regions()
    }
    fn get_store(&self, id: u64) -> Option<Arc<StoreInfo>> {
        self.parent.get_store(id).map(|s| self.update_store_info(s))
    }
    fn get_stores(&self) -> Vec<Arc<StoreInfo>> {
        self.parent
            .get_stores()
            .into_iter()
            .map(|s| self.update_store_info(s))
            .collect()
    }
    fn get_region_stores(&self, r: &RegionInfo) -> Vec<Arc<StoreInfo>> {
        self.parent
            .get_region_stores(r)
            .into_iter()
            .map(|s| self.update_store_info(s))
            .collect()
    }
    fn get_follower_stores(&self, r: &RegionInfo) -> Vec<Arc<StoreInfo>> {
        self.parent
            .get_follower_stores(r)
            .into_iter()
            .map(|s| self.update_store_info(s))
            .collect()
    }
    fn get_leader_store(&self, r: &RegionInfo) -> Option<Arc<StoreInfo>> {
        self.parent
            .get_leader_store(r)
            .map(|s| self.update_store_info(s))
    }
    fn average_region_size(&self) -> i64 {
        self.local.regions.average_region_size()
    }
    fn avg_network_slow_score(&self, id: u64) -> u64 {
        self.local.stores.avg_network_slow_score(id)
    }
    fn rand_leader_regions(
        &self,
        id: u64,
        ranges: &[KeyRange],
        index: &mut dyn FnMut(usize) -> usize,
        shuffle: &mut dyn FnMut(&mut [usize]),
    ) -> Vec<Arc<RegionInfo>> {
        self.local
            .regions
            .rand_leader_regions(id, ranges, index, shuffle)
    }
    fn rand_follower_regions(
        &self,
        id: u64,
        ranges: &[KeyRange],
        index: &mut dyn FnMut(usize) -> usize,
        shuffle: &mut dyn FnMut(&mut [usize]),
    ) -> Vec<Arc<RegionInfo>> {
        self.local
            .regions
            .rand_follower_regions(id, ranges, index, shuffle)
    }
}

/// Per-store retry budget: 10, 5, 2, 1, with lazy registration and explicit GC.
#[derive(Default)]
pub struct RetryQuota {
    limits: HashMap<u64, usize>,
}
impl RetryQuota {
    pub fn get_limit(&mut self, id: u64) -> usize {
        *self.limits.entry(id).or_insert(10)
    }
    pub fn reset_limit(&mut self, id: u64) {
        self.limits.insert(id, 10);
    }
    pub fn attenuate(&mut self, id: u64) {
        let limit = (self.get_limit(id) / 2).max(1);
        self.limits.insert(id, limit);
    }
    pub fn gc(&mut self, keep: &[u64]) {
        let keep: HashSet<_> = keep.iter().copied().collect();
        self.limits.retain(|id, _| keep.contains(id));
    }
    pub fn tracked_stores(&self) -> usize {
        self.limits.len()
    }
}
/// Malformed query escaping, retaining the offending argument and byte offset.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("query unescape failed at argument {argument}, byte {offset}")]
pub struct KeyRangeDecodeError {
    pub argument: usize,
    pub offset: usize,
}
/// Query-unescape byte ranges in pairs; an unmatched final argument is ignored.
/// Invalid UTF-8 and NUL are valid keys, '+' is space and '%2B' is literal '+'.
pub fn get_key_ranges(args: &[&str]) -> Result<Vec<KeyRange>, KeyRangeDecodeError> {
    fn decode(s: &str, argument: usize) -> Result<Vec<u8>, KeyRangeDecodeError> {
        fn hex(c: u8) -> Option<u8> {
            match c {
                b'0'..=b'9' => Some(c - b'0'),
                b'a'..=b'f' => Some(c - b'a' + 10),
                b'A'..=b'F' => Some(c - b'A' + 10),
                _ => None,
            }
        }
        let b = s.as_bytes();
        let mut result = Vec::with_capacity(b.len());
        let mut i = 0;
        while i < b.len() {
            match b[i] {
                b'+' => result.push(b' '),
                b'%' => {
                    let n = b
                        .get(i + 1)
                        .and_then(|c| hex(*c))
                        .zip(b.get(i + 2).and_then(|c| hex(*c)))
                        .ok_or(KeyRangeDecodeError {
                            argument,
                            offset: i,
                        })?;
                    result.push(n.0 * 16 + n.1);
                    i += 2;
                }
                c => result.push(c),
            }
            i += 1;
        }
        Ok(result)
    }
    let mut result = Vec::with_capacity((args.len() / 2).max(1));
    for (i, pair) in args.chunks_exact(2).enumerate() {
        result.push(KeyRange {
            start_key: decode(pair[0], i * 2)?,
            end_key: decode(pair[1], i * 2 + 1)?,
        });
    }
    if result.is_empty() {
        result.push(KeyRange::default());
    }
    Ok(result)
}
/// Count share in each placement segment, using the canonical rule manager and topology.
/// The explicit config/rule inputs borrow the host's published generation.
pub fn get_count_threshold<C: BasicCluster + ?Sized>(
    cluster: &C,
    stores: &[Arc<StoreInfo>],
    store: &StoreInfo,
    range: &KeyRange,
    kind: Rule,
    config: &ReplicationConfig,
    rules: &RuleManager,
) -> f64 {
    let topo = |stores: &[&StoreInfo], labels: &[String], count: i64| {
        let labels: Vec<_> = labels.iter().map(String::as_str).collect();
        get_store_topo_weight(store, stores, &labels, count)
    };
    if !config.enable_placement_rules {
        let count = cluster.region_count_by_range(&range.start_key, &range.end_key) as i64;
        if kind == Rule::LeaderScatter {
            return count as f64;
        }
        let stores: Vec<_> = stores.iter().map(AsRef::as_ref).collect();
        return count.wrapping_mul(config.max_replicas as i64) as f64
            * topo(
                &stores,
                config.location_labels.as_slice(),
                config.max_replicas as i64,
            );
    }
    let mut total = 0.0;
    let mut start = range.start_key.as_slice();
    let mut bounds = rules.get_split_keys(start, &range.end_key);
    bounds.push(range.end_key.clone());
    for end in &bounds {
        let mut segment = 0.0;
        for rule in rules.get_rules_for_apply_range(start, end) {
            let matches = match kind {
                Rule::LeaderScatter => {
                    matches!(rule.role, PeerRoleType::Leader | PeerRoleType::Voter)
                }
                Rule::LearnerScatter => rule.role == PeerRoleType::Learner,
                _ => true,
            };
            if !matches || !match_label_constraints(Some(store), &rule.label_constraints) {
                continue;
            }
            let eligible: Vec<_> = stores
                .iter()
                .filter(|s| {
                    !s.is_removing()
                        && !s.is_removed()
                        && match_label_constraints(Some(s), &rule.label_constraints)
                })
                .map(AsRef::as_ref)
                .collect();
            if eligible.is_empty() {
                segment = 0.0;
                break;
            }
            let count = cluster.region_count_by_range(start, end) as i64;
            segment += if kind == Rule::LeaderScatter {
                count as f64 / eligible.len() as f64
            } else {
                count.wrapping_mul(rule.count) as f64
                    * topo(&eligible, &rule.location_labels, rule.count)
            };
        }
        total += segment;
        start = end;
    }
    total
}
