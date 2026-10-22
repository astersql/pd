// Copyright 2026 AsterSQL.

use astersql_pd_core_constant::PriorityLevel;

use crate::{Type, VERSION_V2, Window};

/// Priority snapshot windows. Mutation requires exclusive access by the caller.
#[derive(Debug, Clone)]
pub struct SlidingWindows {
    pub(crate) windows: [Window; 4],
    pub(crate) last_sum: f64,
}

impl Default for SlidingWindows {
    fn default() -> Self {
        Self::new()
    }
}

impl SlidingWindows {
    /// Creates empty low-through-urgent windows with capacities 100/50/25/12.
    pub fn new() -> Self {
        Self {
            windows: std::array::from_fn(|i| Window::new(100 >> i)),
            last_sum: 0.0,
        }
    }

    /// Returns the sliding window protocol version.
    pub fn version(&self) -> &'static str {
        VERSION_V2
    }

    /// Returns the low-priority window capacity.
    pub fn capacity(&self) -> i64 {
        self.windows[0].capacity()
    }

    /// Returns usage ordered from low to urgent priority.
    pub fn used(&self) -> [i64; 4] {
        std::array::from_fn(|i| self.windows[i].used())
    }

    /// Checks minimum space, ignoring token size as in Go. Non-snapshots bypass admission.
    pub fn available(&self, _token: i64, typ: Type, level: PriorityLevel) -> bool {
        if typ != Type::SEND_SNAPSHOT {
            return true;
        }
        (0..=level.as_i32()).any(|i| self.windows[i as usize].available())
    }

    /// Takes from the first available window at or below the requested priority.
    pub fn take(&mut self, token: i64, typ: Type, level: PriorityLevel) -> bool {
        if typ != Type::SEND_SNAPSHOT {
            return true;
        }
        (0..=level.as_i32()).any(|i| self.windows[i as usize].take(token))
    }

    /// Acknowledges outstanding snapshots from urgent to low priority.
    pub fn ack(&mut self, mut token: i64, typ: Type) {
        if typ != Type::SEND_SNAPSHOT {
            return;
        }
        for window in self.windows.iter_mut().rev() {
            token = window.ack(token);
            if token <= 0 {
                break;
            }
        }
    }
}
