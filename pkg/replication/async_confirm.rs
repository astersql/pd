// Copyright 2026 AsterSQL.
use crate::async_wait::store_quorum;
use crate::{ModeError, ModeManager};
use astersql_pd_core::StoreInfo;
use astersql_pd_schedule_placement::Rule;
use std::sync::Arc;

/// Protocol-independent store acknowledgment of a particular DR state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreDRStatus {
    pub state: String,
    pub state_id: u64,
}

impl ModeManager {
    /// Retain the latest report, including an older report that arrived later.
    pub async fn update_store_dr_status(&self, store_id: u64, status: StoreDRStatus) {
        self.reports.lock().await.insert(store_id, status);
    }

    /// Evaluate acknowledgments against the current state and current live primary set.
    pub async fn tick_async_wait(
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
        if state.config.replication_mode != "dr-auto-sync"
            || state.dr_auto_sync.state != "async_wait"
        {
            return Ok(false);
        }
        let quorum = store_quorum(&state.config.dr_auto_sync, stores, rules, now);
        let next = if quorum.can_sync {
            "sync"
        } else if state.dr_auto_sync.available_stores != quorum.primary_up {
            "async_wait"
        } else {
            let reports = self.reports.lock().await;
            if !quorum.primary_up.iter().all(|id| {
                reports.get(id).is_some_and(|r| {
                    r.state == "async_wait" && r.state_id == state.dr_auto_sync.state_id
                })
            }) {
                return Ok(false);
            }
            "async"
        };
        let mut candidate = self.new_status(next).await?;
        if next != "sync" {
            candidate.available_stores = quorum.primary_up;
        }
        self.persist(&candidate).await?;
        state.dr_auto_sync = candidate;
        Ok(true)
    }

    /// Split suppression follows the current mode, policy and published DR state.
    pub async fn is_region_split_paused(&self) -> bool {
        let state = self.state.lock().await;
        state.config.replication_mode == "dr-auto-sync"
            && state.config.dr_auto_sync.pause_region_split
            && state.dr_auto_sync.state != "sync"
    }
}
