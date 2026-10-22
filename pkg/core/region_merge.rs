// Copyright 2026 AsterSQL.
//! Pairwise merge helper for synthetic region sets, independent of merge policy.
use crate::{RegionInfo, RegionMeta};

/// Merge consecutive pairs, retaining the left identity, peers and leader.
/// An odd tail merges with itself. The higher version epoch wins (right on ties),
/// and its input version is incremented with unsigned wrapping, as in Go.
/// Results own metadata snapshots; Go's pointer aliases are represented by the
/// explicit input epoch mutation, rather than shared mutable metadata.
/// Classified peer caches and heartbeat state remain empty like Go's literal.
///
/// # Panics
/// Panics when the selected epoch is absent, matching Go's nil dereference.
pub fn merge_regions(regions: &mut [RegionInfo]) -> Vec<RegionInfo> {
    let mut results = Vec::with_capacity(regions.len().div_ceil(2));
    for left in (0..regions.len()).step_by(2) {
        let right = (left + 1).min(regions.len() - 1);
        let version = |index: usize| regions[index].meta.epoch.map_or(0, |e| e.version);
        let selected = if version(left) > version(right) {
            left
        } else {
            right
        };
        let epoch = regions[selected]
            .meta
            .epoch
            .as_mut()
            .expect("merge requires a selected region epoch");
        epoch.version = epoch.version.wrapping_add(1);
        let epoch = *epoch;
        let mut result = RegionInfo::new(RegionMeta::default(), None, vec![], vec![]);
        result.meta = RegionMeta {
            id: regions[left].meta.id,
            key_range: crate::KeyRange {
                start_key: regions[left].meta.key_range.start_key.clone(),
                end_key: regions[right].meta.key_range.end_key.clone(),
            },
            peers: regions[left].meta.peers.clone(),
            epoch: Some(epoch),
            ..Default::default()
        };
        result.leader = regions[left].leader.clone();
        results.push(result);
    }
    results
}
