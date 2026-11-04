// Copyright 2026 AsterSQL.
use crate::{Controller, PlanBatch, RecoveryError, RecoveryIndex};
use astersql_pd_core::StoresInfo;
impl Controller {
    pub fn generate_tombstone_tiflash_learner_plan(
        &self,
        index: &RecoveryIndex,
        stores: &StoresInfo,
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
            let leader = index.select_leader(region.id, stores)?.ok_or_else(|| {
                RecoveryError::Backend(format!("can't select leader for region {}", region.id))
            })?;
            let store = stores
                .get_store(leader.store_id)
                .ok_or(RecoveryError::StoreNotFound(leader.store_id))?;
            if store.is_tiflash_write() {
                batch
                    .plans
                    .entry(leader.store_id)
                    .or_default()
                    .tombstones
                    .push(region.id);
                batch.record_affected_region(region);
            }
        }
        Ok(batch)
    }
}
