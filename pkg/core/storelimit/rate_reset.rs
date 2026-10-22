// Copyright 2026 AsterSQL.

use crate::{StoreRateLimit, Type, rate_limit::Limit};

impl StoreRateLimit {
    /// Changes one bucket's region rate, replacing it with a full bucket.
    ///
    /// An unchanged rate preserves tokens and refill time. Zero bypasses admission;
    /// snapshot resets are ignored. Exclusive access serializes rate changes with
    /// admission. Panics for unknown non-snapshot types, matching Go's boundary.
    pub fn reset(&mut self, rate_per_sec: f64, typ: Type) {
        if typ == Type::SEND_SNAPSHOT {
            return;
        }
        if self.rate(typ) != rate_per_sec {
            self.limits[typ.as_i64() as usize] = Limit::new(rate_per_sec);
        }
    }
}
