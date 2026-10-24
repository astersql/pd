// Copyright 2026 AsterSQL.
//! Region anomaly membership with host supplied configuration, snapshots and time.
use astersql_pd_core::{RegionInfo, StoreInfo, is_empty_region};
use astersql_pd_schedule_config::CheckerConfigProvider;
use std::collections::{BTreeMap, BTreeSet};

/// Single classification bit, matching Go RegionStatisticType.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u16)]
pub enum RegionStatisticType {
    MissPeer = 1,
    ExtraPeer = 2,
    DownPeer = 4,
    PendingPeer = 8,
    OfflinePeer = 16,
    LearnerPeer = 32,
    EmptyRegion = 64,
    OversizedRegion = 128,
    UndersizedRegion = 256,
    WitnessLeader = 512,
}
const TYPES: [RegionStatisticType; 10] = [
    RegionStatisticType::MissPeer,
    RegionStatisticType::ExtraPeer,
    RegionStatisticType::DownPeer,
    RegionStatisticType::PendingPeer,
    RegionStatisticType::OfflinePeer,
    RegionStatisticType::LearnerPeer,
    RegionStatisticType::EmptyRegion,
    RegionStatisticType::OversizedRegion,
    RegionStatisticType::UndersizedRegion,
    RegionStatisticType::WitnessLeader,
];
/// Current region lookup, independent of protocol or storage backend.
pub trait RegionInfoProvider {
    fn get_region(&self, id: u64) -> Option<RegionInfo>;
}
impl<F: Fn(u64) -> Option<RegionInfo>> RegionInfoProvider for F {
    fn get_region(&self, id: u64) -> Option<RegionInfo> {
        self(id)
    }
}
/// Samples for a host-owned histogram; first observations produce no sample.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RegionAnomalyDurations {
    pub miss_voter_seconds: Option<i64>,
    pub down_peer_seconds: Option<i64>,
}
/// Placement rules require the rule integration, rather than replica-count fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlacementRulesRequired;
impl std::fmt::Display for PlacementRulesRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("placement rules require rule classification integration")
    }
}
impl std::error::Error for PlacementRulesRequired {}
#[derive(Default)]
struct Entry {
    bits: u16,
    miss_start: i64,
    down_start: i64,
}
/// Owned anomaly index. The host serializes mutation and supplies Unix seconds.
#[derive(Default)]
pub struct RegionStatistics {
    index: BTreeMap<u64, Entry>,
    stats: BTreeMap<RegionStatisticType, BTreeSet<u64>>,
}
impl RegionStatistics {
    /// Create an empty index.
    pub fn new() -> Self {
        Self::default()
    }
    /// Whether any observation, including a healthy zero bitmap, was recorded.
    pub fn is_observed(&self, id: u64) -> bool {
        self.index.contains_key(&id)
    }
    /// Return the recorded bitset, or None for an unobserved region.
    pub fn bitmap(&self, id: u64) -> Option<u16> {
        self.index.get(&id).map(|e| e.bits)
    }
    /// Test a single classification bit.
    pub fn is_region_stats_type(&self, id: u64, typ: RegionStatisticType) -> bool {
        self.bitmap(id).is_some_and(|bits| bits & typ as u16 != 0)
    }
    /// Return classified IDs in deterministic order.
    pub fn region_ids_by_type(&self, typ: RegionStatisticType) -> Vec<u64> {
        self.stats
            .get(&typ)
            .map(|ids| ids.iter().copied().collect())
            .unwrap_or_default()
    }
    /// Query current snapshots and skip IDs removed from the host's region cache.
    pub fn get_region_stats_by_type(
        &self,
        typ: RegionStatisticType,
        provider: &impl RegionInfoProvider,
    ) -> Vec<RegionInfo> {
        self.region_ids_by_type(typ)
            .into_iter()
            .filter_map(|id| provider.get_region(id))
            .collect()
    }
    /// Remove every classification and timestamp for an overlapping or deleted region.
    pub fn clear_defunct_region(&mut self, id: u64) {
        if let Some(old) = self.index.remove(&id) {
            for typ in TYPES {
                if old.bits & typ as u16 != 0 {
                    if let Some(ids) = self.stats.get_mut(&typ) {
                        ids.remove(&id);
                    }
                }
            }
        }
    }
    /// Match the Go heartbeat trigger subset using the current configuration.
    pub fn region_stats_need_update(
        &self,
        region: &RegionInfo,
        conf: &impl CheckerConfigProvider,
    ) -> bool {
        if !self.is_observed(region.meta().id) {
            return true;
        }
        let bits = special_bits(region, conf);
        TYPES
            .into_iter()
            .filter(|t| SPECIAL & *t as u16 != 0)
            .any(|t| self.is_region_stats_type(region.meta().id, t) != (bits & t as u16 != 0))
    }
    /// Replace classification and emit cumulative duration samples using host Unix seconds.
    pub fn observe(
        &mut self,
        region: &RegionInfo,
        stores: &[StoreInfo],
        conf: &impl CheckerConfigProvider,
        now_unix_seconds: i64,
    ) -> Result<RegionAnomalyDurations, PlacementRulesRequired> {
        if conf.is_placement_rules_enabled() {
            return Err(PlacementRulesRequired);
        }
        let mut bits = special_bits(region, conf);
        for (typ, yes) in [
            (
                RegionStatisticType::MissPeer,
                (region.meta().peers.len() as i64) < conf.get_max_replicas(),
            ),
            (
                RegionStatisticType::ExtraPeer,
                (region.meta().peers.len() as i64) > conf.get_max_replicas(),
            ),
            (
                RegionStatisticType::OfflinePeer,
                stores.iter().any(|store| {
                    store.is_removing() && region.get_store_peer(store.id()).is_some()
                }),
            ),
            (
                RegionStatisticType::WitnessLeader,
                region.leader().is_some_and(|p| p.is_witness),
            ),
        ] {
            if yes {
                bits |= typ as u16;
            }
        }
        let entry = self.index.entry(region.meta().id).or_default();
        let mut durations = RegionAnomalyDurations::default();
        for typ in TYPES {
            let ids = self.stats.entry(typ).or_default();
            if bits & typ as u16 != 0 {
                ids.insert(region.meta().id);
            } else {
                ids.remove(&region.meta().id);
            }
        }
        if bits & RegionStatisticType::MissPeer as u16 != 0 {
            if (region.voters().len() as i64) < conf.get_max_replicas() {
                durations.miss_voter_seconds =
                    sample_duration(&mut entry.miss_start, now_unix_seconds);
            }
        } else {
            entry.miss_start = 0;
        }
        if bits & RegionStatisticType::DownPeer as u16 != 0 {
            durations.down_peer_seconds = sample_duration(&mut entry.down_start, now_unix_seconds);
        } else {
            entry.down_start = 0;
        }
        entry.bits = bits;
        Ok(durations)
    }
}
const SPECIAL: u16 = 4 | 8 | 32 | 64 | 128 | 256;
fn special_bits(region: &RegionInfo, conf: &impl CheckerConfigProvider) -> u16 {
    let mut bits = 0;
    for (typ, yes) in [
        (
            RegionStatisticType::DownPeer,
            !region.down_peers().is_empty(),
        ),
        (
            RegionStatisticType::PendingPeer,
            !region.pending_peers().is_empty(),
        ),
        (
            RegionStatisticType::LearnerPeer,
            !region.learners().is_empty(),
        ),
        (
            RegionStatisticType::EmptyRegion,
            is_empty_region(
                region.stats().approximate_size,
                region.stats().approximate_keys,
            ),
        ),
        (
            RegionStatisticType::OversizedRegion,
            region.is_oversized(
                conf.get_region_max_size() as i64,
                conf.get_region_max_keys() as i64,
            ),
        ),
        (
            RegionStatisticType::UndersizedRegion,
            region.need_merge(
                conf.get_max_merge_region_size() as i64,
                conf.get_max_merge_region_keys() as i64,
            ),
        ),
    ] {
        if yes {
            bits |= typ as u16;
        }
    }
    bits
}

fn sample_duration(start: &mut i64, now: i64) -> Option<i64> {
    if *start == 0 {
        *start = now;
        None
    } else {
        Some(now.wrapping_sub(*start))
    }
}
