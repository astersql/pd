// Copyright 2026 AsterSQL.

const MIN_SNAPSHOT_SIZE: i64 = 10;

/// A snapshot admission window, matching Go's single-window accounting.
///
/// Admission checks free space before taking the entire token amount, so one
/// admitted operation may exceed capacity. Resetting capacity keeps usage.
#[derive(Debug, Clone)]
pub struct Window {
    capacity: i64,
    used: i64,
    count: u64,
}

impl Window {
    /// Creates an empty window with capacity clamped to at least ten.
    pub fn new(capacity: i64) -> Self {
        Self {
            capacity: capacity.max(MIN_SNAPSHOT_SIZE),
            used: 0,
            count: 0,
        }
    }

    /// Changes capacity without clearing usage or the operation count.
    pub fn reset(&mut self, capacity: i64) {
        self.capacity = capacity.max(MIN_SNAPSHOT_SIZE);
    }

    /// Returns the configured capacity.
    pub fn capacity(&self) -> i64 {
        self.capacity
    }

    /// Returns the outstanding token usage.
    pub fn used(&self) -> i64 {
        self.used
    }

    /// Returns the operation count with Go's unsigned wrapping semantics.
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Reports whether at least the minimum snapshot size remains.
    pub fn available(&self) -> bool {
        self.used.wrapping_add(MIN_SNAPSHOT_SIZE) <= self.capacity
    }

    /// Takes the entire amount if the window currently has minimum free space.
    pub fn take(&mut self, token: i64) -> bool {
        if !self.available() {
            return false;
        }
        self.used = self.used.wrapping_add(token);
        self.count = self.count.wrapping_add(1);
        true
    }

    /// Acknowledges usage and returns tokens remaining for another window.
    ///
    /// Every acknowledgement against nonzero usage decrements the count,
    /// including partial acknowledgements, matching the Go implementation.
    pub fn ack(&mut self, token: i64) -> i64 {
        if self.used == 0 {
            return token;
        }
        let remaining = if self.used > token {
            self.used = self.used.wrapping_sub(token);
            0
        } else {
            let remaining = token.wrapping_sub(self.used);
            self.used = 0;
            remaining
        };
        self.count = self.count.wrapping_sub(1);
        remaining
    }
}
