// Copyright 2026 AsterSQL.
use crate::async_wait::store_quorum;
use crate::{ModeError, ModeManager};
use astersql_pd_core::{RegionInfo, RegionsInfo, StoreInfo};
use astersql_pd_schedule_placement::Rule;
use std::sync::Arc;

/// Volatile scan cursor and sample, reset only after entering a new recovery state.
#[derive(Default)]
pub(crate) struct RecoveryProgress {
    key: Vec<u8>,
    count: i64,
    sample_recovered: i64,
    sample_total: i64,
    total: i64,
}

impl RecoveryProgress {
    fn recovered(region: &RegionInfo, start: &[u8], state_id: u64) -> bool {
        // INTEGRITY_OVER_LABEL is the canonical domain replication state value 2.
        start >= region.meta().key_range.start_key.as_slice()
            && region.replication_status() == (2, state_id)
    }

    fn update(&mut self, regions: &RegionsInfo, state_id: u64) {
        while !self.key.is_empty() || self.count == 0 {
            let batch = regions.scan_regions(&self.key, b"", 1024);
            if batch.is_empty() {
                return;
            }
            for (i, region) in batch.iter().enumerate() {
                if Self::recovered(region, &self.key, state_id) {
                    self.key = region.meta().key_range.end_key.clone();
                    self.count += 1;
                    continue;
                }
                let mut sample = batch[i..].to_vec();
                if sample.len() < 512 {
                    let end = &sample.last().unwrap().meta().key_range.end_key;
                    if !end.is_empty() {
                        sample.extend(regions.scan_regions(end, b"", 512));
                    }
                }
                self.sample_recovered = 0;
                let mut key = self.key.as_slice();
                for region in &sample {
                    if Self::recovered(region, key, state_id) {
                        self.sample_recovered += 1;
                    }
                    key = &region.meta().key_range.end_key;
                }
                self.sample_total = sample.len() as i64;
                self.total = regions.total_region_count() as i64;
                return;
            }
        }
    }

    fn estimate(&mut self) -> f32 {
        if self.key.is_empty() && self.count > 0 {
            return 1.0;
        }
        if self.sample_total <= self.sample_recovered {
            self.sample_total = self.sample_recovered + 1;
        }
        let unchecked = (self.total - self.count).max(self.sample_total);
        let total = self.count + unchecked;
        let recovered = unchecked as f32 * self.sample_recovered as f32 / self.sample_total as f32;
        (self.count as f32 + recovered) / total as f32
    }
}

impl ModeManager {
    /// Evaluate async recovery timeout or an acknowledged change of available stores.
    pub async fn tick_async(
        &self,
        stores: &[Arc<StoreInfo>],
        rules: &[Rule],
        now: i128,
    ) -> Result<bool, ModeError> {
        let mut state = tokio::select! {
            biased;
            _ = self.cancel.cancelled() => return Err(ModeError::Cancelled),
            state = self.state.lock() => state,
        };
        if state.config.replication_mode != "dr-auto-sync" || state.dr_auto_sync.state != "async" {
            return Ok(false);
        }
        let quorum = store_quorum(&state.config.dr_auto_sync, stores, rules, now);
        let elapsed = state.dr_auto_sync.async_start.map_or(0, |start| {
            let nanos =
                start.timestamp() as i128 * 1_000_000_000 + start.timestamp_subsec_nanos() as i128;
            now.saturating_sub(nanos)
                .clamp(i64::MIN as i128, i64::MAX as i128) as i64
        });
        let next = if quorum.can_sync
            && elapsed > state.config.dr_auto_sync.wait_recover_timeout.as_nanos()
        {
            "sync_recover"
        } else {
            if state.dr_auto_sync.available_stores == quorum.primary_up {
                return Ok(false);
            }
            let reports = self.reports.lock().await;
            if !quorum.primary_up.iter().all(|id| {
                reports.get(id).is_some_and(|r| {
                    r.state == "async" && r.state_id == state.dr_auto_sync.state_id
                })
            }) {
                return Ok(false);
            }
            "async"
        };
        let mut candidate = self.new_status(next).await?;
        if next == "async" {
            candidate.available_stores = quorum.primary_up;
        }
        self.persist(&candidate).await?;
        if next == "sync_recover" {
            *self.recovery.lock().await = RecoveryProgress::default();
        }
        state.dr_auto_sync = candidate;
        Ok(true)
    }

    /// Scan the host's canonical region index; only durable state transitions return true.
    pub async fn tick_recover(
        &self,
        stores: &[Arc<StoreInfo>],
        rules: &[Rule],
        regions: &RegionsInfo,
        now: i128,
    ) -> Result<bool, ModeError> {
        let mut state = tokio::select! {
            biased;
            _ = self.cancel.cancelled() => return Err(ModeError::Cancelled),
            state = self.state.lock() => state,
        };
        if state.config.replication_mode != "dr-auto-sync"
            || state.dr_auto_sync.state != "sync_recover"
        {
            return Ok(false);
        }
        let quorum = store_quorum(&state.config.dr_auto_sync, stores, rules, now);
        let next = if !quorum.can_sync && quorum.has_majority {
            "async"
        } else {
            let mut recovery = self.recovery.lock().await;
            recovery.update(regions, state.dr_auto_sync.state_id);
            let progress = recovery.estimate();
            if progress != 1.0 {
                state.dr_auto_sync.recover_progress = progress;
                state.dr_auto_sync.total_regions = recovery.total;
                state.dr_auto_sync.synced_regions = recovery.count;
                return Ok(false);
            }
            "sync"
        };
        let mut candidate = self.new_status(next).await?;
        if next == "async" {
            candidate.available_stores = quorum.primary_up;
        }
        self.persist(&candidate).await?;
        state.dr_auto_sync = candidate;
        Ok(true)
    }
}
