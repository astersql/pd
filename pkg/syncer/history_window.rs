// Copyright 2026 AsterSQL.
//! Retained full-sync windows and bounded replay-capacity adaptation.
use crate::{HistoryBuffer, HistoryError, HistoryIndex, normalize_history_buffer_capacity};
use astersql_pd_core::RegionInfo;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, MutexGuard},
};

type Retains = Arc<Mutex<BTreeMap<HistoryIndex, usize>>>;
fn lock(retains: &Retains) -> MutexGuard<'_, BTreeMap<HistoryIndex, usize>> {
    // No user code runs under this lock; recovering keeps Drop safe during unwinding.
    retains.lock().unwrap_or_else(|error| error.into_inner())
}

#[derive(Debug)]
pub(crate) struct WindowPolicy {
    pub(crate) base: usize,
    maximum: usize,
    unit: usize,
    observed: u64,
    low_rounds: u8,
    retains: Retains,
}
impl WindowPolicy {
    pub(crate) fn new(base: usize, maximum: usize, unit: usize) -> Self {
        Self {
            base,
            maximum,
            unit,
            observed: 0,
            low_rounds: 0,
            retains: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }
    pub(crate) fn reset(&mut self) {
        self.observed = 0;
        self.low_rounds = 0;
        // Old guards refer only to the old generation, even if indexes are reused.
        self.retains = Arc::new(Mutex::new(BTreeMap::new()));
    }
}

/// Keeps a full-sync start index eligible for growth until dropped.
/// Own this guard across the full sync and catch-up; error returns, cancelled futures,
/// and unwinding release it automatically. It can be dropped on another thread.
/// Retention is bounded by maximum capacity: always check `retained_records_from`.
#[derive(Debug)]
#[must_use = "dropping the guard releases the full-sync window"]
pub struct HistoryRetain {
    index: HistoryIndex,
    retains: Retains,
}
impl Drop for HistoryRetain {
    fn drop(&mut self) {
        let mut retains = lock(&self.retains);
        if let Some(count) = retains.get_mut(&self.index) {
            *count -= 1;
            if *count == 0 {
                retains.remove(&self.index);
            }
        }
    }
}

impl HistoryBuffer {
    /// Retain a full-sync start index. Duplicate starts have independent guards.
    /// Old or future starts are allowed, matching Go; replay detects any gap.
    pub fn retain_from(&self, index: HistoryIndex) -> HistoryRetain {
        *lock(&self.window.retains).entry(index).or_default() += 1;
        HistoryRetain {
            index,
            retains: self.window.retains.clone(),
        }
    }

    /// Observe a downstream replay window; grow to twice its size when it exceeds
    /// half the current capacity. Keep the largest observation until the next check.
    pub fn observe_required_window(&mut self, window: u64) -> Result<(), HistoryError> {
        self.window.observed = self.window.observed.max(window);
        if window > (self.capacity / 2) as u64 {
            self.grow_for_window(window.saturating_mul(2))?;
        }
        Ok(())
    }

    /// Consume this round's maximum observation. After three consecutive windows
    /// at most one quarter of capacity, halve once, bounded by the base capacity.
    /// Active full syncs and high windows reset the low-window streak.
    pub fn maybe_shrink(&mut self) -> Result<(), HistoryError> {
        let observed = std::mem::take(&mut self.window.observed);
        if !lock(&self.window.retains).is_empty()
            || self.capacity <= self.window.base
            || observed > (self.capacity / 4) as u64
        {
            self.window.low_rounds = 0;
            return Ok(());
        }
        self.window.low_rounds += 1;
        if self.window.low_rounds == 3 {
            self.resize_history((self.capacity / 2).max(self.window.base))?;
            self.window.low_rounds = 0;
        }
        Ok(())
    }

    /// Return a complete catch-up snapshot and its exclusive end. An empty snapshot
    /// at next is successful; overwritten or future starts return an explicit gap.
    pub fn retained_records_from(
        &self,
        index: HistoryIndex,
    ) -> Result<(Vec<Arc<RegionInfo>>, HistoryIndex), HistoryError> {
        if index < self.first_index() || index > self.next_index() {
            return Err(HistoryError::HistoryGap {
                requested: index,
                first: self.first_index(),
                next: self.next_index(),
            });
        }
        Ok((self.records_from(index), self.next_index()))
    }

    pub(crate) fn prepare_required_window(
        &mut self,
        end: HistoryIndex,
    ) -> Result<(), HistoryError> {
        let earliest = lock(&self.window.retains)
            .first_key_value()
            .map(|(index, _)| *index);
        if let Some(index) = earliest {
            self.grow_for_window(end.0.saturating_sub(index.0))?;
        }
        Ok(())
    }
    fn grow_for_window(&mut self, window: u64) -> Result<(), HistoryError> {
        // Clamp before normalization/conversion to avoid overflow for huge windows.
        let window = usize::try_from(window)
            .unwrap_or(usize::MAX)
            .min(self.window.maximum);
        let target = normalize_history_buffer_capacity(window, self.window.unit)?;
        if target > self.capacity {
            self.resize_history(target)?;
        }
        Ok(())
    }
    fn resize_history(&mut self, capacity: usize) -> Result<(), HistoryError> {
        if capacity > self.capacity {
            self.records
                .try_reserve_exact(capacity - self.records.len())?;
        } else {
            self.records
                .drain(..self.records.len().saturating_sub(capacity));
            self.records.shrink_to(capacity);
        }
        self.capacity = capacity;
        Ok(())
    }
}
