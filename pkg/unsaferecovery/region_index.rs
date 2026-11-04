// Copyright 2026 AsterSQL.
use crate::{Controller, PeerReport, RecoveryError};
use astersql_pd_core::{Peer, PeerRole, RegionMeta, StoresInfo};
use std::{collections::BTreeMap, sync::Arc};
#[derive(Clone, Debug)]
pub struct RegionItem {
    pub store_id: u64,
    pub(crate) report: Arc<PeerReport>,
}
impl RegionItem {
    pub fn new(store_id: u64, report: PeerReport) -> Result<Self, RecoveryError> {
        if report.region.is_none() {
            return Err(RecoveryError::Backend(
                "store report is missing region".into(),
            ));
        }
        Ok(Self {
            store_id,
            report: Arc::new(report),
        })
    }
    pub fn region(&self) -> &RegionMeta {
        self.report
            .region
            .as_ref()
            .expect("validated region report")
    }
    pub fn report(&self) -> &PeerReport {
        &self.report
    }
    pub fn is_initialized(&self) -> bool {
        !self.region().peers.is_empty()
    }
    pub fn is_epoch_stale(&self, other: &Self) -> bool {
        epoch(self.region()) < epoch(other.region())
    }
    pub fn is_raft_stale(&self, other: &Self, stores: &StoresInfo) -> Result<bool, RecoveryError> {
        let a = self.report.raft.clone().unwrap_or_default();
        let b = other.report.raft.clone().unwrap_or_default();
        // Go's oracle uses 64-bit int subtraction. Preserve its signed wrap at extremes.
        for difference in [
            a.term.wrapping_sub(b.term) as i64,
            a.last_index
                .max(self.report.applied_index)
                .wrapping_sub(b.last_index.max(other.report.applied_index)) as i64,
            a.last_index.wrapping_sub(b.last_index) as i64,
            a.commit.wrapping_sub(b.commit) as i64,
        ] {
            if difference != 0 {
                return Ok(difference < 0);
            }
        }
        let a_store = stores
            .get_store(self.store_id)
            .ok_or(RecoveryError::StoreNotFound(self.store_id))?;
        let b_store = stores
            .get_store(other.store_id)
            .ok_or(RecoveryError::StoreNotFound(other.store_id))?;
        if !a_store.is_tikv() {
            return Ok(true);
        }
        if !b_store.is_tikv() {
            return Ok(false);
        }
        Ok(self.region().peers.iter().any(|p| {
            p.store_id == self.store_id
                && matches!(p.role, PeerRole::DemotingVoter | PeerRole::Learner)
        }))
    }
}
fn epoch(region: &RegionMeta) -> (u64, u64) {
    region
        .epoch
        .map(|e| (e.version, e.conf_ver))
        .unwrap_or_default()
}
fn contains(region: &RegionMeta, key: &[u8]) -> bool {
    region.key_range.start_key.as_slice() <= key
        && (region.key_range.end_key.is_empty() || key < region.key_range.end_key.as_slice())
}
#[derive(Clone, Debug, Default)]
pub struct RegionIndex {
    by_start: BTreeMap<Vec<u8>, Arc<RegionItem>>,
    by_id: BTreeMap<u64, Arc<RegionItem>>,
}
impl RegionIndex {
    pub fn len(&self) -> usize {
        self.by_start.len()
    }
    pub fn is_empty(&self) -> bool {
        self.by_start.is_empty()
    }
    pub fn contains(&self, id: u64) -> bool {
        self.by_id.contains_key(&id)
    }
    pub fn items(&self) -> impl Iterator<Item = &Arc<RegionItem>> {
        self.by_start.values()
    }
    pub fn overlaps(&self, region: &RegionMeta) -> Vec<Arc<RegionItem>> {
        let start = &region.key_range.start_key;
        let end = &region.key_range.end_key;
        let lower = self
            .by_start
            .range(..=start.clone())
            .next_back()
            .filter(|(_, r)| contains(r.region(), start))
            .map(|(k, _)| k.clone())
            .unwrap_or_else(|| start.clone());
        self.by_start
            .range(lower..)
            .take_while(|(key, _)| end.is_empty() || *key < end)
            .map(|(_, item)| item.clone())
            .collect()
    }
    /// Inputs must arrive by descending epoch, as in build_region_index.
    pub fn insert(&mut self, item: Arc<RegionItem>) -> Result<bool, RecoveryError> {
        if self.contains(item.region().id) {
            return Err(RecoveryError::Backend(format!(
                "region {} shouldn't be updated twice",
                item.region().id
            )));
        }
        let overlaps = self.overlaps(item.region());
        for newer in &overlaps {
            if newer.is_epoch_stale(&item) {
                return Err(RecoveryError::Backend(
                    "recovery index epoch insertion order is stale".into(),
                ));
            }
        }
        if !overlaps.is_empty() {
            return Ok(false);
        }
        self.by_id.insert(item.region().id, item.clone());
        self.by_start
            .insert(item.region().key_range.start_key.clone(), item);
        Ok(true)
    }
}
#[derive(Clone, Debug, Default)]
pub struct RecoveryIndex {
    pub regions: RegionIndex,
    pub peers: BTreeMap<u64, Vec<Arc<RegionItem>>>,
    pub orphaned: BTreeMap<u64, Vec<Peer>>,
}
impl RecoveryIndex {
    pub fn select_leader(
        &self,
        region: u64,
        stores: &StoresInfo,
    ) -> Result<Option<Arc<RegionItem>>, RecoveryError> {
        let mut leader: Option<Arc<RegionItem>> = None;
        for peer in self.peers.get(&region).into_iter().flatten() {
            if match &leader {
                None => true,
                Some(current) => current.is_raft_stale(peer, stores)?,
            } {
                leader = Some(peer.clone());
            }
        }
        Ok(leader)
    }
    pub fn find_overlap_with_empty<'a, 'b>(
        &'a self,
        empty: &'b [RegionMeta],
    ) -> Option<(&'a RegionItem, &'b RegionMeta)> {
        if empty.is_empty() {
            return None;
        }
        for peers in self.peers.values() {
            let mut checked: Option<&RegionMeta> = None;
            for peer in peers {
                if !peer.is_initialized() {
                    continue;
                }
                let region = peer.region();
                if checked.is_some_and(|old| {
                    epoch(old).0 == epoch(region).0 && old.key_range == region.key_range
                }) {
                    continue;
                }
                if let Some(empty) = first_empty_overlap(empty, region) {
                    return Some((peer.as_ref(), empty));
                }
                checked = Some(region);
            }
        }
        None
    }
}
pub fn first_empty_overlap<'a>(
    empty: &'a [RegionMeta],
    region: &RegionMeta,
) -> Option<&'a RegionMeta> {
    let start = &region.key_range.start_key;
    let end = &region.key_range.end_key;
    let i =
        empty.partition_point(|r| !r.key_range.end_key.is_empty() && r.key_range.end_key <= *start);
    empty
        .get(i)
        .filter(|r| end.is_empty() || r.key_range.start_key < *end)
}
impl Controller {
    pub fn build_region_index(&self) -> Result<RecoveryIndex, RecoveryError> {
        let mut result = RecoveryIndex::default();
        for (store, report) in &self.expected_reports {
            let report = report.as_ref().ok_or_else(|| {
                RecoveryError::Backend("recovery store reports are incomplete".into())
            })?;
            for peer in &report.peers {
                let item = Arc::new(RegionItem::new(*store, peer.clone())?);
                result.peers.entry(item.region().id).or_default().push(item);
            }
        }
        let mut newest = Vec::new();
        for peers in result.peers.values() {
            let latest = peers
                .iter()
                .reduce(|a, b| if a.is_epoch_stale(b) { b } else { a })
                .expect("nonempty peer group");
            if !latest.is_initialized() {
                continue;
            }
            newest.push(latest.clone());
            for peer in &latest.region().peers {
                let failed = self.failed_stores.contains(&peer.store_id)
                    || (self.auto_detect && !self.expected_reports.contains_key(&peer.store_id));
                if !failed && !peers.iter().any(|p| p.store_id == peer.store_id) {
                    result
                        .orphaned
                        .entry(latest.region().id)
                        .or_default()
                        .push(peer.clone());
                }
            }
        }
        newest.sort_by(|a, b| epoch(b.region()).cmp(&epoch(a.region())));
        for peer in newest {
            result.regions.insert(peer)?;
        }
        Ok(result)
    }
}
