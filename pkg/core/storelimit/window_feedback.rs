// Copyright 2026 AsterSQL.

use crate::SlidingWindows;

impl SlidingWindows {
    /// Adjusts capacities using proportional and accumulated error.
    ///
    /// Ignores feedback while the low-priority window has minimum space.
    /// Retains usage and keeps capacity at least the default window size.
    pub fn feedback(&mut self, error: f64) {
        if self.windows[0].available() {
            return;
        }
        self.last_sum += error;
        let capacity = 20.0 * error + 10.0 * self.last_sum;
        let capacity = if capacity < 100.0 { 100.0 } else { capacity };
        for (i, window) in self.windows.iter_mut().enumerate() {
            window.reset((capacity as i64) >> i);
        }
    }
}
