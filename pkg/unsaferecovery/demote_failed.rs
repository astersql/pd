// Copyright 2026 AsterSQL.
use crate::{
    Controller, DemoteFailedVoters, PlanBatch, PreparedPlan, RecoveryError, RecoveryIndex, Stage,
};
use astersql_pd_core::StoresInfo;
impl Controller {
    pub fn generate_demote_failed_voter_plan(
        &self,
        index: &RecoveryIndex,
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
            let leader = index
                .peers
                .get(&region.id)
                .and_then(|peers| peers.iter().find(|p| p.report().is_force_leader));
            let Some(leader) = leader else { continue };
            batch
                .plans
                .entry(leader.store_id)
                .or_default()
                .demotes
                .push(DemoteFailedVoters {
                    region_id: region.id,
                    failed_voters: self.failed_peers(index, leader.region()),
                });
            batch.record_affected_region(leader.region());
        }
        for (store, report) in &self.expected_reports {
            let report = report.as_ref().ok_or_else(|| {
                RecoveryError::Backend("recovery store reports are incomplete".into())
            })?;
            for peer in &report.peers {
                let region = peer.region.as_ref().ok_or_else(|| {
                    RecoveryError::Backend("store report is missing region".into())
                })?;
                if !index.regions.contains(region.id) && !self.can_elect_leader(region, false) {
                    batch
                        .plans
                        .entry(*store)
                        .or_default()
                        .tombstones
                        .push(region.id);
                    batch.record_affected_region(region);
                }
            }
        }
        Ok(batch)
    }
    pub fn prepare_recovery_plan(
        &self,
        index: &RecoveryIndex,
        stores: &StoresInfo,
    ) -> Result<Option<PreparedPlan>, RecoveryError> {
        if let Some(force) = self.prepare_force_leader(index, stores)? {
            return Ok(Some(force));
        }
        let batch = self.generate_demote_failed_voter_plan(index)?;
        Ok(batch.has_work().then_some(PreparedPlan {
            stage: Stage::DemoteFailedVoter,
            batch,
        }))
    }
}
