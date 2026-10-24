// Copyright 2026 AsterSQL.
//! Full-region snapshots with an independent history retention boundary.
use crate::{HistoryBuffer, HistoryIndex, HistoryRetain, MAX_SYNC_REGION_BATCH_SIZE};
use astersql_pd_core::{BasicCluster, RegionInfo};
use std::{num::NonZeroUsize, sync::Arc};

/// Position in a full scan, deliberately distinct from a history sequence number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotOffset(pub usize);

/// Borrowed immutable full-scan batch; carries no history acknowledgement.
#[derive(Debug)]
pub struct SnapshotBatch<'a> {
    pub offset: SnapshotOffset,
    pub records: &'a [Arc<RegionInfo>],
}

/// Owns the scan and its retain through sending and the host's catch-up phase.
#[derive(Debug)]
pub struct FullSnapshot {
    start: HistoryIndex,
    regions: Vec<Arc<RegionInfo>>,
    _retain: HistoryRetain,
}
impl FullSnapshot {
    /// Capture under the host's append/bind lock. Exclusive history access must
    /// cover an entire append batch, so bind cannot capture a partial append.
    /// The returned value holds no cluster/history borrow across sending.
    pub fn bind(history: &mut HistoryBuffer, cluster: &BasicCluster) -> Self {
        let start = history.next_index();
        let retain = history.retain_from(start);
        Self {
            start,
            regions: cluster.regions.get_regions(),
            _retain: retain,
        }
    }
    /// Bind-time exclusive history boundary, unaffected by snapshot offsets.
    pub fn sync_start_index(&self) -> HistoryIndex {
        self.start
    }
    /// Batches start at zero; empty snapshots have no full-scan frame.
    pub fn batches(&self, size: NonZeroUsize) -> impl Iterator<Item = SnapshotBatch<'_>> {
        let size = size.get().min(MAX_SYNC_REGION_BATCH_SIZE);
        self.regions
            .chunks(size)
            .enumerate()
            .map(move |(i, records)| SnapshotBatch {
                offset: SnapshotOffset(i * size),
                records,
            })
    }
    /// Send without holding the append lock. Failure or unwinding drops retain;
    /// success returns ownership so retention continues through catch-up.
    /// The host callback performs rate limiting, cancellation and open checks.
    pub fn send<E>(
        self,
        size: NonZeroUsize,
        mut send: impl FnMut(SnapshotBatch<'_>) -> Result<(), E>,
    ) -> Result<Self, E> {
        for batch in self.batches(size) {
            send(batch)?;
        }
        Ok(self)
    }
}
