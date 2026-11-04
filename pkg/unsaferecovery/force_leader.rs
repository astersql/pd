// Copyright 2026 AsterSQL.
use crate::{Controller, RecoveryError, RecoveryIndex, Stage};
use astersql_pd_core::{Peer, PeerRole, RegionMeta, StoresInfo};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ForceLeaderPlan {
    pub failed_stores: Vec<u64>,
    pub enter_force_leaders: Vec<u64>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DemoteFailedVoters {
    pub region_id: u64,
    pub failed_voters: Vec<Peer>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecoveryPlan {
    pub creates: Vec<RegionMeta>,
    pub tombstones: Vec<u64>,
    pub demotes: Vec<DemoteFailedVoters>,
    pub force_leader: Option<ForceLeaderPlan>,
    pub step: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlanBatch {
    pub plans: BTreeMap<u64, RecoveryPlan>,
    pub affected_regions: BTreeSet<u64>,
    pub affected_meta_regions: BTreeSet<u64>,
    pub affected_table_ids: BTreeSet<i64>,
}
impl PlanBatch {
    pub fn has_work(&self) -> bool {
        self.plans.values().any(|p| {
            !p.creates.is_empty()
                || !p.tombstones.is_empty()
                || !p.demotes.is_empty()
                || p.force_leader
                    .as_ref()
                    .is_some_and(|f| !f.enter_force_leaders.is_empty())
        })
    }
    pub fn record_affected_region(&mut self, region: &RegionMeta) {
        self.affected_regions.insert(region.id);
        let (meta, table) =
            astersql_pd_codec::meta_or_table(&region.key_range.start_key).unwrap_or_default();
        if meta {
            self.affected_meta_regions.insert(region.id);
        } else if table != 0 {
            self.affected_table_ids.insert(table);
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedPlan {
    pub stage: Stage,
    pub batch: PlanBatch,
}
impl Controller {
    pub fn is_failed(&self, peer: &Peer) -> bool {
        self.failed_stores.contains(&peer.store_id)
            || (self.auto_detect && !self.expected_reports.contains_key(&peer.store_id))
    }
    pub fn can_elect_leader(&self, region: &RegionMeta, only_incoming: bool) -> bool {
        let quorum = |incoming: bool| {
            let mut live = 0usize;
            let mut failed = 0usize;
            for peer in &region.peers {
                let voter = if incoming {
                    matches!(peer.role, PeerRole::Voter | PeerRole::IncomingVoter)
                } else {
                    matches!(peer.role, PeerRole::Voter | PeerRole::DemotingVoter)
                };
                if voter {
                    if self.is_failed(peer) {
                        failed += 1
                    } else {
                        live += 1
                    }
                }
            }
            failed < live
        };
        quorum(true) && (only_incoming || quorum(false))
    }
    pub fn failed_peers(&self, index: &RecoveryIndex, region: &RegionMeta) -> Vec<Peer> {
        if self.can_elect_leader(region, true) {
            return vec![];
        }
        region
            .peers
            .iter()
            .filter(|peer| {
                self.is_failed(peer)
                    || index.orphaned.get(&region.id).is_some_and(|orphans| {
                        orphans
                            .iter()
                            .any(|p| p.id == peer.id || p.store_id == peer.store_id)
                    })
            })
            .cloned()
            .collect()
    }
    pub fn prepare_force_leader(
        &self,
        index: &RecoveryIndex,
        stores: &StoresInfo,
    ) -> Result<Option<PreparedPlan>, RecoveryError> {
        for (commit, stage) in [
            (true, Stage::ForceLeaderForCommitMerge),
            (false, Stage::ForceLeader),
        ] {
            let batch = self.generate_force_leader_plan(index, stores, commit)?;
            if batch.has_work() {
                return Ok(Some(PreparedPlan { stage, batch }));
            }
        }
        Ok(None)
    }
    pub fn generate_force_leader_plan(
        &self,
        index: &RecoveryIndex,
        stores: &StoresInfo,
        for_commit_merge: bool,
    ) -> Result<PlanBatch, RecoveryError> {
        let mut batch = PlanBatch::default();
        if self.recovery_error.is_some() {
            return Ok(batch);
        }
        for item in index.regions.items() {
            let region = item.region();
            if self.can_elect_leader(region, false) {
                continue;
            }
            if index
                .peers
                .get(&region.id)
                .is_some_and(|peers| peers.iter().any(|p| p.report().is_force_leader))
            {
                continue;
            }
            if for_commit_merge && !item.report().has_commit_merge {
                continue;
            }
            if !for_commit_merge && item.report().has_commit_merge {
                return Err(RecoveryError::Backend(format!(
                    "unexpected commit merge state for region {}",
                    region.id
                )));
            }
            let leader = index.select_leader(region.id, stores)?.ok_or_else(|| {
                RecoveryError::Backend(format!("can't select leader for region {}", region.id))
            })?;
            let force = batch
                .plans
                .entry(leader.store_id)
                .or_default()
                .force_leader
                .get_or_insert_with(|| ForceLeaderPlan {
                    failed_stores: self.failed_stores.iter().copied().collect(),
                    enter_force_leaders: vec![],
                });
            if self.auto_detect {
                for peer in self.failed_peers(index, leader.region()) {
                    if !force.failed_stores.contains(&peer.store_id) {
                        force.failed_stores.push(peer.store_id);
                    }
                }
            }
            force.enter_force_leaders.push(region.id);
            batch.record_affected_region(leader.region());
        }
        if batch.has_work() {
            for store in self.expected_reports.keys() {
                batch
                    .plans
                    .entry(*store)
                    .or_default()
                    .force_leader
                    .get_or_insert_with(ForceLeaderPlan::default);
            }
        }
        Ok(batch)
    }
}
