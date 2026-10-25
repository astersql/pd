// Copyright 2026 AsterSQL.

use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap, VecDeque},
    fmt,
    sync::{Arc, Mutex},
    time::Instant,
};

use crate::Operator;

/// The weight of each nonempty priority bucket, from low to urgent.
pub const PRIORITY_WEIGHT: [f64; 4] = [1.0, 4.0, 9.0, 16.0];

/// Invalid boundary input, reported without consuming queued operators.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueError {
    InvalidPriority(i32),
    InvalidRandom,
    UnpairedOperator,
}
impl fmt::Display for QueueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPriority(p) => write!(f, "invalid operator priority: {p}"),
            Self::InvalidRandom => f.write_str("random sample must be finite and in [0, 1)"),
            Self::UnpairedOperator => {
                f.write_str("related merge operator is missing its queue partner")
            }
        }
    }
}
impl std::error::Error for QueueError {}

#[derive(Default)]
struct Buckets {
    ops: [VecDeque<Arc<Operator>>; 4],
    total_weight: f64,
}
impl Buckets {
    fn put(&mut self, priority: usize, ops: impl IntoIterator<Item = Arc<Operator>>) {
        if self.ops[priority].is_empty() {
            self.total_weight += PRIORITY_WEIGHT[priority];
        }
        self.ops[priority].extend(ops);
    }
}

/// Concurrent weighted waiting queue with FIFO order inside each priority bucket.
///
/// The host supplies a unit-interval random sample for each dequeue. Related merge
/// operators should be inserted together with [`Self::put_merge_operators`].
#[derive(Default)]
pub struct RandBuckets(Mutex<Buckets>);
impl RandBuckets {
    fn priority(op: &Operator) -> Result<usize, QueueError> {
        let p = op.priority_level().as_i32();
        if (0..4).contains(&p) {
            Ok(p as usize)
        } else {
            Err(QueueError::InvalidPriority(p))
        }
    }
    pub fn put_operator(&self, op: Arc<Operator>) -> Result<(), QueueError> {
        let priority = Self::priority(&op)?;
        self.0
            .lock()
            .expect("waiting queue poisoned")
            .put(priority, [op]);
        Ok(())
    }
    /// Return false for a non-pair or missing related-region metadata, as in Go.
    /// Both operators use the first operator's insertion priority.
    pub fn put_merge_operators(&self, ops: &[Arc<Operator>]) -> Result<bool, QueueError> {
        if ops.len() != 2
            || ops
                .iter()
                .any(|op| !op.additional_info().has_related_merge_region())
        {
            return Ok(false);
        }
        let priority = Self::priority(&ops[0])?;
        self.0
            .lock()
            .expect("waiting queue poisoned")
            .put(priority, ops.iter().cloned());
        Ok(true)
    }
    /// Return a snapshot in ascending bucket order, preserving FIFO within buckets.
    pub fn list_operator(&self) -> Vec<Arc<Operator>> {
        self.0
            .lock()
            .expect("waiting queue poisoned")
            .ops
            .iter()
            .flat_map(|ops| ops.iter().cloned())
            .collect()
    }
    /// Atomically remove one operator, or two for a related merge operation.
    /// Empty queues return None; deadlines and readiness are not checked here.
    pub fn get_operator(&self, random: f64) -> Result<Option<Vec<Arc<Operator>>>, QueueError> {
        if !random.is_finite() || !(0.0..1.0).contains(&random) {
            return Err(QueueError::InvalidRandom);
        }
        let mut state = self.0.lock().expect("waiting queue poisoned");
        if state.total_weight == 0.0 {
            return Ok(None);
        }
        let mut sum = 0.0;
        for (i, weight) in PRIORITY_WEIGHT.iter().enumerate() {
            if state.ops[i].is_empty() {
                continue;
            }
            let proportion = weight / state.total_weight;
            if random >= sum && random < sum + proportion {
                let batch_size = if state.ops[i][0].additional_info().has_related_merge_region() {
                    2
                } else {
                    1
                };
                if state.ops[i].len() < batch_size {
                    return Err(QueueError::UnpairedOperator);
                }
                let result = state.ops[i].drain(..batch_size).collect();
                if state.ops[i].is_empty() {
                    state.total_weight -= weight;
                }
                return Ok(Some(result));
            }
            sum += proportion;
        }
        Ok(None)
    }
}

/// Concurrent per-description counters; unsigned overflow/underflow wraps like Go.
#[derive(Default)]
pub struct WaitingOperatorStatus(Mutex<HashMap<String, u64>>);
impl WaitingOperatorStatus {
    pub fn inc_count(&self, kind: &str) {
        let mut counts = self.0.lock().expect("waiting counts poisoned");
        let count = counts.entry(kind.to_owned()).or_default();
        *count = count.wrapping_add(1);
    }
    pub fn dec_count(&self, kind: &str) {
        let mut counts = self.0.lock().expect("waiting counts poisoned");
        let count = counts.entry(kind.to_owned()).or_default();
        *count = count.wrapping_sub(1);
    }
    pub fn get_count(&self, kind: &str) -> u64 {
        self.0
            .lock()
            .expect("waiting counts poisoned")
            .get(kind)
            .copied()
            .unwrap_or(0)
    }
}

/// An operator and its host monotonic-clock push deadline.
#[derive(Clone)]
pub struct OperatorWithTime {
    pub op: Arc<Operator>,
    pub deadline: Instant,
}
impl PartialEq for OperatorWithTime {
    fn eq(&self, other: &Self) -> bool {
        self.deadline == other.deadline
    }
}
impl Eq for OperatorWithTime {}
impl PartialOrd for OperatorWithTime {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for OperatorWithTime {
    fn cmp(&self, other: &Self) -> Ordering {
        other.deadline.cmp(&self.deadline)
    }
}

/// Concurrent minimum-deadline heap. Equal deadlines have unspecified order.
#[derive(Default)]
pub struct ConcurrentHeapOpQueue(Mutex<BinaryHeap<OperatorWithTime>>);
impl ConcurrentHeapOpQueue {
    pub fn len(&self) -> usize {
        self.0.lock().expect("push queue poisoned").len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn push(&self, item: OperatorWithTime) {
        self.0.lock().expect("push queue poisoned").push(item);
    }
    /// Remove the earliest entry even if its deadline is in the future.
    pub fn pop(&self) -> Option<OperatorWithTime> {
        self.0.lock().expect("push queue poisoned").pop()
    }
}
