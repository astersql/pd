// Copyright 2026 AsterSQL.
//! Admin scatter operation creation and final-distribution accounting.
use crate::{ScatterCounts, ScatterTarget};
use astersql_pd_core::{ENGINE_KEY, ENGINE_TIFLASH, Peer, RegionInfo, StoresInfo};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_operator::{BuilderContext, BuilderError, Operator, RangeOperatorFactory};
use std::collections::BTreeMap;

/// A malformed plan or a typed failure from the canonical operation builder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScatterOperatorError {
    NoLeader,
    InvalidTarget,
    Build(BuilderError),
}
impl std::fmt::Display for ScatterOperatorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "scatter operator error: {self:?}")
    }
}
impl std::error::Error for ScatterOperatorError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Build(error) => Some(error),
            _ => None,
        }
    }
}
impl ScatterCounts {
    /// Record a final layout, partitioning peer counts by engine. As in Go Put,
    /// missing stores are skipped and the leader uses the ordinary engine.
    pub fn record_distribution(
        &mut self,
        stores: &StoresInfo,
        peers: &BTreeMap<u64, Peer>,
        leader: u64,
        group: &str,
    ) {
        if leader == 0 {
            return;
        }
        for peer in peers.values() {
            if let Some(store) = stores.get_store(peer.store_id) {
                let engine = if store.label_value(ENGINE_KEY) == ENGINE_TIFLASH {
                    ENGINE_TIFLASH
                } else {
                    "tikv"
                };
                self.set(
                    group,
                    engine,
                    peer.store_id,
                    false,
                    self.get(group, engine, peer.store_id, false)
                        .wrapping_add(1),
                );
            }
        }
        self.set(
            group,
            "tikv",
            leader,
            true,
            self.get(group, "tikv", leader, true).wrapping_add(1),
        );
    }
}
fn valid_target(region: &RegionInfo, target: &ScatterTarget) -> bool {
    if region.meta().peers.len() != target.peers.len()
        || !target.peers.contains_key(&target.leader)
        || target.leader == 0
    {
        return false;
    }
    let mut remaining: Vec<_> = region
        .meta()
        .peers
        .iter()
        .map(|p| (p.role, p.is_witness))
        .collect();
    for (&id, p) in &target.peers {
        if id == 0 || id != p.store_id {
            return false;
        }
        let Some(index) = remaining
            .iter()
            .position(|kind| *kind == (p.role, p.is_witness))
        else {
            return false;
        };
        remaining.swap_remove(index);
    }
    true
}
/// Consume a selected admin layout using the real operator builder. The host
/// serializes access to counts across planning and this commit. No controller
/// admission or internal seeded-round state is performed here.
/// On no-op record the current layout; on error record only the original layout.
pub async fn create_scatter_operator<V, F>(
    context: BuilderContext<'_, V>,
    region: &RegionInfo,
    target: &ScatterTarget,
    group: &str,
    skip_store_limit: bool,
    supports_conf_change_v2: bool,
    counts: &mut ScatterCounts,
    allocate: F,
) -> Result<Option<Operator>, ScatterOperatorError>
where
    F: FnMut() -> Result<u64, BuilderError>,
{
    let leader = region.leader().ok_or(ScatterOperatorError::NoLeader)?;
    let original: BTreeMap<_, _> = region
        .meta()
        .peers
        .iter()
        .map(|p| (p.store_id, p.clone()))
        .collect();
    let stores = context.stores;
    if !valid_target(region, target) {
        counts.record_distribution(stores, &original, leader.store_id, group);
        return Err(ScatterOperatorError::InvalidTarget);
    }
    if original.keys().eq(target.peers.keys()) && leader.store_id == target.leader {
        counts.record_distribution(stores, &target.peers, target.leader, group);
        return Ok(None);
    }
    let result =
        RangeOperatorFactory::new("scatter-region", context, region, supports_conf_change_v2)
            .create_scatter(
                target.peers.clone(),
                target.leader,
                0,
                skip_store_limit,
                true,
                allocate,
            )
            .await;
    match result {
        Ok(op) => {
            counts.record_distribution(stores, &target.peers, target.leader, group);
            op.additional_info().set("group", group);
            op.additional_info().set(
                "leader-picked-count",
                target.leader_picked_count.to_string(),
            );
            op.set_priority_level(PriorityLevel::HIGH);
            Ok(Some(op))
        }
        Err(error) => {
            counts.record_distribution(stores, &original, leader.store_id, group);
            Err(ScatterOperatorError::Build(error))
        }
    }
}
