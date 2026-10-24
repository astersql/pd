// Copyright 2026 AsterSQL.
//! TiKV storage configuration values without transport or background work.
use crate::null_default;
use astersql_pd_typeutil::parse_mb_from_text;
use serde::{Deserialize, Serialize};

/// TiKV raftstore v2 engine identifier.
pub const RAFTSTORE_V2: &str = "raft-kv2";
/// Coprocessor split and bucket settings, with Go zero-value defaults.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Coprocessor {
    #[serde(deserialize_with = "null_default")]
    pub region_max_size: String,
    #[serde(deserialize_with = "null_default")]
    pub region_split_size: String,
    #[serde(deserialize_with = "null_default")]
    pub region_max_keys: i64,
    #[serde(deserialize_with = "null_default")]
    pub region_split_keys: i64,
    #[serde(deserialize_with = "null_default")]
    pub enable_region_bucket: bool,
    #[serde(deserialize_with = "null_default")]
    pub region_bucket_size: String,
}
/// TiKV storage engine settings.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Storage {
    #[serde(deserialize_with = "null_default")]
    pub engine: String,
}
/// TiKV unified read pool settings.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct UnifiedReadPool {
    #[serde(deserialize_with = "null_default")]
    pub max_thread_count: u64,
}
/// TiKV read pools.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReadPool {
    #[serde(deserialize_with = "null_default")]
    pub unified: UnifiedReadPool,
}
/// Store configuration and derived MiB caches. Equality ignores derived caches.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StoreConfig {
    #[serde(deserialize_with = "null_default")]
    pub coprocessor: Coprocessor,
    #[serde(deserialize_with = "null_default")]
    pub storage: Storage,
    #[serde(deserialize_with = "null_default")]
    pub readpool: ReadPool,
    #[serde(skip)]
    pub region_max_size_mb: u64,
    #[serde(skip)]
    pub region_split_size_mb: u64,
    #[serde(skip)]
    pub region_bucket_size_mb: u64,
}
impl PartialEq for StoreConfig {
    fn eq(&self, other: &Self) -> bool {
        self.coprocessor == other.coprocessor
            && self.storage == other.storage
            && self.readpool == other.readpool
    }
}
impl Eq for StoreConfig {}
/// A split would create a region eligible for merging again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeAgainError {
    Size,
    Keys,
}
impl std::fmt::Display for MergeAgainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Size => "[PD:checker:ErrCheckerMergeAgain]region will be merged again, the smallest region of the split regions is less than max-merge-region-size, it will be merged again",
            Self::Keys => "[PD:checker:ErrCheckerMergeAgain]region will be merged again, the smallest region of the split regions is less than max-merge-region-keys",
        })
    }
}
impl std::error::Error for MergeAgainError {}
impl StoreConfig {
    /// Refresh derived size caches after changing raw configuration.
    pub fn adjust(&mut self) {
        self.region_max_size_mb = parse_mb_from_text(&self.coprocessor.region_max_size, 144);
        self.region_split_size_mb = parse_mb_from_text(&self.coprocessor.region_split_size, 96);
        self.region_bucket_size_mb = parse_mb_from_text(&self.coprocessor.region_bucket_size, 96);
    }
    pub fn get_region_max_size(&self) -> u64 {
        if self.coprocessor.region_max_size.is_empty() {
            144
        } else {
            self.region_max_size_mb
        }
    }
    pub fn get_region_split_size(&self) -> u64 {
        if self.coprocessor.region_split_size.is_empty() {
            96
        } else {
            self.region_split_size_mb
        }
    }
    pub fn get_region_max_keys(&self) -> u64 {
        if self.coprocessor.region_max_keys == 0 {
            1440000
        } else {
            self.coprocessor.region_max_keys as u64
        }
    }
    pub fn get_region_split_keys(&self) -> u64 {
        if self.coprocessor.region_split_keys == 0 {
            960000
        } else {
            self.coprocessor.region_split_keys as u64
        }
    }
    pub fn is_enable_region_bucket(&self) -> bool {
        self.coprocessor.enable_region_bucket
    }
    pub fn set_region_bucket_enabled(&mut self, enabled: bool) {
        self.coprocessor.enable_region_bucket = enabled;
    }
    pub fn get_region_bucket_size(&self) -> u64 {
        if !self.is_enable_region_bucket() {
            0
        } else if self.coprocessor.region_bucket_size.is_empty() {
            96
        } else {
            self.region_bucket_size_mb
        }
    }
    pub fn is_raft_kv2(&self) -> bool {
        self.storage.engine == RAFTSTORE_V2
    }
    pub fn get_unified_read_pool_max_thread_count(&self) -> u64 {
        self.readpool.unified.max_thread_count
    }
    pub fn check_region_size(&self, size: u64, merge_size: u64) -> Result<(), MergeAgainError> {
        let split = self.get_region_split_size();
        if size < self.get_region_max_size() || split == 0 {
            return Ok(());
        }
        let small = size % split;
        if small != 0 && small <= merge_size {
            Err(MergeAgainError::Size)
        } else {
            Ok(())
        }
    }
    pub fn check_region_keys(&self, keys: u64, merge_keys: u64) -> Result<(), MergeAgainError> {
        if keys < self.get_region_max_keys() {
            return Ok(());
        }
        let small = keys % self.get_region_split_keys();
        if small != 0 && small <= merge_keys {
            Err(MergeAgainError::Keys)
        } else {
            Ok(())
        }
    }
}
impl std::fmt::Display for StoreConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&serde_json::to_string_pretty(self).map_err(|_| std::fmt::Error)?)
    }
}
