// Copyright 2026 AsterSQL.
//! Store-local leader transfer reference counts. Clones snapshot these values;
//! heartbeat statistics and rate limiters retain their separate sharing rules.
use astersql_pd_core_constant::Direction;

impl super::StoreInfo {
    /// Pause one direction. Unknown directions are ignored, as in Go's switch.
    pub fn pause_leader_transfer(&mut self, direction: Direction) {
        self.change_leader_transfer(direction, 1);
    }

    /// Release one pause. Counts intentionally remain signed: an unmatched
    /// resume produces a negative count, which still disallows transfer.
    pub fn resume_leader_transfer(&mut self, direction: Direction) {
        self.change_leader_transfer(direction, -1);
    }

    fn change_leader_transfer(&mut self, direction: Direction, delta: i64) {
        match direction {
            Direction::IN => self.control[0] = self.control[0].wrapping_add(delta),
            Direction::OUT => self.control[1] = self.control[1].wrapping_add(delta),
            _ => {}
        }
    }

    /// Whether this store may be selected as the target of a leader transfer.
    pub fn allow_leader_transfer_in(&self) -> bool {
        self.control[0] == 0
    }
    /// Whether this store may be selected as the source of a leader transfer.
    pub fn allow_leader_transfer_out(&self) -> bool {
        self.control[1] == 0
    }

    /// Add one slow store eviction reference.
    pub fn slow_store_evicted(&mut self) {
        self.control[2] = self.control[2].wrapping_add(1);
    }
    /// Release one eviction reference, preserving signed Go counter semantics.
    pub fn slow_store_recovered(&mut self) {
        self.control[2] = self.control[2].wrapping_sub(1);
    }
    /// Whether at least one positive eviction reference remains.
    pub fn evicted_as_slow_store(&self) -> bool {
        self.control[2] > 0
    }

    /// Add one stopping store eviction reference.
    pub fn stopping_store_evicted(&mut self) {
        self.control[3] = self.control[3].wrapping_add(1);
    }
    /// Release one eviction reference, preserving signed Go counter semantics.
    pub fn stopping_store_recovered(&mut self) {
        self.control[3] = self.control[3].wrapping_sub(1);
    }
    /// Whether at least one positive eviction reference remains.
    pub fn evicted_as_stopping_store(&self) -> bool {
        self.control[3] > 0
    }

    /// Add one slow trend eviction reference.
    pub fn slow_trend_evicted(&mut self) {
        self.control[4] = self.control[4].wrapping_add(1);
    }
    /// Release one eviction reference, preserving signed Go counter semantics.
    pub fn slow_trend_recovered(&mut self) {
        self.control[4] = self.control[4].wrapping_sub(1);
    }
    /// Whether at least one positive eviction reference remains.
    pub fn is_evicted_as_slow_trend(&self) -> bool {
        self.control[4] > 0
    }
}
