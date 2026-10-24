// Copyright 2026 AsterSQL.
use std::{sync::Arc, time::Duration};

use prometheus::{Histogram, HistogramOpts, IntCounterVec, Opts, Registry};
use tokio::{
    sync::{Mutex, oneshot},
    task::JoinHandle,
};

use crate::{LabelerError, RegionLabeler, Timestamp};

/// Collectors owned and registered by the host, never a global registry.
pub struct LabelerMetrics {
    pub events: IntCounterVec,
    pub creation_duration: Histogram,
}
impl LabelerMetrics {
    /// Preserve the Go names, help, labels and exponential histogram buckets.
    pub fn register(registry: &Registry) -> Result<Self, prometheus::Error> {
        let events = IntCounterVec::new(
            Opts::new(
                "pd_schedule_labeler_event_counter",
                "Counter of the scheduler label.",
            ),
            &["type", "event"],
        )?;
        let creation_duration = Histogram::with_opts(
            HistogramOpts::new(
                "pd_schedule_new_region_labeler_duration_seconds",
                "Bucketed histogram of processing time (s) of creating new region labeler.",
            )
            .buckets((0..13).map(|n| 0.001 * 2_f64.powi(n)).collect()),
        )?;
        registry.register(Box::new(events.clone()))?;
        if let Err(error) = registry.register(Box::new(creation_duration.clone())) {
            let _ = registry.unregister(Box::new(events));
            return Err(error);
        }
        Ok(Self {
            events,
            creation_duration,
        })
    }
}

impl RegionLabeler {
    /// Count effective rules and their original key ranges, independently of GC cadence.
    pub fn get_rule_and_key_range_counts(&self, now: Timestamp) -> (usize, usize) {
        let rules = self.get_all_label_rules(now);
        (
            rules.len(),
            rules
                .iter()
                .map(|r| r.key_ranges().unwrap_or_default().len())
                .sum(),
        )
    }
    /// Remove expired labels and persist each changed rule, continuing after I/O errors.
    /// As in Go, memory cleanup precedes persistence; failed deletions retain an empty rule.
    pub async fn clear_expired_labels(&mut self, now: Timestamp) -> Vec<LabelerError> {
        if self.index.min_expire().is_none_or(|expiry| expiry > now) {
            return Vec::new();
        }
        let ids: Vec<_> = self.index.rules().map(|r| r.id.clone()).collect();
        let mut errors = Vec::new();
        for id in ids {
            if !self.index.remove_expired_labels(&id, now) {
                continue;
            }
            let rule = self.index.rule(&id).unwrap().clone();
            let result = if rule.labels.is_empty() {
                self.delete_label_rule(&id).await
            } else {
                self.storage
                    .run_in_txn(async |txn| {
                        self.storage.save_region_rule(txn, &self.paths, &id, &rule)
                    })
                    .await
                    .map_err(LabelerError::from)
            };
            if let Err(error) = result {
                errors.push(error);
            }
        }
        errors
    }
}

/// Host-owned cancellation and join handle for periodic cleanup.
pub struct LabelerGc {
    cancel: Option<oneshot::Sender<()>>,
    task: JoinHandle<Vec<LabelerError>>,
}
impl LabelerGc {
    /// Start on the current runtime. First cleanup occurs after one interval.
    /// The supplied clock maps the host's time to Unix nanoseconds.
    pub fn start(
        owner: Arc<Mutex<RegionLabeler>>,
        interval: Duration,
        now: impl Fn() -> Timestamp + Send + 'static,
    ) -> Self {
        assert!(!interval.is_zero(), "GC interval must be positive");
        let (cancel, mut cancelled) = oneshot::channel();
        let task = tokio::spawn(async move {
            let mut ticker =
                tokio::time::interval_at(tokio::time::Instant::now() + interval, interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut errors = Vec::new();
            loop {
                tokio::select! {
                    biased;
                    _ = &mut cancelled => return errors,
                    _ = ticker.tick() => {
                        tokio::select! {
                            biased;
                            _ = &mut cancelled => return errors,
                            mut guard = owner.lock() => errors = guard.clear_expired_labels(now()).await,
                        }
                    }
                }
            }
        });
        Self {
            cancel: Some(cancel),
            task,
        }
    }
    /// Cancel and wait for any in-flight storage operation to finish.
    /// Returns errors from the latest sweep; earlier sweeps are not accumulated.
    pub async fn shutdown(mut self) -> Result<Vec<LabelerError>, tokio::task::JoinError> {
        if let Some(cancel) = self.cancel.take() {
            let _ = cancel.send(());
        }
        (&mut self.task).await
    }
}
impl Drop for LabelerGc {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            let _ = cancel.send(());
        }
    }
}
