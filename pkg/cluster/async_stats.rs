// Copyright 2026 AsterSQL.
//! Independent best-effort admissions for one immutable region event.
use astersql_pd_core::{RegionInfo, need_transfer_witness_leader};
use astersql_pd_statistics::{HotCache, HotTaskError, HotTaskReceipt};
use std::sync::{
    Arc,
    mpsc::{SyncSender, TrySendError},
};

/// Witness queue admission; successful admission does not imply scheduling success.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WitnessAdmission {
    Skipped,
    Queued,
    Full,
    Closed,
}
/// Independent directional hot receipts and witness admission outcome.
pub struct AsyncStatsReceipt {
    pub hot: [Result<HotTaskReceipt, HotTaskError>; 3],
    pub witness: WitnessAdmission,
}
/// Enqueues expiration/write flow first, then witness recovery without blocking.
/// Pass the installed canonical scheduler's sender; None means not installed.
/// The event Arc is preserved. Dropped receipts do not cancel admitted hot work.
pub fn handle_stats_async(
    hot: &HotCache,
    witness: Option<&SyncSender<Arc<RegionInfo>>>,
    region: Arc<RegionInfo>,
    now_unix_nanos: i64,
) -> AsyncStatsReceipt {
    let hot = hot.check_region_flow_async(&region, now_unix_nanos);
    let witness = if need_transfer_witness_leader(Some(&region)) {
        match witness {
            None => WitnessAdmission::Skipped,
            Some(sender) => match sender.try_send(region) {
                Ok(()) => WitnessAdmission::Queued,
                Err(TrySendError::Full(_)) => WitnessAdmission::Full,
                Err(TrySendError::Disconnected(_)) => WitnessAdmission::Closed,
            },
        }
    } else {
        WitnessAdmission::Skipped
    };
    AsyncStatsReceipt { hot, witness }
}
