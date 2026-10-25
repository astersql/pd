// Copyright 2026 AsterSQL.
//! Split-key grouping, preserving the source's per-key validation order.
use astersql_pd_core::{RegionInfo, RegionsInfo};
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_filter::{RegionReplicationContext, is_region_replicated};
use std::{collections::BTreeMap, sync::Arc};

/// A canonical region snapshot and its split keys in input order.
/// Duplicate interior keys are retained, as in Go groupKeysByRegion.
#[derive(Debug)]
pub struct RegionGroupKeys {
    pub region: Arc<RegionInfo>,
    pub keys: Vec<Vec<u8>>,
}

/// Groups keys in their current half-open region ranges, skipping holes,
/// insufficient replication, absent leaders and already existing boundaries.
/// The host receives addSuspectRegions(false, id) for each replication rejection,
/// including repeated keys and region-start keys. No other rejection marks suspect.
/// Group iteration is deterministic by ID; Go leaves map iteration unspecified.
/// This function neither submits operators nor reports split completion.
pub fn group_keys_by_region<C: SharedConfigProvider>(
    regions: &RegionsInfo,
    replication: &RegionReplicationContext<'_, C>,
    keys: &[Vec<u8>],
    mut add_suspect: impl FnMut(bool, u64),
) -> BTreeMap<u64, RegionGroupKeys> {
    let mut groups = BTreeMap::new();
    for key in keys {
        // The canonical scan may return the next range across a hole, so membership
        // must be checked before using the returned snapshot.
        let Some(region) = regions.scan_regions(key, &[], 1).into_iter().next() else {
            continue;
        };
        if !region.meta().key_range.contains(key) {
            continue;
        }
        if !is_region_replicated(replication, &region) {
            add_suspect(false, region.meta().id);
            continue;
        }
        if region.leader().is_none() || region.meta().key_range.start_key == *key {
            continue;
        }
        groups
            .entry(region.meta().id)
            .or_insert_with(|| RegionGroupKeys {
                region,
                keys: Vec::new(),
            })
            .keys
            .push(key.clone());
    }
    groups
}
