// Copyright 2026 AsterSQL.
//! Batched region queries with protocol-independent response snapshots.
use crate::*;
use std::{collections::HashMap, sync::Arc};
/// Query payload containing only the fields exposed by Go QueryRegions.
#[derive(Debug)]
pub struct RegionQueryResponse {
    pub region: RegionMeta,
    pub leader: Option<Peer>,
    pub down_peers: Vec<DownPeer>,
    pub pending_peers: Vec<Peer>,
    pub buckets: Option<Arc<RegionBuckets>>,
}
/// Positional key results and deduplicated ID payloads; missing IDs map to None.
#[derive(Debug, Default)]
pub struct RegionQueryResult {
    pub key_ids: Vec<u64>,
    pub prev_key_ids: Vec<u64>,
    pub regions_by_id: HashMap<u64, Option<RegionQueryResponse>>,
}
impl RegionQueryResponse {
    fn from_region(region: &RegionInfo, need_buckets: bool) -> Self {
        Self {
            region: region.meta().clone(),
            leader: region.leader().cloned(),
            down_peers: region.down_peers().to_vec(),
            pending_peers: region.pending_peers().to_vec(),
            buckets: if need_buckets {
                region.report_buckets.clone()
            } else {
                None
            },
        }
    }
}
impl RegionState {
    /// Queries keys, contiguous predecessors and IDs in batches of 128.
    /// Key misses produce zero; explicit missing nonzero IDs retain a None entry.
    /// The shared borrow provides a consistent registry view for this call.
    pub fn query_regions(
        &self,
        keys: &[&[u8]],
        prev_keys: &[&[u8]],
        ids: &[u64],
        need_buckets: bool,
    ) -> RegionQueryResult {
        let mut result = RegionQueryResult::default();
        for (input, previous) in [(keys, false), (prev_keys, true)] {
            let mut positions = Vec::with_capacity(input.len());
            for batch in input.chunks(128) {
                let regions = if previous {
                    self.root.search_by_prev_keys(batch)
                } else {
                    self.root.search_by_keys(batch)
                };
                for region in regions {
                    let id = region.as_ref().map_or(0, |r| r.meta().id);
                    positions.push(id);
                    if id != 0 {
                        result.regions_by_id.entry(id).or_insert_with(|| {
                            region
                                .as_ref()
                                .map(|r| RegionQueryResponse::from_region(r, need_buckets))
                        });
                    }
                }
            }
            if previous {
                result.prev_key_ids = positions;
            } else {
                result.key_ids = positions;
            }
        }
        for batch in ids.chunks(128) {
            for &id in batch {
                if id != 0 {
                    result.regions_by_id.entry(id).or_insert_with(|| {
                        self.regions
                            .get(&id)
                            .map(|r| RegionQueryResponse::from_region(r, need_buckets))
                    });
                }
            }
        }
        result
    }
}
