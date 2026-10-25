// Copyright 2026 AsterSQL.
//! Admission state shared by a host controller, independent of dispatch and limits.

use crate::{
    CREATED, CancelReason, OpKind, Operator, QueueError, RandBuckets, WaitingOperatorStatus,
};
use astersql_pd_core::BasicCluster;
use std::{
    collections::{HashMap, HashSet},
    fmt,
    sync::{Arc, Mutex},
};

/// A malformed batch or a source-defined admission rejection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmissionError {
    InvalidBatch,
    Rejected(CancelReason),
    Queue(QueueError),
}
impl fmt::Display for AdmissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBatch => f.write_str("invalid operator batch"),
            Self::Rejected(reason) => f.write_str(reason.as_str()),
            Self::Queue(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for AdmissionError {}
impl From<QueueError> for AdmissionError {
    fn from(error: QueueError) -> Self {
        Self::Queue(error)
    }
}
/// The host can record terminal operators and dispatch admitted operators after unlock.
pub struct AdmissionOutcome {
    pub admitted: Vec<Arc<Operator>>,
    pub removed: Vec<Arc<Operator>>,
}
#[derive(Default)]
pub(super) struct AdmissionState {
    operators: HashMap<u64, Arc<Operator>>,
    counts: HashMap<u32, u64>,
    pub(super) waiting: RandBuckets,
    pub(super) waiting_counts: WaitingOperatorStatus,
}
impl AdmissionState {
    fn detach(&mut self, id: u64) -> Option<Arc<Operator>> {
        let op = self.operators.remove(&id)?;
        *self
            .counts
            .get_mut(&op.scheduler_kind().0)
            .expect("registered count") -= 1;
        Some(op)
    }
    fn detach_related(&mut self, op: &Operator, removed: &mut Vec<Arc<Operator>>) {
        let id = op.additional_info().related_merge_region();
        if id != 0
            && let Some(related) = self.detach(id)
        {
            related.cancel_with_reasons(&[CancelReason::RelatedMergeRegion]);
            removed.push(related);
        }
    }
}
/// Per-region unique operations with serialized batch transitions and queue counts.
///
/// The caller supplies its canonical cluster read snapshot and waiting limit. No
/// cluster state is copied into this container. The host must not mutate operator
/// status, priority or related-region metadata concurrently with admission. Clock
/// callbacks must not reenter this controller. Dispatch, store budgets, history
/// retention and transport belong to subsequent controller layers.
pub struct ControllerAdmission {
    pub(super) state: Mutex<AdmissionState>,
    max_waiting: u64,
}
impl ControllerAdmission {
    pub fn new(max_waiting: u64) -> Self {
        Self {
            state: Mutex::new(AdmissionState::default()),
            max_waiting,
        }
    }
    fn valid_batch(ops: &[Arc<Operator>]) -> bool {
        let mut ids = HashSet::new();
        if ops.is_empty() || ops.iter().any(|o| !ids.insert(o.region_id())) {
            return false;
        }
        ops.iter().all(|o| {
            if !o.additional_info().has_related_merge_region() {
                return true;
            }
            let related = o.additional_info().related_merge_region();
            related != 0
                && related != o.region_id()
                && ops.iter().any(|p| {
                    p.region_id() == related
                        && p.additional_info().related_merge_region() == o.region_id()
                })
        })
    }
    pub(super) fn reject(ops: &[Arc<Operator>], reason: CancelReason) -> AdmissionError {
        for op in ops {
            op.cancel_with_reasons(&[reason]);
        }
        AdmissionError::Rejected(reason)
    }
    fn check(
        &self,
        state: &AdmissionState,
        cluster: &BasicCluster,
        ops: &[Arc<Operator>],
        promoting: bool,
    ) -> Result<(), AdmissionError> {
        if !Self::valid_batch(ops) {
            return Err(AdmissionError::InvalidBatch);
        }
        for op in ops {
            let reason = if let Some(region) = cluster.regions.get_region(op.region_id()) {
                if region.meta().epoch.unwrap_or_default()
                    != op.region_epoch().copied().unwrap_or_default()
                {
                    Some(CancelReason::EpochNotMatch)
                } else if state.operators.get(&op.region_id()).is_some_and(|old| {
                    op.priority_level().as_i32() <= old.priority_level().as_i32()
                }) {
                    Some(CancelReason::AlreadyExist)
                } else if op.status() != CREATED {
                    Some(CancelReason::NotInCreateStatus)
                } else if !promoting
                    && state.waiting_counts.get_count(op.desc()) >= self.max_waiting
                {
                    Some(CancelReason::ExceedWaitLimit)
                } else {
                    None
                }
            } else {
                Some(CancelReason::RegionNotFound)
            };
            if let Some(reason) = reason {
                return Err(Self::reject(ops, reason));
            }
        }
        // Evaluate every member: expiry itself changes CREATED to EXPIRED.
        let mut expired = false;
        for op in ops {
            expired |= op.check_expired();
        }
        if expired {
            return Err(Self::reject(ops, CancelReason::Expired));
        }
        Ok(())
    }
    pub(super) fn admit(
        &self,
        state: &mut AdmissionState,
        cluster: &BasicCluster,
        ops: &[Arc<Operator>],
        promoting: bool,
    ) -> Result<AdmissionOutcome, AdmissionError> {
        self.check(state, cluster, ops, promoting)?;
        // No map or counts change until every new operator starts successfully.
        for op in ops {
            if !op.start() {
                return Err(Self::reject(ops, CancelReason::StartFailed));
            }
        }
        let mut removed = Vec::new();
        // Remove all replaced members first, so recursive partner removal cannot
        // touch a newly admitted member or change a replaced member to CANCELED.
        let replaced: Vec<_> = ops
            .iter()
            .filter_map(|op| state.detach(op.region_id()))
            .collect();
        for old in replaced {
            state.detach_related(&old, &mut removed);
            old.replace();
            removed.push(old);
        }
        for op in ops {
            state.operators.insert(op.region_id(), op.clone());
            *state.counts.entry(op.scheduler_kind().0).or_default() += 1;
        }
        Ok(AdmissionOutcome {
            admitted: ops.to_vec(),
            removed,
        })
    }
    pub fn add_operators(
        &self,
        cluster: &BasicCluster,
        ops: &[Arc<Operator>],
    ) -> Result<AdmissionOutcome, AdmissionError> {
        self.admit(
            &mut self.state.lock().expect("admission poisoned"),
            cluster,
            ops,
            false,
        )
    }
    /// Queue one operation or a reciprocal pair, counting a pair once. The host
    /// calls promote_waiting after enqueue to compose Go AddWaitingOperator.
    pub fn enqueue_waiting(
        &self,
        cluster: &BasicCluster,
        ops: &[Arc<Operator>],
    ) -> Result<(), AdmissionError> {
        let state = self.state.lock().expect("admission poisoned");
        if ops.len() != 1 && ops.len() != 2 {
            return Err(AdmissionError::InvalidBatch);
        }
        self.check(&state, cluster, ops, false)?;
        if ops.len() == 2 {
            if !state.waiting.put_merge_operators(ops)? {
                return Err(AdmissionError::InvalidBatch);
            }
        } else {
            state.waiting.put_operator(ops[0].clone())?;
        }
        state.waiting_counts.inc_count(ops[0].desc());
        Ok(())
    }
    /// Skip invalid/stale batches until one is admitted or the queue empties.
    /// A host random sample is reused on each retry; each dequeue renormalizes.
    pub fn promote_waiting(
        &self,
        cluster: &BasicCluster,
        random: f64,
    ) -> Result<Option<AdmissionOutcome>, AdmissionError> {
        let mut state = self.state.lock().expect("admission poisoned");
        while let Some(ops) = state.waiting.get_operator(random)? {
            state.waiting_counts.dec_count(ops[0].desc());
            match self.admit(&mut state, cluster, &ops, true) {
                Ok(out) => return Ok(Some(out)),
                Err(AdmissionError::Rejected(_)) => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    }
    /// Identity comparison prevents an old dispatch from deleting a replacement.
    pub fn remove_operator(&self, op: &Arc<Operator>, reason: CancelReason) -> Vec<Arc<Operator>> {
        self.detach_operator(op, &[reason])
    }
    pub(super) fn detach_operator(
        &self,
        op: &Arc<Operator>,
        reasons: &[CancelReason],
    ) -> Vec<Arc<Operator>> {
        let mut state = self.state.lock().expect("admission poisoned");
        if !state
            .operators
            .get(&op.region_id())
            .is_some_and(|old| Arc::ptr_eq(old, op))
        {
            return vec![];
        }
        let mut removed = vec![state.detach(op.region_id()).expect("matched above")];
        state.detach_related(op, &mut removed);
        op.cancel_with_reasons(reasons);
        removed
    }
    pub fn remove_operators(&self, reason: CancelReason) -> Vec<Arc<Operator>> {
        let mut state = self.state.lock().expect("admission poisoned");
        let mut removed = Vec::new();
        while let Some(id) = state.operators.keys().next().copied() {
            let op = state.detach(id).expect("existing key");
            state.detach_related(&op, &mut removed);
            op.cancel_with_reasons(&[reason]);
            removed.push(op);
        }
        removed
    }
    pub fn get_operator(&self, region: u64) -> Option<Arc<Operator>> {
        self.state
            .lock()
            .expect("admission poisoned")
            .operators
            .get(&region)
            .cloned()
    }
    /// A snapshot taken under the same lock as pair admission and removal.
    pub fn operators(&self) -> Vec<Arc<Operator>> {
        self.state
            .lock()
            .expect("admission poisoned")
            .operators
            .values()
            .cloned()
            .collect()
    }
    /// Count exactly one primary scheduler kind, matching Go OperatorCount.
    pub fn operator_count(&self, kind: OpKind) -> u64 {
        self.state
            .lock()
            .expect("admission poisoned")
            .counts
            .get(&kind.0)
            .copied()
            .unwrap_or(0)
    }
    pub fn waiting_count(&self, desc: &str) -> u64 {
        self.state
            .lock()
            .expect("admission poisoned")
            .waiting_counts
            .get_count(desc)
    }
}
