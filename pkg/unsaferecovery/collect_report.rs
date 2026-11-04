// Copyright 2026 AsterSQL.
use crate::{Controller, RecoveryError};
use astersql_pd_core::RegionMeta;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RaftState {
    pub term: u64,
    pub vote: u64,
    pub commit: u64,
    pub last_index: u64,
}
/// Recovery projection of a peer report. Transport-only local storage fields do not enter planning.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PeerReport {
    pub region: Option<RegionMeta>,
    pub raft: Option<RaftState>,
    pub is_force_leader: bool,
    pub has_commit_merge: bool,
    pub applied_index: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoreReport {
    pub peers: Vec<PeerReport>,
    pub step: u64,
}
impl Controller {
    pub fn collect_report(
        &mut self,
        store: u64,
        report: Option<StoreReport>,
    ) -> Result<bool, RecoveryError> {
        if !self.stage.is_running() {
            return Ok(false);
        }
        if self.failed_stores.contains(&store) {
            return Err(RecoveryError::Backend(format!(
                "Receive heartbeat from Failed store {store}"
            )));
        }
        let Some(report) = report else {
            return Ok(false);
        };
        if report.step != self.step {
            return Ok(false);
        }
        let Some(previous) = self.expected_reports.get_mut(&store) else {
            return Ok(false);
        };
        let first = previous.is_none();
        *previous = Some(report);
        if first {
            self.num_stores_reported += 1;
            return Ok(self.num_stores_reported == self.expected_reports.len());
        }
        Ok(false)
    }
    pub fn reported_store_count(&self) -> usize {
        self.num_stores_reported
    }
    /// Ask an unreported store to confirm the exact phase step, with the same retry deadline.
    /// Protocol handlers use `dispatch_plan` to include the complete phase payload.
    pub fn request_report(&mut self, store: u64, now: i128) -> Option<u64> {
        self.dispatch_plan(store, now).map(|plan| plan.step)
    }
    pub fn reports(&self) -> &std::collections::BTreeMap<u64, Option<StoreReport>> {
        &self.expected_reports
    }
    pub fn report(&self, id: u64) -> Option<&StoreReport> {
        self.expected_reports.get(&id).and_then(Option::as_ref)
    }
}
