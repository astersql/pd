// Copyright 2026 AsterSQL.
//! Readiness transitions for a serialized follower stream.
use crate::HistoryIndex;

#[derive(Default)]
pub(crate) struct ClientReady {
    running: bool,
    full_syncing: bool,
}

/// Classification is read-only until application succeeds. In particular, an
/// exhausted local history must not acknowledge a partially applied frame.
pub(crate) struct ReadyFrame {
    pub in_full_sync: bool,
    pub positional: bool,
    has_regions: bool,
}

impl ClientReady {
    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn full_syncing(&self) -> bool {
        self.full_syncing
    }

    pub fn reset_stream(&mut self) {
        *self = Self::default();
    }

    /// Error responses clear running but retain the current stream's phase.
    pub fn reject(&mut self) {
        self.running = false;
    }

    pub fn classify(&self, start: HistoryIndex, has_regions: bool) -> ReadyFrame {
        let starts_full =
            !self.full_syncing && !self.running && start == HistoryIndex(0) && has_regions;
        let in_full_sync = self.full_syncing || starts_full;
        ReadyFrame {
            in_full_sync,
            positional: in_full_sync && !starts_full && has_regions,
            has_regions,
        }
    }

    /// Only an empty frame ends full sync, including an old leader keepalive.
    /// A nonzero first data frame is incremental and may make a fresh stream ready.
    pub fn complete(&mut self, frame: ReadyFrame) {
        self.full_syncing = frame.in_full_sync && frame.has_regions;
        if !self.full_syncing {
            self.running = true;
        }
    }
}
