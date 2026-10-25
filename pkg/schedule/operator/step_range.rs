// Copyright 2026 AsterSQL.
//! Range-changing steps over owned heartbeat state, independent of transports.
use std::{convert::Infallible, fmt, time::Duration};

use astersql_pd_core::{BasicCluster, RegionInfo, RegionMeta};
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_hbstream::{Merge, Operation, SplitPolicy, SplitRegion as SplitCommand};

use crate::{OpInfluence, TransferLeader};

/// Merge source into target; only the passive target observes completion locally.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MergeRegion {
    pub from_region: Option<RegionMeta>,
    pub to_region: Option<RegionMeta>,
    pub is_passive: bool,
}
/// Split the original byte range with an ordered policy and binary keys.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SplitRegion {
    pub start_key: Vec<u8>,
    pub end_key: Vec<u8>,
    pub policy: SplitPolicy,
    pub split_keys: Vec<Vec<u8>>,
}

fn influence_counts(influence: &OpInfluence, region: &RegionInfo, delta: i64) {
    let leader_id = region.leader().map_or(0, |p| p.id);
    for peer in &region.meta().peers {
        influence.update_store(peer.store_id, |s| {
            s.region_count = s.region_count.wrapping_add(delta);
            if peer.id == leader_id {
                s.leader_count = s.leader_count.wrapping_add(delta);
            }
        });
    }
}
impl MergeRegion {
    pub fn conf_ver_changed(&self, _region: &RegionInfo) -> u64 {
        0
    }
    /// Active merges wait for source disappearance in the controller.
    ///
    /// # Panics
    /// A passive merge requires a target, matching Go's direct target access.
    pub fn is_finish(&self, region: &RegionInfo) -> bool {
        self.is_passive
            && region.meta().key_range
                != self
                    .to_region
                    .as_ref()
                    .expect("passive merge requires a target")
                    .key_range
    }
    /// Range steps do not reject in-progress heartbeat state.
    pub fn check_in_progress<C: SharedConfigProvider>(
        &self,
        _cluster: &BasicCluster,
        _config: &C,
        _region: &RegionInfo,
        _health: bool,
        _now: i128,
    ) -> Result<(), Infallible> {
        Ok(())
    }
    /// Passive merge removes counts only, preserving sizes and scheduling costs.
    pub fn influence(&self, influence: &OpInfluence, region: &RegionInfo) {
        if self.is_passive {
            influence_counts(influence, region, -1);
        }
    }
    /// Preserve signed Go duration overflow when multiplying the fast timeout.
    pub fn timeout_nanos(&self, size: i64) -> i64 {
        (TransferLeader::default().timeout(size).as_nanos() as i64).wrapping_mul(10)
    }
    /// Unsigned duration view preserves the signed source value's bits.
    pub fn timeout(&self, size: i64) -> Duration {
        Duration::from_nanos(self.timeout_nanos(size) as u64)
    }
    /// Only the source sends a command. Target metadata is an owned snapshot.
    pub fn get_cmd(&self, _region: &RegionInfo, _v2: bool) -> Option<Operation> {
        (!self.is_passive).then(|| Operation {
            merge: Some(Merge {
                target: self.to_region.clone(),
            }),
            ..Default::default()
        })
    }
}
impl fmt::Display for MergeRegion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "merge region {} into region {}",
            self.from_region.as_ref().map_or(0, |r| r.id),
            self.to_region.as_ref().map_or(0, |r| r.id)
        )
    }
}
impl SplitRegion {
    pub fn conf_ver_changed(&self, _region: &RegionInfo) -> u64 {
        0
    }
    /// Epoch and identity changes alone do not acknowledge a split.
    pub fn is_finish(&self, region: &RegionInfo) -> bool {
        region.meta().key_range.start_key != self.start_key
            || region.meta().key_range.end_key != self.end_key
    }
    pub fn check_in_progress<C: SharedConfigProvider>(
        &self,
        _cluster: &BasicCluster,
        _config: &C,
        _region: &RegionInfo,
        _health: bool,
        _now: i128,
    ) -> Result<(), Infallible> {
        Ok(())
    }
    /// Each peer contributes one region; the reported leader contributes one leader.
    pub fn influence(&self, influence: &OpInfluence, region: &RegionInfo) {
        influence_counts(influence, region, 1);
    }
    pub fn timeout(&self, size: i64) -> Duration {
        TransferLeader::default().timeout(size)
    }
    /// Preserve key ordering, empty keys, duplicates and unknown policy values.
    pub fn get_cmd(&self, _region: &RegionInfo, _v2: bool) -> Operation {
        Operation {
            split_region: Some(SplitCommand {
                policy: self.policy,
                keys: self.split_keys.clone(),
            }),
            ..Default::default()
        }
    }
}
impl fmt::Display for SplitRegion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("split region with policy ")?;
        match self.policy {
            SplitPolicy::Scan => f.write_str("SCAN"),
            SplitPolicy::Approximate => f.write_str("APPROXIMATE"),
            SplitPolicy::UseKey => f.write_str("USEKEY"),
            SplitPolicy::Unknown(n) => write!(f, "{n}"),
        }
    }
}
