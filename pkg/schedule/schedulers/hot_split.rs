// Copyright 2026 AsterSQL.
//! Hot bucket decisions over canonical immutable region and statistics snapshots.
use crate::{HotRankV1, HotRankV2, HotRegionParams, HotSourceOperation};
use astersql_pd_core::RegionInfo;
use astersql_pd_keyutil::between;
use astersql_pd_schedule_hbstream::SplitPolicy;
use astersql_pd_schedule_operator::{BuilderError, OpKind, Operator, create_split_region_operator};
use astersql_pd_statistics::{Loads, StoreLoad};
use astersql_pd_statistics_buckets::BucketStat;
use astersql_pd_statistics_utils::{RWType, RegionStatKind};

/// Invalid host input or a real operator builder rejection.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum HotSplitError {
    #[error("hot split requires two distinct dimensions in 0..4")]
    InvalidPriorities,
    #[error("hot split requires read or write statistics")]
    InvalidDirection,
    #[error(transparent)]
    Builder(#[from] BuilderError),
}
/// Buckets report bytes, keys and queries, but never CPU.
pub fn is_bucket_load_dim_supported(dim: usize) -> bool {
    dim < 3
}
/// Use the first supported priority, then the second, then byte statistics.
/// Duplicate/unknown dimensions are permitted here to preserve Go's fallback.
pub fn bucket_first_stat(
    priorities: [usize; 2],
    rw: RWType,
) -> Result<RegionStatKind, HotSplitError> {
    let dim = priorities
        .into_iter()
        .find(|&d| is_bucket_load_dim_supported(d))
        .unwrap_or(0);
    rw.region_stats()
        .get(dim)
        .copied()
        .ok_or(HotSplitError::InvalidDirection)
}
/// A split takes precedence over balance ranks only when an operator exists.
/// Empty operators with rank zero can still indicate a required pre-move split.
pub struct HotSplitResult {
    pub operators: Vec<Operator>,
    pub progressive_rank: i32,
    /// Per-region failures do not discard other successfully built splits.
    pub errors: Vec<(u64, HotSplitError)>,
}
impl HotSplitResult {
    fn new(operators: Vec<Operator>) -> Self {
        let progressive_rank = if operators.is_empty() { 0 } else { 5 };
        Self {
            operators,
            progressive_rank,
            errors: Vec::new(),
        }
    }
}
enum SplitRank {
    V1(HotRankV1),
    V2(HotRankV2),
}
/// One round's configuration; no cache ownership, transport, persistence or runtime.
/// Host admission and hot-degree selection happen before supplying bucket stats.
pub struct HotSplit {
    priorities: [usize; 2],
    rw: RWType,
    operation: HotSourceOperation,
    threshold: f64,
    rank: SplitRank,
}
impl HotSplit {
    pub fn new(
        config: HotRegionParams,
        priorities: [usize; 2],
        rw: RWType,
        operation: HotSourceOperation,
    ) -> Result<Self, HotSplitError> {
        if priorities.iter().any(|&d| d >= 4) || priorities[0] == priorities[1] {
            return Err(HotSplitError::InvalidPriorities);
        }
        bucket_first_stat(priorities, rw)?;
        let write_leader = rw == RWType::WRITE && operation == HotSourceOperation::TransferLeader;
        let threshold = config.split_thresholds;
        let rank = if config.rank_formula_version == "v1" {
            SplitRank::V1(
                HotRankV1::new(
                    config,
                    priorities,
                    write_leader,
                    StoreLoad::default(),
                    StoreLoad::default(),
                    StoreLoad::default(),
                )
                .map_err(|_| HotSplitError::InvalidPriorities)?,
            )
        } else {
            SplitRank::V2(
                HotRankV2::new(
                    config,
                    priorities,
                    write_leader,
                    StoreLoad::default(),
                    StoreLoad::default(),
                    StoreLoad::default(),
                )
                .map_err(|_| HotSplitError::InvalidPriorities)?,
            )
        };
        Ok(Self {
            priorities,
            rw,
            operation,
            threshold,
            rank,
        })
    }
    pub fn bucket_first_stat(&self) -> RegionStatKind {
        // Validated at construction; the fallback is always bucket-supported.
        bucket_first_stat(self.priorities, self.rw).expect("validated hot split direction")
    }
    /// Reuse the ranking formula's first/all/any selected-dimension semantics.
    /// Equality to the threshold does not split; CPU remains valid for this gate.
    pub fn too_hot_need_split(&self, store: &Loads, peer: &Loads) -> bool {
        let load = StoreLoad {
            loads: *peer,
            ..Default::default()
        };
        let expect = StoreLoad {
            loads: *store,
            ..Default::default()
        };
        match &self.rank {
            SplitRank::V1(rank) => rank.check_loads(&load, &expect, self.threshold, true),
            SplitRank::V2(rank) => rank.check_loads(&load, &expect, self.threshold, true),
        }
    }
    fn operator(&self, region: &RegionInfo, key: &[u8]) -> Result<Option<Operator>, HotSplitError> {
        let range = &region.meta().key_range;
        if !between(&range.start_key, &range.end_key, key) {
            return Ok(None);
        }
        let desc = if self.rw == RWType::READ {
            "split-hot-read-region"
        } else {
            "split-hot-write-region"
        };
        Ok(Some(create_split_region_operator(
            desc,
            region,
            OpKind::SPLIT,
            SplitPolicy::UseKey,
            vec![key.to_vec()],
        )?))
    }
    /// Retain source order and only buckets with at least one interior endpoint.
    /// A bucket spanning the whole region is intentionally excluded as in Go.
    pub fn by_load(
        &self,
        region: &RegionInfo,
        buckets: &[BucketStat],
    ) -> Result<Option<Operator>, HotSplitError> {
        let range = &region.meta().key_range;
        let stats: Vec<_> = buckets
            .iter()
            .filter(|s| {
                between(&range.start_key, &range.end_key, &s.start_key)
                    || between(&range.start_key, &range.end_key, &s.end_key)
            })
            .collect();
        if stats.len() <= 1 {
            return Ok(None);
        }
        let dim = self.bucket_first_stat().as_i32() as usize;
        let total = stats
            .iter()
            .fold(0u64, |sum, s| sum.wrapping_add(s.loads[dim]));
        let (mut acc, mut index) = (0u64, 0usize);
        while acc < total / 2 && index < stats.len() {
            acc = acc.wrapping_add(stats[index].loads[dim]);
            index += 1;
        }
        if index == 0 {
            return Ok(None);
        }
        let stat = stats[index - 1];
        let key = if between(&range.start_key, &range.end_key, &stat.end_key) {
            &stat.end_key
        } else {
            &stat.start_key
        };
        let op = self.operator(region, key)?;
        if let Some(op) = &op {
            op.additional_info()
                .set("accLoads", acc.wrapping_sub(stat.loads[dim]).to_string());
            op.additional_info().set("totalLoads", total.to_string());
        }
        Ok(op)
    }
    /// Select the upper median of interior keys, using effective core buckets.
    pub fn by_size(&self, region: &RegionInfo) -> Result<Option<Operator>, HotSplitError> {
        let Some(buckets) = region.buckets() else {
            return Ok(None);
        };
        let range = &region.meta().key_range;
        let keys: Vec<_> = buckets
            .keys
            .iter()
            .filter(|key| between(&range.start_key, &range.end_key, key))
            .collect();
        if keys.is_empty() {
            return Ok(None);
        }
        self.operator(region, keys[keys.len() / 2])
    }
    /// Try a hot split before enumerating balance destinations. Empty hot buckets
    /// leave ordinary balance search available. Builder errors remain typed.
    pub fn for_hot_region(
        &self,
        enabled: bool,
        region: &RegionInfo,
        store: &Loads,
        peer: &Loads,
        buckets: &[BucketStat],
    ) -> Result<Option<HotSplitResult>, HotSplitError> {
        if !enabled || !self.too_hot_need_split(store, peer) {
            return Ok(None);
        }
        Ok(self
            .by_load(region, buckets)?
            .map(|op| HotSplitResult::new(vec![op])))
    }
    /// Size splitting precedes any peer move (including its reverse region).
    /// Some(empty) suppresses moving an oversized region with no usable buckets;
    /// None means no size split is required. Leader transfers bypass this gate.
    pub fn before_move(
        &self,
        enabled: bool,
        max_size: i64,
        regions: &[&RegionInfo],
    ) -> Result<Option<HotSplitResult>, HotSplitError> {
        if !enabled || self.operation != HotSourceOperation::MovePeer {
            return Ok(None);
        }
        let oversized: Vec<_> = regions
            .iter()
            .filter(|r| r.stats().approximate_size > max_size)
            .collect();
        if oversized.is_empty() {
            return Ok(None);
        }
        let mut ops = Vec::new();
        let mut errors = Vec::new();
        for region in oversized {
            match self.by_size(region) {
                Ok(Some(op)) => ops.push(op),
                Ok(None) => {}
                Err(error) => errors.push((region.meta().id, error)),
            }
        }
        let mut result = HotSplitResult::new(ops);
        result.errors = errors;
        Ok(Some(result))
    }
}
