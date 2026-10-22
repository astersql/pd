// Copyright 2026 AsterSQL.
//! Host-owned metrics and explicit-clock heartbeat stage observation.
use prometheus::{Counter, CounterVec, Opts, Registry};
use std::time::Duration;

/// A bounded heartbeat stage, preserving the Go metric label values.
#[derive(Clone, Copy, Debug)]
#[repr(usize)]
pub enum Stage {
    PreCheck,
    AsyncHotStats,
    RegionGuide,
    CheckOverlaps,
    ValidateRegion,
    SetRegion,
    UpdateSubTree,
    CollectRegionStats,
    Other,
}
impl Stage {
    pub const ALL: [Self; 9] = [
        Self::PreCheck,
        Self::AsyncHotStats,
        Self::RegionGuide,
        Self::CheckOverlaps,
        Self::ValidateRegion,
        Self::SetRegion,
        Self::UpdateSubTree,
        Self::CollectRegionStats,
        Self::Other,
    ];
    pub fn label(self) -> &'static str {
        [
            "PreCheck",
            "AsyncHotStatsDuration",
            "RegionGuide",
            "SaveCache_CheckOverlaps",
            "SaveCache_InvalidRegion",
            "SaveCache_SetRegion",
            "SaveCache_UpdateSubTree",
            "CollectRegionStats",
            "Other",
        ][self as usize]
    }
}

/// Cached counters registered only in the registry supplied by the host.
pub struct HeartbeatMetrics {
    stages: [(Counter, Counter); 9],
}
impl HeartbeatMetrics {
    pub fn register(registry: &Registry) -> Result<Self, prometheus::Error> {
        let sum = CounterVec::new(
            Opts::new(
                "pd_core_region_heartbeat_breakdown_handle_duration_seconds_sum",
                "Bucketed histogram of processing time (s) of handle the heartbeat stage.",
            ),
            &["name"],
        )?;
        let count = CounterVec::new(
            Opts::new(
                "pd_core_region_heartbeat_breakdown_handle_duration_seconds_count",
                "Bucketed histogram of processing count of handle the heartbeat stage.",
            ),
            &["name"],
        )?;
        let stages = Stage::ALL.map(|stage| {
            (
                sum.with_label_values(&[stage.label()]),
                count.with_label_values(&[stage.label()]),
            )
        });
        registry.register(Box::new(sum.clone()))?;
        if let Err(err) = registry.register(Box::new(count)) {
            registry.unregister(Box::new(sum))?;
            return Err(err);
        }
        Ok(Self { stages })
    }
    pub fn duration(&self, stage: Stage) -> f64 {
        self.stages[stage as usize].0.get()
    }
    pub fn count(&self, stage: Stage) -> f64 {
        self.stages[stage as usize].1.get()
    }
}

/// Per-heartbeat checkpoint. The host supplies a clock reading on a common epoch.
/// No runtime, global registry, background worker or object pool is installed.
/// Call `begin` before finishing stages; both SaveCache boundaries reset the window.
pub struct HeartbeatTracer<'a, C> {
    metrics: &'a HeartbeatMetrics,
    clock: C,
    checkpoint: Duration,
}
impl<'a, C: Fn() -> Duration> HeartbeatTracer<'a, C> {
    pub fn new(metrics: &'a HeartbeatMetrics, clock: C) -> Self {
        Self {
            metrics,
            clock,
            checkpoint: Duration::ZERO,
        }
    }
    pub fn begin(&mut self) {
        self.reset_checkpoint();
    }
    pub fn reset_checkpoint(&mut self) {
        self.checkpoint = (self.clock)();
    }
    pub fn on_save_cache_begin(&mut self) {
        self.reset_checkpoint();
    }
    pub fn on_save_cache_finished(&mut self) {
        self.reset_checkpoint();
    }
    pub fn finish(&mut self, stage: Stage) {
        let now = (self.clock)();
        let elapsed = now.saturating_sub(self.checkpoint);
        self.checkpoint = now;
        let (sum, count) = &self.metrics.stages[stage as usize];
        if !elapsed.is_zero() {
            sum.inc_by(elapsed.as_secs_f64());
        }
        count.inc();
    }
}
