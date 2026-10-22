// Copyright 2026 AsterSQL.

use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_ratelimit::RateLimiter;

use crate::{INFLUENCE, STORE_LIMIT_TYPE_LEN, Type, UNLIMITED, VERSION_V1};

/// The v1 store limiter, with an independent token bucket for each limit type.
///
/// Admission uses the caller's runtime and the leaf limiter's monotonic clock.
/// Snapshot requests bypass the buckets; priority does not affect admission.
pub struct StoreRateLimit {
    pub(super) limits: [Limit; STORE_LIMIT_TYPE_LEN],
}

pub(super) struct Limit {
    rate_per_sec: f64,
    limiter: Option<RateLimiter>,
}

impl Limit {
    pub(super) fn new(rate_per_sec: f64) -> Self {
        let capacity = if rate_per_sec >= UNLIMITED {
            UNLIMITED as i64
        } else if rate_per_sec > 1.0 {
            (rate_per_sec * INFLUENCE as f64) as i64
        } else {
            INFLUENCE
        };
        let refill = if rate_per_sec >= UNLIMITED {
            rate_per_sec
        } else {
            rate_per_sec * INFLUENCE as f64
        };
        Self {
            rate_per_sec,
            // Go's initial Reset(0) leaves the bucket absent and bypasses it.
            limiter: (rate_per_sec != 0.0).then(|| RateLimiter::new(refill, capacity)),
        }
    }
}

impl StoreRateLimit {
    /// Creates initially full, independent buckets at the given region rate.
    pub fn new(rate_per_sec: f64) -> Self {
        Self {
            limits: std::array::from_fn(|_| Limit::new(rate_per_sec)),
        }
    }

    /// Checks available tokens without consuming them, ignoring priority.
    ///
    /// Panics for unknown non-snapshot types, matching Go's indexing boundary.
    pub async fn available(&self, cost: i64, typ: Type, _priority: PriorityLevel) -> bool {
        if typ == Type::SEND_SNAPSHOT {
            return true;
        }
        match &self.limits[typ.as_i64() as usize].limiter {
            Some(limiter) => limiter.available(cost).await,
            None => true,
        }
    }

    /// Consumes tokens only if immediately available, ignoring priority.
    ///
    /// Panics for unknown non-snapshot types, matching Go's indexing boundary.
    pub async fn take(&self, cost: i64, typ: Type, _priority: PriorityLevel) -> bool {
        if typ == Type::SEND_SNAPSHOT {
            return true;
        }
        match &self.limits[typ.as_i64() as usize].limiter {
            Some(limiter) => limiter.allow_n(cost).await,
            None => true,
        }
    }

    /// Returns the configured region rate, before conversion to token units.
    pub fn rate(&self, typ: Type) -> f64 {
        self.limits[typ.as_i64() as usize].rate_per_sec
    }

    /// Reports the token bucket version.
    pub fn version(&self) -> &'static str {
        VERSION_V1
    }

    /// Acknowledges completion without refunding v1 tokens.
    pub fn ack(&self, _cost: i64, _typ: Type) {}

    /// Accepts feedback without changing v1 capacity or tokens.
    pub fn feedback(&self, _error: f64) {}
}
