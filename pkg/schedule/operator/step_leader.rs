// Copyright 2026 AsterSQL.
//! Heartbeat-driven leader transfer, using canonical cluster and command types.
use std::{fmt, time::Duration};

use astersql_pd_core::{BasicCluster, RegionInfo};
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_hbstream::{Operation, TransferLeader as LeaderCommand};
use astersql_pd_storelimit::{REGION_INFLUENCE, Type};

use crate::OpInfluence;

/// A compatible primary target plus ordered alternative leader stores.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransferLeader {
    pub from_store: u64,
    pub to_store: u64,
    pub to_stores: Vec<u64>,
}

/// A rejected candidate, retaining its store and failure category.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeaderCandidateError {
    pub store_id: u64,
    pub reason: LeaderTargetError,
}

/// The validation order matches the Go step and validateStore helper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaderTargetError {
    MissingPeer,
    Learner,
    MissingStore,
    Unhealthy,
    Down,
}
impl fmt::Display for LeaderTargetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::MissingPeer => "peer does not existed",
            Self::Learner => "peer already is a learner",
            Self::MissingStore => "target store does not exist",
            Self::Unhealthy => "target store is unhealthy",
            Self::Down => "target store is down",
        })
    }
}

/// All candidates failed; errors retain candidate order and duplicates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeaderTransferError(pub Vec<LeaderCandidateError>);
impl fmt::Display for LeaderTransferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[")?;
        for (i, e) in self.0.iter().enumerate() {
            if i != 0 {
                f.write_str(" ")?;
            }
            write!(f, "{}", e.reason)?;
        }
        f.write_str("]")
    }
}
impl std::error::Error for LeaderTransferError {}
impl fmt::Display for TransferLeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "transfer leader from store {} to store {}",
            self.from_store, self.to_store
        )
    }
}
impl TransferLeader {
    /// Leadership changes never increase the configuration version.
    pub fn conf_ver_changed(&self, _region: &RegionInfo) -> u64 {
        0
    }

    /// Only a reported heartbeat leader can acknowledge the transfer.
    /// An absent leader has store ID zero, matching Go's nil-safe getter.
    pub fn is_finish(&self, region: &RegionInfo) -> bool {
        let leader = region.leader().map_or(0, |p| p.store_id);
        self.to_stores.contains(&leader) || leader == self.to_store
    }

    /// Accept any peered nonlearner candidate whose store passes health checks.
    /// The clock is an explicit Unix nanosecond boundary shared with core.
    pub fn check_in_progress<C: SharedConfigProvider>(
        &self,
        cluster: &BasicCluster,
        config: &C,
        region: &RegionInfo,
        need_store_health_check: bool,
        now_unix_nanos: i128,
    ) -> Result<(), LeaderTransferError> {
        let mut errors = Vec::with_capacity(self.to_stores.len() + 1);
        for id in self
            .to_stores
            .iter()
            .copied()
            .chain(std::iter::once(self.to_store))
        {
            let reason = match region.get_store_peer(id) {
                None => Some(LeaderTargetError::MissingPeer),
                Some(peer) if peer.is_learner() => Some(LeaderTargetError::Learner),
                Some(_) => match cluster.stores.get_store(id) {
                    None => Some(LeaderTargetError::MissingStore),
                    Some(store)
                        if need_store_health_check && store.is_unhealthy(now_unix_nanos) =>
                    {
                        Some(LeaderTargetError::Unhealthy)
                    }
                    Some(store)
                        if i128::from(store.down_time_nanos(now_unix_nanos))
                            > config.get_max_store_down_time().as_nanos() as i128 =>
                    {
                        Some(LeaderTargetError::Down)
                    }
                    Some(_) => None,
                },
            };
            match reason {
                None => return Ok(()),
                Some(reason) => errors.push(LeaderCandidateError {
                    store_id: id,
                    reason,
                }),
            }
        }
        Err(LeaderTransferError(errors))
    }

    /// Move resource deltas to the primary and charge each distinct target once.
    pub fn influence(&self, influence: &OpInfluence, region: &RegionInfo) {
        let size = region.stats().approximate_size;
        influence.update_store(self.from_store, |s| {
            s.leader_size = s.leader_size.wrapping_sub(size);
            s.leader_count = s.leader_count.wrapping_sub(1);
        });
        influence.update_store(self.to_store, |s| {
            s.leader_size = s.leader_size.wrapping_add(size);
            s.leader_count = s.leader_count.wrapping_add(1);
            s.add_step_cost(
                Type::TRANSFER_LEADER_IN,
                REGION_INFLUENCE[Type::TRANSFER_LEADER_IN.as_i64() as usize],
            );
        });
        for (i, id) in self.to_stores.iter().copied().enumerate() {
            if id != self.to_store && !self.to_stores[..i].contains(&id) {
                influence.update_store(id, |s| {
                    s.add_step_cost(
                        Type::TRANSFER_LEADER_IN,
                        REGION_INFLUENCE[Type::TRANSFER_LEADER_IN.as_i64() as usize],
                    )
                });
            }
        }
    }

    /// Go truncates 0.6 seconds/MiB before wrapping into signed nanoseconds.
    pub fn timeout(&self, region_size: i64) -> Duration {
        let seconds = (0.6 * region_size as f64) as i64;
        Duration::from_nanos(seconds.wrapping_mul(1_000_000_000).max(60_000_000_000) as u64)
    }

    /// Preserve missing peers, duplicate candidates and order; v2 is irrelevant.
    pub fn get_cmd(&self, region: &RegionInfo, _use_conf_change_v2: bool) -> Operation {
        Operation {
            transfer_leader: Some(LeaderCommand {
                peer: region.get_store_peer(self.to_store).cloned(),
                peers: self
                    .to_stores
                    .iter()
                    .map(|id| region.get_store_peer(*id).cloned())
                    .collect(),
            }),
            ..Default::default()
        }
    }
}
