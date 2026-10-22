// Copyright 2026 AsterSQL.
use crate::{ConcurrencyLimiter, RateLimiter};
use astersql_pd_syncutil::asynchronous::Mutex;
use std::sync::Arc;

const EPS: f64 = 1e-8;

/// Go-compatible configuration update flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum UpdateStatus {
    NoChange = 2,
    Updated = 4,
    Deleted = 8,
}

struct State {
    concurrency: Arc<ConcurrencyLimiter>,
    rate: Option<Arc<RateLimiter>>,
}
impl State {
    fn is_empty(&self) -> bool {
        self.concurrency.get_limit() == 0 && self.rate.is_none()
    }
}

/// Configuration for the concurrency and QPS dimensions of one label.
/// Updates are serialized without creating a runtime. Existing handles remain
/// valid when disabling a dimension replaces or removes its limiter.
pub struct Limiter {
    state: Mutex<State>,
}
impl Default for Limiter {
    fn default() -> Self {
        Self::new()
    }
}
impl Limiter {
    /// Creates an unlimited concurrency limiter with no QPS bucket.
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State {
                concurrency: Arc::new(ConcurrencyLimiter::new(0)),
                rate: None,
            }),
        }
    }

    /// Returns the current concurrency limiter, retaining its lifetime.
    pub async fn concurrency_limiter(&self) -> Arc<ConcurrencyLimiter> {
        self.state.lock().await.concurrency.clone()
    }

    /// Returns the current QPS bucket, if configured.
    pub async fn rate_limiter(&self) -> Option<Arc<RateLimiter>> {
        self.state.lock().await.rate.clone()
    }

    /// Returns configured concurrency and admitted tasks.
    pub async fn concurrency_status(&self) -> (u64, u64) {
        let s = self.state.lock().await;
        (
            s.concurrency.get_limit(),
            s.concurrency.get_running_tasks_num(),
        )
    }

    /// Returns configured QPS and burst, or zeros when absent.
    pub async fn qps_status(&self) -> (f64, i64) {
        let s = self.state.lock().await;
        match &s.rate {
            Some(rate) => (rate.limit().await, rate.burst().await),
            None => (0.0, 0),
        }
    }

    /// Changes concurrency; disabling replaces the running-task counter.
    pub async fn update_concurrency_config(&self, limit: u64) -> UpdateStatus {
        let mut s = self.state.lock().await;
        if s.concurrency.get_limit() == limit {
            return UpdateStatus::NoChange;
        }
        if limit == 0 {
            s.concurrency = Arc::new(ConcurrencyLimiter::new(0));
            if s.is_empty() {
                return UpdateStatus::Deleted;
            }
        } else {
            s.concurrency.set_limit(limit);
        }
        UpdateStatus::Updated
    }

    /// Changes QPS and burst using Go's epsilon and deletion rules.
    /// With no existing bucket, even invalid settings create a bucket.
    pub async fn update_qps_config(&self, limit: f64, burst: i64) -> UpdateStatus {
        let mut s = self.state.lock().await;
        let (old_limit, old_burst) = match &s.rate {
            Some(rate) => (rate.limit().await, rate.burst().await),
            None => (0.0, 0),
        };
        if (old_limit - limit).abs() < EPS && old_burst == burst {
            return UpdateStatus::NoChange;
        }
        if let Some(rate) = &s.rate {
            if limit <= EPS || burst < 1 {
                s.rate = None;
                if s.is_empty() {
                    return UpdateStatus::Deleted;
                }
            } else {
                rate.set_limit(limit).await;
                rate.set_burst(burst).await;
            }
        } else {
            s.rate = Some(Arc::new(RateLimiter::new(limit, burst)));
        }
        UpdateStatus::Updated
    }
}
