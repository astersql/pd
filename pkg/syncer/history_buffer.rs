// Copyright 2026 AsterSQL.
//! Bounded region-change history, independent of storage revisions and transports.
use astersql_pd_core::RegionInfo;
use std::{collections::VecDeque, sync::Arc};

const CAPACITY_UNIT: usize = 10_000;

/// Sequence number of a region change, or the next number to assign.
/// This is deliberately distinct from a MetaStore revision.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HistoryIndex(pub u64);

/// Failure to construct a ring or assign another history index.
#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("history capacity overflow")]
    CapacityOverflow,
    #[error("history allocation failed: {0}")]
    AllocationFailed(#[from] std::collections::TryReserveError),
    #[error("history index exhausted")]
    IndexExhausted,
}

/// Round up to `unit * 2^n`, defaulting a zero unit to 10,000.
/// Rust capacities are nonnegative; zero size still receives one unit.
pub fn normalize_history_buffer_capacity(size: usize, unit: usize) -> Result<usize, HistoryError> {
    let mut capacity = if unit == 0 { CAPACITY_UNIT } else { unit };
    while capacity < size {
        capacity = capacity
            .checked_mul(2)
            .ok_or(HistoryError::CapacityOverflow)?;
    }
    Ok(capacity)
}

/// Fixed-capacity history retaining the newest region changes in assignment order.
/// `first_index()..next_index()` is exactly the replayable half-open interval.
/// Mutations require exclusive access; hosts may share the ring through their own lock.
/// Returned snapshots share immutable region values and survive overwrites and resets.
#[derive(Debug)]
pub struct HistoryBuffer {
    records: VecDeque<Arc<RegionInfo>>,
    capacity: usize,
    next: HistoryIndex,
}

impl HistoryBuffer {
    /// Create empty history with capacity rounded in units of 10,000.
    pub fn new(capacity: usize) -> Result<Self, HistoryError> {
        Self::with_capacity_unit(capacity, CAPACITY_UNIT)
    }

    /// Create empty history with an explicit normalization unit.
    pub fn with_capacity_unit(capacity: usize, unit: usize) -> Result<Self, HistoryError> {
        let capacity = normalize_history_buffer_capacity(capacity, unit)?;
        let mut records = VecDeque::new();
        records.try_reserve_exact(capacity)?;
        Ok(Self {
            records,
            capacity,
            next: HistoryIndex(0),
        })
    }

    /// Maximum retained record count, excluding any allocator spare space.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Number of replayable records.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether no changes are currently replayable.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Next sequence number to assign, also the exclusive replay end.
    pub fn next_index(&self) -> HistoryIndex {
        self.next
    }

    /// Oldest replayable sequence number, equal to next when empty.
    pub fn first_index(&self) -> HistoryIndex {
        HistoryIndex(self.next.0 - self.records.len() as u64)
    }

    /// Discard all records and restart assignment at the given index.
    /// This operation only changes memory; it does not persist a checkpoint.
    pub fn reset_with_index(&mut self, index: HistoryIndex) {
        self.records.clear();
        self.next = index;
    }

    /// Assign the next index, evicting the oldest change when full.
    /// Exhaustion returns an error before any record or index changes.
    pub fn record(&mut self, region: Arc<RegionInfo>) -> Result<HistoryIndex, HistoryError> {
        let next = self
            .next
            .0
            .checked_add(1)
            .ok_or(HistoryError::IndexExhausted)?;
        let assigned = self.next;
        if self.records.len() == self.capacity {
            self.records.pop_front();
        }
        self.records.push_back(region);
        self.next = HistoryIndex(next);
        Ok(assigned)
    }

    /// Read a retained change; old and not-yet-assigned indexes return `None`.
    pub fn get(&self, index: HistoryIndex) -> Option<&Arc<RegionInfo>> {
        if index < self.first_index() || index >= self.next {
            return None;
        }
        self.records.get((index.0 - self.first_index().0) as usize)
    }

    /// Snapshot changes from index through the current exclusive end.
    pub fn records_from(&self, index: HistoryIndex) -> Vec<Arc<RegionInfo>> {
        self.records_between(index, self.next)
    }

    /// Snapshot `[index, end)`, clamping end to next. Invalid or empty ranges
    /// return an empty vector, matching Go's nil slice; old starts are not clamped.
    pub fn records_between(&self, index: HistoryIndex, end: HistoryIndex) -> Vec<Arc<RegionInfo>> {
        let end = end.min(self.next);
        if index >= end || index < self.first_index() {
            return Vec::new();
        }
        let start = (index.0 - self.first_index().0) as usize;
        self.records
            .range(start..start + (end.0 - index.0) as usize)
            .cloned()
            .collect()
    }
}
