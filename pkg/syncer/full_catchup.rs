// Copyright 2026 AsterSQL.
//! Full-sync catch-up at a captured history boundary.
use crate::{FullSnapshot, HistoryBuffer, HistoryError, HistoryIndex, MAX_SYNC_REGION_BATCH_SIZE};
use astersql_pd_core::RegionInfo;
use std::{
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

/// Full-sync wire positions differ from history indexes for an empty region scan.
#[derive(Debug)]
pub enum CatchUpFrame<'a> {
    Records {
        start_index: u64,
        records: &'a [Arc<RegionInfo>],
    },
    Complete {
        next_index: HistoryIndex,
    },
}

/// Both failures terminate this stream. A history gap requires a new full sync.
#[derive(Debug, thiserror::Error)]
pub enum FullCatchUpError<E> {
    #[error("full sync requires resynchronization: {0}")]
    ResyncRequired(HistoryError),
    #[error("full sync send failed: {0}")]
    Send(E),
}
impl FullSnapshot {
    /// Consume the retained snapshot after its full-scan frames were sent.
    /// Host serializes this send with downstream sends, checks cancellation/open
    /// state in `send`, and removes/closes the stream in `disconnect` on failure.
    /// Only success supplies the history index from which live draining resumes.
    pub fn catch_up<E>(
        self,
        history: &Mutex<HistoryBuffer>,
        size: NonZeroUsize,
        mut send: impl FnMut(CatchUpFrame<'_>) -> Result<(), E>,
        disconnect: impl FnOnce(),
    ) -> Result<HistoryIndex, FullCatchUpError<E>> {
        let result = (|| {
            // Capture the complete interval under the append lock, then release
            // it before any transport callback can block or append new records.
            let (records, end) = history
                .lock()
                .expect("history append lock poisoned")
                .retained_records_from(self.sync_start_index())
                .map_err(FullCatchUpError::ResyncRequired)?;
            let start = if self.batches(size).next().is_none() {
                0
            } else {
                self.sync_start_index().0
            };
            let size = size.get().min(MAX_SYNC_REGION_BATCH_SIZE);
            for (batch, records) in records.chunks(size).enumerate() {
                send(CatchUpFrame::Records {
                    start_index: start + (batch * size) as u64,
                    records,
                })
                .map_err(FullCatchUpError::Send)?;
            }
            // Do not reread history here: writes during sends belong to the
            // subsequent live drain and must not be acknowledged by completion.
            send(CatchUpFrame::Complete { next_index: end }).map_err(FullCatchUpError::Send)?;
            Ok(end)
        })();
        if result.is_err() {
            disconnect();
        }
        // `self` keeps the retain alive through completion/disconnection.
        result
    }
}
