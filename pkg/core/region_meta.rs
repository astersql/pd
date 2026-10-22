// Copyright 2026 AsterSQL.
//! Owned region identity and byte ranges, independent of protobuf.

use crate::Peer;

/// A region identifier, preserving the full unsigned protocol domain.
pub type RegionId = u64;

/// Configuration and data versions; ordering/staleness is a separate policy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RegionEpoch {
    pub conf_ver: u64,
    pub version: u64,
}

/// A half-open byte range. An empty end key denotes positive infinity.
/// Raw bounds are preserved, including equal or reversed finite bounds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyRange {
    pub start_key: Vec<u8>,
    pub end_key: Vec<u8>,
}

impl KeyRange {
    /// Test unsigned lexicographic membership in `[start_key, end_key)`.
    pub fn contains(&self, key: &[u8]) -> bool {
        key >= self.start_key.as_slice()
            && (self.end_key.is_empty() || key < self.end_key.as_slice())
    }
}

/// Owned region metadata. An absent epoch remains distinct from a zero epoch.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegionMeta {
    pub id: RegionId,
    pub key_range: KeyRange,
    pub epoch: Option<RegionEpoch>,
    pub peers: Vec<Peer>,
}
