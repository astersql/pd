// Copyright 2026 AsterSQL.
//! Deadline-ordered retries for the initial GC warmup campaign.
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};
/// One queued deadline per scope. Promotion removes it before another attempt may be scheduled.
#[derive(Default)]
pub(crate) struct GCRetryQueue {
    ordered: BTreeSet<(Instant, u32)>,
    by_id: BTreeMap<u32, Instant>,
}
impl GCRetryQueue {
    pub(crate) fn schedule(&mut self, id: u32, deadline: Instant) {
        self.remove(id);
        self.by_id.insert(id, deadline);
        self.ordered.insert((deadline, id));
    }
    pub(crate) fn remove(&mut self, id: u32) {
        if let Some(deadline) = self.by_id.remove(&id) {
            self.ordered.remove(&(deadline, id));
        }
    }
    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.ordered.first().map(|(deadline, _)| *deadline)
    }
    pub(crate) fn pop_due(&mut self, now: Instant, limit: usize) -> Vec<u32> {
        let mut due = Vec::with_capacity(limit.min(self.by_id.len()));
        while due.len() < limit {
            let Some((deadline, id)) = self.ordered.first().copied() else {
                break;
            };
            if deadline > now {
                break;
            }
            self.remove(id);
            due.push(id);
        }
        due
    }
    pub(crate) fn retain(&mut self, mut keep: impl FnMut(u32) -> bool) {
        self.by_id.retain(|id, _| keep(*id));
        self.ordered.retain(|(_, id)| self.by_id.contains_key(id));
    }
    pub(crate) fn clear(&mut self) {
        self.by_id.clear();
        self.ordered.clear();
    }
}

pub(crate) fn next_retry_delay(previous: std::time::Duration) -> std::time::Duration {
    if previous.is_zero() {
        std::time::Duration::from_secs(1)
    } else {
        (previous * 2).min(std::time::Duration::from_secs(30))
    }
}
