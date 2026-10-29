// Copyright 2026 AsterSQL.
//! Host-clock-driven split retries; no private runtime or background tasks.
use crate::{SplitAttempt, SplitSubmissions, group_keys_by_region};
use astersql_pd_core::{BasicCluster, RegionsInfo};
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_filter::RegionReplicationContext;
use astersql_pd_schedule_operator::ControllerAdmission;
use std::collections::BTreeSet;

/// Next action the host must drive, using monotonic milliseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitRetryPhase {
    Ready,
    Watching,
    Backoff { until_ms: u64 },
    Done,
}
/// Observed completion only. IDs are sorted to normalize Go map iteration.
#[derive(Debug, PartialEq, Eq)]
pub struct SplitRetryResult {
    pub percentage: usize,
    pub region_ids: Vec<u64>,
    pub remaining_keys: Vec<Vec<u8>>,
}
/// Preserves SplitRegions' inclusive retry limit and per-attempt cancellation.
/// Cancellation ends observation, not the outer retries or their sleep. The host
/// must continue driving backoff even after cancellation to obtain Go's result.
/// Dropping this value stops work immediately and owns no spawned resources.
pub struct SplitRetry {
    original_len: usize,
    remaining: Vec<Vec<u8>>,
    ids: BTreeSet<u64>,
    retry_limit: i64,
    attempt_index: u64,
    attempt: Option<SplitAttempt>,
    phase: SplitRetryPhase,
    deadline_ms: u64,
    next_tick_ms: u64,
}
impl SplitRetry {
    pub fn new(keys: Vec<Vec<u8>>, retry_limit: impl Into<i64>) -> Self {
        let retry_limit = retry_limit.into();
        Self {
            original_len: keys.len(),
            remaining: keys,
            ids: BTreeSet::new(),
            retry_limit,
            attempt_index: 0,
            attempt: None,
            phase: if retry_limit < 0 {
                SplitRetryPhase::Done
            } else {
                SplitRetryPhase::Ready
            },
            deadline_ms: 0,
            next_tick_ms: 0,
        }
        .with_empty_input()
    }
    fn with_empty_input(mut self) -> Self {
        if self.original_len == 0 {
            self.phase = SplitRetryPhase::Done;
        }
        self
    }
    pub fn phase(&self) -> SplitRetryPhase {
        self.phase
    }
    pub fn remaining_keys(&self) -> &[Vec<u8>] {
        &self.remaining
    }
    /// Group current remaining keys and submit through the canonical controller.
    /// The host dispatches admitted outcomes and retains typed rejection errors.
    /// Returns None unless ready (including calls after completion).
    pub fn start_attempt<C: SharedConfigProvider>(
        &mut self,
        now_ms: u64,
        cluster: &BasicCluster,
        controller: &ControllerAdmission,
        replication: &RegionReplicationContext<'_, C>,
        add_suspect: impl FnMut(bool, u64),
    ) -> Option<SplitSubmissions> {
        if self.phase != SplitRetryPhase::Ready {
            return None;
        }
        let groups =
            group_keys_by_region(&cluster.regions, replication, &self.remaining, add_suspect);
        let (attempt, submissions) = SplitAttempt::submit(cluster, controller, groups);
        self.attempt = Some(attempt);
        self.deadline_ms = now_ms.saturating_add(60_000);
        self.next_tick_ms = now_ms.saturating_add(100);
        self.phase = SplitRetryPhase::Watching;
        Some(submissions)
    }
    /// Drive a watch tick, timeout/cancellation, or backoff expiry. If cancellation
    /// and a tick coincide, cancellation wins (Go select permits either order).
    pub fn advance(&mut self, now_ms: u64, cancelled: bool, regions: &RegionsInfo) {
        match self.phase {
            SplitRetryPhase::Watching => {
                let attempt = self.attempt.as_mut().expect("watching owns attempt");
                if !cancelled && now_ms < self.deadline_ms {
                    if now_ms < self.next_tick_ms {
                        return;
                    }
                    attempt.scan(regions);
                    self.next_tick_ms = now_ms.saturating_add(100);
                    if !attempt.finished() {
                        return;
                    }
                }
                let attempt = self.attempt.take().expect("watching owns attempt");
                self.remaining = attempt.results().unprocessed_keys(&self.remaining);
                self.ids.extend(attempt.results().regions().keys().copied());
                if self.remaining.is_empty() {
                    self.phase = SplitRetryPhase::Done;
                } else {
                    let delay = 100_u64
                        .saturating_mul(
                            1_u64
                                .checked_shl(self.attempt_index.try_into().unwrap_or(u32::MAX))
                                .unwrap_or(u64::MAX),
                        )
                        .min(60_000);
                    self.phase = SplitRetryPhase::Backoff {
                        until_ms: now_ms.saturating_add(delay),
                    };
                }
            }
            SplitRetryPhase::Backoff { until_ms } if now_ms >= until_ms => {
                if self.attempt_index >= self.retry_limit as u64 {
                    self.phase = SplitRetryPhase::Done;
                } else {
                    self.attempt_index += 1;
                    self.phase = SplitRetryPhase::Ready;
                }
            }
            _ => {}
        }
    }
    /// Snapshot of settled attempts. An active scan is incorporated when it ends,
    /// including on cancellation or timeout; admission never counts as progress.
    pub fn result(&self) -> SplitRetryResult {
        SplitRetryResult {
            percentage: if self.original_len == 0 {
                0
            } else {
                100 - self.remaining.len() * 100 / self.original_len
            },
            region_ids: self.ids.iter().copied().collect(),
            remaining_keys: self.remaining.clone(),
        }
    }
}
