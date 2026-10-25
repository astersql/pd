// Copyright 2026 AsterSQL.
//! Terminal retention and notification over canonical operator references.
use crate::*;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};

pub const OPERATOR_STATUS_REMAIN_TIME: Duration = Duration::from_secs(600);
/// Success hooks also allow the host to increment its finished counters.
pub type SuccessCallback = Arc<dyn Fn(&Operator) + Send + Sync>;
/// An owned view: status and finish time are frozen; the operator stays canonical.
#[derive(Clone)]
pub struct ArchivedOperator {
    pub operator: Arc<Operator>,
    pub status: OpStatus,
    pub finish_time: Instant,
}
impl ArchivedOperator {
    pub fn record(&self) -> OpRecord<'_> {
        self.operator.record(self.finish_time)
    }
}
/// One metric observation per burial. The host exports operators_count by
/// (desc,event), and finish_operators_duration_seconds only for `finish`.
/// No process-global registry or subscriber is installed by this library.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalObservation {
    pub desc: String,
    pub event: &'static str,
    pub unexpected: bool,
    pub running_time: Option<Duration>,
}
#[derive(Default)]
struct RecordState {
    records: HashMap<u64, ArchivedOperator>,
    // Weak identity tombstones suppress retries after TTL expiry/clear without
    // retaining operators forever. Dead entries are reclaimed on the next burial.
    buried: HashMap<usize, Weak<Operator>>,
}
/// Consume every removed member from admission/dispatch, including merge partners,
/// replacements and rejected batches. Removal/limiter acknowledgement remains in
/// the existing controller. Callbacks are configured before sharing this owner.
#[derive(Default)]
pub struct ControllerRecords {
    state: Mutex<RecordState>,
    success_callbacks: Vec<Option<SuccessCallback>>,
}
impl ControllerRecords {
    /// Replaces the callback list, preserving order and skipping absent callbacks.
    pub fn set_success_callbacks(&mut self, callbacks: Vec<Option<SuccessCallback>>) {
        self.success_callbacks = callbacks;
    }
    pub fn with_success_callbacks(mut self, callbacks: Vec<Option<SuccessCallback>>) -> Self {
        self.set_success_callbacks(callbacks);
        self
    }
    /// Returns the sole metric observation, or None for a duplicate identity.
    /// Unexpected nonterminal states are canceled with Unknown, as in Go burial.
    /// Publication precedes callbacks; callbacks may query or reenter this owner.
    /// A panicking host callback cannot cause a retry to notify twice.
    pub fn bury_operator(&self, op: Arc<Operator>, now: Instant) -> Option<TerminalObservation> {
        let mut state = self.state.lock().expect("records poisoned");
        state.buried.retain(|_, op| op.strong_count() != 0);
        let identity = Arc::as_ptr(&op) as usize;
        if state.buried.contains_key(&identity) {
            return None;
        }
        let status = op.status();
        let unexpected = !status.is_end();
        let event = match status {
            SUCCESS => "finish",
            REPLACED => "replace",
            EXPIRED => "expire",
            TIMEOUT => "timeout",
            CANCELED => "cancel",
            _ => "unexpected",
        };
        if unexpected {
            op.cancel_with_reasons(&[CancelReason::Unknown]);
        }
        state.buried.insert(identity, Arc::downgrade(&op));
        state
            .records
            .retain(|_, r| now <= r.finish_time + OPERATOR_STATUS_REMAIN_TIME);
        state.records.insert(
            op.region_id(),
            ArchivedOperator {
                operator: op.clone(),
                status: op.status(),
                finish_time: now,
            },
        );
        drop(state);
        let observation = TerminalObservation {
            desc: op.desc().to_owned(),
            event,
            unexpected,
            running_time: if status == SUCCESS {
                op.reach_time_of(STARTED)
                    .map(|start| now.saturating_duration_since(start))
            } else {
                None
            },
        };
        if status == SUCCESS {
            for callback in self.success_callbacks.iter().flatten() {
                callback(&op);
            }
        }
        Some(observation)
    }
    pub fn bury_removed(
        &self,
        removed: &[Arc<Operator>],
        now: Instant,
    ) -> Vec<TerminalObservation> {
        removed
            .iter()
            .filter_map(|op| self.bury_operator(op.clone(), now))
            .collect()
    }
    /// Only successful identity removal authorizes notification, never `completed`
    /// alone. All removed partners are archived, with their own actual status.
    pub fn record_dispatch(
        &self,
        outcome: &DispatchOutcome,
        now: Instant,
    ) -> Vec<TerminalObservation> {
        self.bury_removed(&outcome.removed, now)
    }
    /// Active operators take precedence over the last retained region record.
    pub fn get_status(
        &self,
        admission: &ControllerAdmission,
        id: u64,
        now: Instant,
    ) -> Option<ArchivedOperator> {
        if let Some(operator) = admission.get_operator(id) {
            return Some(ArchivedOperator {
                status: operator.status(),
                operator,
                finish_time: now,
            });
        }
        let mut state = self.state.lock().expect("records poisoned");
        state
            .records
            .retain(|_, r| now <= r.finish_time + OPERATOR_STATUS_REMAIN_TIME);
        state.records.get(&id).cloned()
    }
    /// Inclusive finish-time filter. TTL key enumeration is strict at expiry,
    /// while direct status lookup is inclusive, matching cache.Get/GetAllID.
    pub fn get_records(&self, from: Instant, now: Instant) -> Vec<ArchivedOperator> {
        let mut state = self.state.lock().expect("records poisoned");
        state
            .records
            .retain(|_, r| now <= r.finish_time + OPERATOR_STATUS_REMAIN_TIME);
        state
            .records
            .values()
            .filter(|r| r.finish_time >= from && now < r.finish_time + OPERATOR_STATUS_REMAIN_TIME)
            .cloned()
            .collect()
    }
    /// Histories include unsuccessful operations and use query time, as Go does.
    pub fn get_history(&self, from: Instant, now: Instant) -> Vec<OpHistory> {
        self.get_records(from, now)
            .iter()
            .flat_map(|r| r.operator.history_at(now))
            .collect()
    }
    /// Clears retained region records, preserving identity notification guards.
    pub fn clean_all_op_records(&self) {
        self.state.lock().expect("records poisoned").records.clear();
    }
}
