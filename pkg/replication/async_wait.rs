// Copyright 2026 AsterSQL.
use crate::{ModeError, ModeManager};
use astersql_pd_core::StoreInfo;
use astersql_pd_schedule_placement::{PeerRoleType, Rule, match_label_constraints};
use std::sync::Arc;
impl ModeManager {
    /// Evaluate the sync-to-async-wait edge over canonical host snapshots.
    pub async fn tick_sync(
        &self,
        stores: &[Arc<StoreInfo>],
        rules: &[Rule],
        now_unix_nanos: i128,
    ) -> Result<bool, ModeError> {
        let mut state = tokio::select! {
            biased;
            _ = self.cancel.cancelled() => return Err(ModeError::Cancelled),
            state = self.state.lock() => state,
        };
        if state.config.replication_mode != "dr-auto-sync" || state.dr_auto_sync.state != "sync" {
            return Ok(false);
        }
        let config = &state.config.dr_auto_sync;
        let mut groups: [Vec<&StoreInfo>; 4] = Default::default();
        for store in stores.iter().filter(|s| !s.is_removed()) {
            let down = usize::from(
                store.down_time_nanos(now_unix_nanos) >= config.wait_store_timeout.as_nanos(),
            );
            let label = store.label_value(&config.label_key);
            if label == config.primary {
                groups[down].push(store);
            }
            if label == config.dr {
                groups[2 + down].push(store);
            }
        }
        let (mut primary, mut dr, mut total, mut up) = (false, false, 0_i64, 0_i64);
        for rule in rules
            .iter()
            .filter(|r| r.start_key.is_empty() && r.end_key.is_empty())
        {
            if rule.role != PeerRoleType::Learner {
                total = total.wrapping_add(rule.count);
            }
            let p = minimal_up_voters(rule, &groups[0], &groups[1]);
            let d = minimal_up_voters(rule, &groups[2], &groups[3]);
            primary |= p > 0;
            dr |= d > 0;
            up = up.wrapping_add(p.wrapping_add(d).min(rule.count));
        }
        if (primary && dr) || up.wrapping_mul(2) <= total {
            return Ok(false);
        }
        let mut candidate = self.new_status("async_wait").await?;
        candidate.available_stores = groups[0].iter().map(|s| s.id()).collect();
        candidate.available_stores.sort_unstable();
        self.persist(&candidate).await?;
        state.dr_auto_sync = candidate;
        Ok(true)
    }
}

fn minimal_up_voters(rule: &Rule, up: &[&StoreInfo], down: &[&StoreInfo]) -> i64 {
    if rule.role == PeerRoleType::Learner {
        return 0;
    }
    let matching = |stores: &[&StoreInfo]| {
        stores
            .iter()
            .filter(|s| match_label_constraints(Some(s), &rule.label_constraints))
            .count() as i64
    };
    rule.count
        .wrapping_sub(matching(down))
        .max(0)
        .min(matching(up))
}
