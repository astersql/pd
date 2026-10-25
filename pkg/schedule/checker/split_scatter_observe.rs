// Copyright 2026 AsterSQL.
//! Split heartbeat observation and batch identity, independent of dispatch policy.
use astersql_pd_codec::{TXN_KEYSPACE_MODE_PREFIX, encode_keyspace_boundary};
use astersql_pd_core::{RegionInfo, RegionsInfo};
use std::{collections::BTreeMap, time::Duration};

/// Identity of one source/child result in a recorded split batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitScatterPendingItem {
    pub region_id: u64,
    pub group: String,
    pub source_region_id: u64,
    pub split_version: u64,
    /// Caller supplied deadline on a monotonic clock, also part of batch identity.
    pub expire_at: Duration,
}

/// Observation of a single result; missing heartbeats remain distinguishable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitScatterObservation {
    MissingResult,
    AwaitingResult,
    StaleResult,
    MissingSource,
    AwaitingSource,
    Ready,
}

/// Pure version gate: an absent region/epoch has version zero, as in Go.
pub fn split_scatter_region_version(region: Option<&RegionInfo>) -> u64 {
    region.and_then(|r| r.meta().epoch).map_or(0, |e| e.version)
}

/// Observe a pending result against the real region registry.
/// A newer source permits an unchanged sibling; a newer result is stale.
pub fn observe_split_scatter_result(
    pending: &SplitScatterPendingItem,
    regions: &RegionsInfo,
) -> SplitScatterObservation {
    let Some(result) = regions.get_region(pending.region_id) else {
        return SplitScatterObservation::MissingResult;
    };
    match split_scatter_region_version(Some(&result)).cmp(&pending.split_version) {
        std::cmp::Ordering::Less => return SplitScatterObservation::AwaitingResult,
        std::cmp::Ordering::Greater => return SplitScatterObservation::StaleResult,
        std::cmp::Ordering::Equal => {}
    }
    let Some(source) = regions.get_region(pending.source_region_id) else {
        return SplitScatterObservation::MissingSource;
    };
    if split_scatter_region_version(Some(&source)) < pending.split_version {
        return SplitScatterObservation::AwaitingSource;
    }
    SplitScatterObservation::Ready
}

/// Recorded batch identities. Dispatch backoff, expiration and operator creation
/// belong to the retry controller; observation alone never consumes a result.
#[derive(Default)]
pub struct SplitScatterObserver {
    pending: BTreeMap<u64, SplitScatterPendingItem>,
}
impl SplitScatterObserver {
    /// Record source and children together. Empty or disabled batches are ignored.
    /// The supplied split version is preserved, except zero is normalized to one.
    pub fn record_batch(
        &mut self,
        source: u64,
        version: u64,
        children: &[u64],
        expire_at: Duration,
        enabled: bool,
    ) {
        if children.is_empty() || !enabled {
            return;
        }
        let group = format!("split-scatter-{source}-{}", children[0]);
        for id in children.iter().copied().chain(std::iter::once(source)) {
            self.pending.insert(
                id,
                SplitScatterPendingItem {
                    region_id: id,
                    group: group.clone(),
                    source_region_id: source,
                    split_version: version.max(1),
                    expire_at,
                },
            );
        }
    }
    /// Read an owned identity for rechecking after potentially slow cluster reads.
    pub fn pending(&self, region_id: u64) -> Option<SplitScatterPendingItem> {
        self.pending.get(&region_id).cloned()
    }
    /// Current recorded result count (source included).
    pub fn len(&self) -> usize {
        self.pending.len()
    }
    /// Whether there are no pending results.
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
    /// Recheck all identity fields, including the deadline of repeated batches.
    pub fn is_current(&self, expected: &SplitScatterPendingItem) -> bool {
        self.pending.get(&expected.region_id) == Some(expected)
    }
    /// Remove only the observed batch identity; stale snapshots cannot remove replacements.
    pub fn remove_if_current(&mut self, expected: &SplitScatterPendingItem) -> bool {
        if !self.is_current(expected) {
            return false;
        }
        self.pending.remove(&expected.region_id);
        true
    }
    /// Ready items in region-ID order, without consuming or requiring all siblings.
    pub fn ready(&self, regions: &RegionsInfo) -> Vec<SplitScatterPendingItem> {
        self.pending
            .values()
            .filter(|p| observe_split_scatter_result(p, regions) == SplitScatterObservation::Ready)
            .cloned()
            .collect()
    }
}

/// Transactional keyspace grouping requires exact starts at both encoded bounds;
/// a region merely containing a boundary does not establish the keyspace.
pub fn has_split_scatter_txn_keyspace_bounds(regions: &RegionsInfo, id: u32) -> bool {
    let left = encode_keyspace_boundary(TXN_KEYSPACE_MODE_PREFIX, id);
    let right = encode_keyspace_boundary(TXN_KEYSPACE_MODE_PREFIX, id.wrapping_add(1));
    let snapshot = regions.get_regions();
    [left, right].iter().all(|bound| {
        snapshot
            .iter()
            .any(|r| r.meta().key_range.start_key == bound.as_slice())
    })
}
