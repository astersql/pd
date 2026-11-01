// Copyright 2026 AsterSQL.
//! Per-term, cached lease telemetry; the host owns registrations.
use prometheus::{Counter, Histogram};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
pub(crate) struct LeaseMetrics {
    requests: [Histogram; 3],
    responses: Histogram,
    ticks: Histogram,
    reasons: [Counter; 3],
    reported: [AtomicBool; 3],
    last_start: Mutex<Option<Instant>>,
    last_response: Mutex<Option<Instant>>,
    _remaining: Arc<dyn Fn() -> f64 + Send + Sync>,
}
impl LeaseMetrics {
    pub(crate) fn new(
        term: &Arc<crate::Leadership>,
        scope: &astersql_pd_metricutil::ClassicMetrics,
    ) -> Self {
        let purpose = "leader election";
        let weak = Arc::downgrade(term);
        let remaining: Arc<dyn Fn() -> f64 + Send + Sync> =
            Arc::new(move || weak.upgrade().map_or(0., |term| term.remaining_ttl()));
        scope.lease_ttl.bind(purpose, Arc::downgrade(&remaining));
        Self {
            requests: ["success", "error", "canceled"].map(|result| {
                scope
                    .keep_alive_request_duration
                    .with_label_values(&[purpose, result])
            }),
            responses: scope
                .keep_alive_response_interval
                .with_label_values(&[purpose]),
            ticks: scope.keep_alive_tick_interval.with_label_values(&[purpose]),
            reasons: ["invalid_ttl", "lease_expired", "context_canceled"].map(|reason| {
                scope
                    .renewal_termination_total
                    .with_label_values(&[purpose, reason])
            }),
            reported: Default::default(),
            last_start: Default::default(),
            last_response: Default::default(),
            _remaining: remaining,
        }
    }
    pub(crate) fn request(&self) -> RequestMetrics<'_> {
        let now = Instant::now();
        if let Some(last) = self.last_start.lock().unwrap().replace(now) {
            self.ticks
                .observe(now.saturating_duration_since(last).as_secs_f64());
        }
        RequestMetrics {
            metrics: self,
            begin: now,
            result: 2,
        }
    }
    pub(crate) fn response(&self, ttl: i64) {
        if ttl <= 0 {
            self.terminate(0);
            return;
        }
        let now = Instant::now();
        if let Some(last) = self.last_response.lock().unwrap().replace(now) {
            self.responses
                .observe(now.saturating_duration_since(last).as_secs_f64());
        }
    }
    pub(crate) fn terminate(&self, reason: usize) {
        if !self.reported[reason].swap(true, Ordering::Relaxed) {
            self.reasons[reason].inc();
        }
    }
}
pub(crate) struct RequestMetrics<'a> {
    metrics: &'a LeaseMetrics,
    begin: Instant,
    result: usize,
}
impl RequestMetrics<'_> {
    pub(crate) fn complete<T>(&mut self, result: &crate::Result<T>) {
        self.result = match result {
            Ok(_) => 0,
            Err(crate::ElectionError::Cancelled) => 2,
            Err(_) => 1,
        };
    }
}
impl Drop for RequestMetrics<'_> {
    fn drop(&mut self) {
        self.metrics.requests[self.result].observe(self.begin.elapsed().as_secs_f64());
    }
}
