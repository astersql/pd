// Copyright 2026 AsterSQL.
//! Store limiter assembly, retaining shared state across store clones.
use std::sync::{Arc, RwLock};

use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_storelimit::{REGION_INFLUENCE, SlidingWindows, StoreRateLimit, Type};

use crate::StoreInfo;

/// The supported real limiter implementations. Admission uses the host runtime.
pub enum StoreLimiter {
    V1(StoreRateLimit),
    V2(SlidingWindows),
}

impl StoreLimiter {
    pub fn version(&self) -> &'static str {
        match self {
            Self::V1(limit) => limit.version(),
            Self::V2(limit) => limit.version(),
        }
    }

    /// V2 reset is intentionally a no-op: feedback controls its capacity.
    pub fn reset(&mut self, rate: f64, typ: Type) {
        if let Self::V1(limit) = self {
            limit.reset(rate, typ);
        }
    }

    /// Returns the v1 region rate. V2 has no region-rate contract.
    pub fn rate(&self, typ: Type) -> f64 {
        match self {
            Self::V1(limit) => limit.rate(typ),
            Self::V2(_) => panic!("v2 has no region rate"),
        }
    }

    pub fn feedback(&mut self, error: f64) {
        match self {
            Self::V1(limit) => limit.feedback(error),
            Self::V2(limit) => limit.feedback(error),
        }
    }

    pub async fn available(&self, cost: i64, typ: Type, level: PriorityLevel) -> bool {
        match self {
            Self::V1(limit) => limit.available(cost, typ, level).await,
            Self::V2(limit) => limit.available(cost, typ, level),
        }
    }

    pub async fn take(&mut self, cost: i64, typ: Type, level: PriorityLevel) -> bool {
        match self {
            Self::V1(limit) => limit.take(cost, typ, level).await,
            Self::V2(limit) => limit.take(cost, typ, level),
        }
    }

    pub fn ack(&mut self, cost: i64, typ: Type) {
        match self {
            Self::V1(limit) => limit.ack(cost, typ),
            Self::V2(limit) => limit.ack(cost, typ),
        }
    }
}

impl StoreInfo {
    /// Replaces this store's limiter; existing clones keep their old handle.
    pub fn set_store_limit(&mut self, limit: Option<StoreLimiter>) {
        self.limiter = limit.map(|limit| Arc::new(RwLock::new(limit)));
    }

    /// Panics on a nil limiter, as does Go's LimitVersion.
    pub fn limit_version(&self) -> &'static str {
        self.limiter
            .as_ref()
            .expect("nil store limiter")
            .read()
            .unwrap()
            .version()
    }

    /// Uses the first rate, or zero if none is provided. Existing clones observe reset.
    pub fn reset_store_limit(&self, typ: Type, rates: &[f64]) {
        self.limiter
            .as_ref()
            .expect("nil store limiter")
            .write()
            .unwrap()
            .reset(rates.first().copied().unwrap_or(0.0), typ);
    }

    /// Feedback on a nil limiter is a no-op.
    pub fn feedback(&self, error: f64) {
        if let Some(limit) = &self.limiter {
            limit.write().unwrap().feedback(error);
        }
    }

    /// Checks the normal region influence at the requested priority.
    /// The read guard serializes reset with v1 admission; callers
    /// must not retain another write guard for this shared limiter across await.
    pub async fn is_available(&self, typ: Type, level: PriorityLevel) -> bool {
        let cost = REGION_INFLUENCE[typ.as_i64() as usize];
        self.limiter
            .as_ref()
            .expect("nil store limiter")
            .read()
            .unwrap()
            .available(cost, typ, level)
            .await
    }
}
