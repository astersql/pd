// Copyright 2026 AsterSQL.
//! Transport-independent bounded history replay plans.
use crate::{HistoryBuffer, HistoryError, HistoryIndex};
use astersql_pd_core::RegionInfo;
use std::{num::NonZeroUsize, sync::Arc};

/// Go's maximum number of region changes per incremental frame.
pub const MAX_SYNC_REGION_BATCH_SIZE: usize = 1000;

/// A frame beginning at a history index, with complete immutable region values.
#[derive(Debug, Clone)]
pub struct SyncFrame {
    start_index: HistoryIndex,
    records: Vec<Arc<RegionInfo>>,
}
impl SyncFrame {
    /// Empty acknowledgement at the host's successfully sent index.
    pub fn keepalive(sent_index: HistoryIndex) -> Self {
        Self {
            start_index: sent_index,
            records: Vec::new(),
        }
    }
    /// First history sequence number in this frame.
    pub fn start_index(&self) -> HistoryIndex {
        self.start_index
    }
    /// Region changes in assignment order; all domain fields are preserved.
    pub fn records(&self) -> &[Arc<RegionInfo>] {
        &self.records
    }
    /// Exclusive end, eligible for acknowledgement only after successful sending.
    pub fn end_index(&self) -> HistoryIndex {
        HistoryIndex(self.start_index.0 + self.records.len() as u64)
    }
}

/// Either a complete bounded incremental snapshot or a request for full sync.
#[derive(Debug)]
pub enum HistoryReplay {
    /// The host must run full sync and catch up from this captured history index.
    FullSync { sync_start_index: HistoryIndex },
    /// Ordered frames; an already-current request has one empty acknowledgement.
    Incremental(Vec<SyncFrame>),
}
impl HistoryBuffer {
    /// Plan replay to the exclusive bind-time end, excluding subsequent writes.
    /// Batch size must be nonzero and is capped at the protocol's 1000 records.
    /// Does not send, advance downstream state, or alter assigned history indexes.
    pub fn plan_replay(
        &mut self,
        start: HistoryIndex,
        end: HistoryIndex,
        batch_size: NonZeroUsize,
    ) -> Result<HistoryReplay, HistoryError> {
        if start.0 == 0 || start > end {
            return Ok(HistoryReplay::FullSync {
                sync_start_index: end,
            });
        }
        if start < end {
            self.observe_required_window(end.0 - start.0)?;
        }
        if start == end {
            return Ok(HistoryReplay::Incremental(vec![SyncFrame::keepalive(
                start,
            )]));
        }
        let records = self.records_between(start, end);
        if records.len() as u64 != end.0 - start.0 {
            return Ok(HistoryReplay::FullSync {
                sync_start_index: end,
            });
        }
        let size = batch_size.get().min(MAX_SYNC_REGION_BATCH_SIZE);
        let frames = records
            .chunks(size)
            .enumerate()
            .map(|(batch, records)| SyncFrame {
                // The complete snapshot bounds every offset and frame end by `end`.
                start_index: HistoryIndex(start.0 + (batch * size) as u64),
                records: records.to_vec(),
            })
            .collect();
        Ok(HistoryReplay::Incremental(frames))
    }
}
