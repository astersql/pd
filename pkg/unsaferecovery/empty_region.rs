// Copyright 2026 AsterSQL.
use crate::{Controller, PlanBatch, PreparedPlan, RecoveryError, RecoveryIndex, Stage};
use astersql_pd_core::{KeyRange, Peer, RegionEpoch, RegionMeta, StoresInfo};
/// Real host allocator; the domain neither opens storage nor owns an ID counter.
#[async_trait::async_trait]
pub trait RecoveryIdAllocator: Send {
    async fn allocate(&mut self) -> Result<u64, RecoveryError>;
}
async fn append_create(
    c: &Controller,
    pending: &mut Vec<(u64, RegionMeta)>,
    stores: &StoresInfo,
    ids: &mut impl RecoveryIdAllocator,
    start: &[u8],
    end: &[u8],
    preferred: u64,
) -> Result<(), RecoveryError> {
    let store = if preferred != 0
        && stores
            .get_store(preferred)
            .is_some_and(|s| !s.is_tiflash_write())
    {
        preferred
    } else {
        c.expected_reports
            .keys()
            .copied()
            .find(|id| stores.get_store(*id).is_some_and(|s| s.is_tikv()))
            .ok_or_else(|| {
                RecoveryError::Backend(
                    "can't find available store(exclude tiflash) to create new region".into(),
                )
            })?
    };
    let id = ids.allocate().await?;
    let peer = ids.allocate().await?;
    pending.push((
        store,
        RegionMeta {
            id,
            key_range: KeyRange {
                start_key: start.to_vec(),
                end_key: end.to_vec(),
            },
            epoch: Some(RegionEpoch {
                version: 1,
                conf_ver: 1,
            }),
            peers: vec![Peer {
                id: peer,
                store_id: store,
                ..Default::default()
            }],
            ..Default::default()
        },
    ));
    Ok(())
}
impl Controller {
    pub async fn generate_create_empty_region_plan(
        &self,
        index: &RecoveryIndex,
        stores: &StoresInfo,
        ids: &mut impl RecoveryIdAllocator,
    ) -> Result<PlanBatch, RecoveryError> {
        let mut batch = PlanBatch::default();
        if self.recovery_error.is_some() {
            return Ok(batch);
        }
        let mut pending = Vec::new();
        let mut last_end: &[u8] = &[];
        let mut last_store = 0;
        for item in index.regions.items() {
            let region = item.region();
            if region.key_range.start_key != last_end {
                append_create(
                    self,
                    &mut pending,
                    stores,
                    ids,
                    last_end,
                    &region.key_range.start_key,
                    item.store_id,
                )
                .await?;
            }
            last_end = &region.key_range.end_key;
            last_store = item.store_id;
        }
        if !last_end.is_empty() || index.regions.is_empty() {
            append_create(self, &mut pending, stores, ids, last_end, &[], last_store).await?;
        }
        if !self.disable_paranoid_check {
            let empty = pending.iter().map(|(_, r)| r).collect::<Vec<_>>();
            if let Some((peer, empty)) = index.find_overlap_with_empty(&empty) {
                return Err(RecoveryError::Backend(format!(
                    "Find overlap peer {} with newly created empty region {}",
                    astersql_pd_core::region_to_hex_meta(Some(peer.region())),
                    astersql_pd_core::region_to_hex_meta(Some(empty))
                )));
            }
        }
        for (store, region) in pending {
            batch.record_affected_region(&region);
            batch.newly_created_regions.insert(region.id);
            batch.plans.entry(store).or_default().creates.push(region);
        }
        Ok(batch)
    }
    pub async fn prepare_recovery_with_ids(
        &self,
        index: &RecoveryIndex,
        stores: &StoresInfo,
        ids: &mut impl RecoveryIdAllocator,
    ) -> Result<Option<PreparedPlan>, RecoveryError> {
        if let Some(plan) = self.prepare_recovery_plan(index, stores)? {
            return Ok(Some(plan));
        }
        let batch = self
            .generate_create_empty_region_plan(index, stores, ids)
            .await?;
        Ok(batch.has_work().then_some(PreparedPlan {
            stage: Stage::CreateEmptyRegion,
            batch,
        }))
    }
}
