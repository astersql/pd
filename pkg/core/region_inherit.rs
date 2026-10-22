// Copyright 2026 AsterSQL.
//! Region provenance and inheritance over immutable bucket snapshots.
use crate::RegionInfo;
use std::sync::Arc;

/// Origin of the region metadata, matching Go's zero-value storage source.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RegionSource {
    #[default]
    Storage,
    Sync,
    Heartbeat,
}
/// Bucket boundaries reported separately from a region heartbeat.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegionBuckets {
    pub region_id: u64,
    pub version: u64,
    pub keys: Vec<Vec<u8>>,
}
/// Heartbeat bucket metadata; its region ID comes from its owning region.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegionBucketMeta {
    pub version: u64,
    pub keys: Vec<Vec<u8>>,
}
impl RegionInfo {
    /// Return metadata provenance.
    pub fn source(&self) -> RegionSource {
        self.source
    }
    /// Set provenance when assembling a storage or sync snapshot.
    pub fn set_source(&mut self, source: RegionSource) {
        self.source = source;
    }
    /// Whether metadata came from storage and may be stale.
    pub fn loaded_from_storage(&self) -> bool {
        self.source == RegionSource::Storage
    }
    /// Whether metadata came from the region syncer.
    pub fn loaded_from_sync(&self) -> bool {
        self.source == RegionSource::Sync
    }
    /// Return the shared report snapshot, independently of heartbeat metadata.
    pub fn report_buckets(&self) -> Option<&Arc<RegionBuckets>> {
        self.report_buckets.as_ref()
    }
    /// Return effective buckets, preferring heartbeat metadata.
    pub fn buckets(&self) -> Option<RegionBuckets> {
        if let Some(meta) = &self.bucket_meta {
            Some(RegionBuckets {
                region_id: self.meta.id,
                version: meta.version,
                keys: meta.keys.clone(),
            })
        } else {
            self.report_buckets.as_deref().cloned()
        }
    }
    /// Correct uninitialized size/keys and optionally inherit missing report buckets.
    /// A size of one denotes a genuinely empty region and is never corrected.
    /// Provenance, flow counters and heartbeat bucket metadata remain local.
    pub fn inherit(&mut self, origin: Option<&Self>, bucket_enabled: bool) {
        if self.stats.approximate_size == 0 {
            if let Some(origin) = origin {
                self.stats.approximate_size = origin.stats.approximate_size;
                self.stats.approximate_keys = origin.stats.approximate_keys;
            } else {
                self.stats.approximate_size = 1;
            }
        }
        if bucket_enabled && self.report_buckets.is_none() {
            if let Some(origin) = origin {
                self.report_buckets = origin.report_buckets.clone();
            }
        }
    }
}
