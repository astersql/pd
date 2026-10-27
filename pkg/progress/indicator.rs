// Copyright 2026 AsterSQL.
use std::{
    collections::VecDeque,
    fmt,
    time::{Duration, SystemTime},
};

const MIN_WINDOW: Duration = Duration::from_secs(600);
const MAX_WINDOW: Duration = Duration::from_secs(7200);

/// Progress is the online/offline store progress snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct Progress {
    pub action: String,
    pub progress_percent: f64,
    pub left_second: f64,
    pub current_speed: f64,
}

/// InvalidUpdateInterval rejects a zero interval before dividing durations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidUpdateInterval;
impl fmt::Display for InvalidUpdateInterval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("update interval must be positive")
    }
}
impl std::error::Error for InvalidUpdateInterval {}

/// ProgressIndicator tracks sampled cumulative work using a bounded history.
#[derive(Clone, Debug)]
pub struct ProgressIndicator {
    progress: Progress,
    target_region_size: f64,
    history: VecDeque<f64>,
    window_capacity: usize,
    window_length: usize,
    update_interval: Duration,
    current_window_length: usize,
    complete_at: Option<SystemTime>,
}
impl ProgressIndicator {
    /// Creates a tracker. Like Go, initial percent is the unmodified current/total.
    pub fn new(
        action: impl Into<String>,
        current: f64,
        total: f64,
        update_interval: Duration,
    ) -> Result<Self, InvalidUpdateInterval> {
        if update_interval.is_zero() {
            return Err(InvalidUpdateInterval);
        }
        let update_interval = update_interval.min(MIN_WINDOW);
        Ok(Self {
            progress: Progress {
                action: action.into(),
                progress_percent: current / total,
                left_second: f64::MAX,
                current_speed: 0.0,
            },
            target_region_size: total,
            history: VecDeque::from([current]),
            window_capacity: (MAX_WINDOW.as_nanos() / update_interval.as_nanos()) as usize + 1,
            window_length: (MIN_WINDOW.as_nanos() / update_interval.as_nanos()) as usize,
            update_interval,
            current_window_length: 1,
            complete_at: None,
        })
    }
    /// Returns the current immutable value.
    pub fn progress(&self) -> &Progress {
        &self.progress
    }
    /// Returns the target amount of cumulative work.
    pub fn target_region_size(&self) -> f64 {
        self.target_region_size
    }
    /// Raises the target when a preparing store needs more work.
    pub fn raise_target(&mut self, target: f64) {
        if self.target_region_size < target {
            self.target_region_size = target;
        }
    }
    /// Returns the latest completion timestamp, if any.
    pub fn complete_at(&self) -> Option<SystemTime> {
        self.complete_at
    }
    /// Returns retained sample count, bounded independently of the active window.
    pub fn history_len(&self) -> usize {
        self.history.len()
    }
    /// Returns the maximum retained sample count.
    pub fn window_capacity(&self) -> usize {
        self.window_capacity
    }
    /// Returns the configured active window length.
    pub fn window_length(&self) -> usize {
        self.window_length
    }
    /// Returns the active sample count.
    pub fn current_window_length(&self) -> usize {
        self.current_window_length
    }
    /// Returns the first sample used for calculating speed.
    pub fn front(&self) -> f64 {
        self.history[self.history.len() - self.current_window_length()]
    }
    /// Adjusts the active window for patrol duration; takes effect on next push.
    pub fn adjust_window_length(&mut self, duration: Duration) {
        self.window_length = (duration.clamp(MIN_WINDOW, MAX_WINDOW).as_nanos()
            / self.update_interval.as_nanos()) as usize
            + 1;
    }
    /// Samples cumulative work using the system clock for completion.
    pub fn push(&mut self, data: f64) {
        self.push_at(data, SystemTime::now());
    }
    /// Samples cumulative work using a host supplied completion timestamp.
    pub fn push_at(&mut self, data: f64, now: SystemTime) {
        self.history.push_back(data);
        self.current_window_length = self.history.len().min(self.window_length);
        while self.history.len() > self.window_capacity {
            self.history.pop_front();
        }
        self.update_progress(now);
    }
    fn update_progress(&mut self, now: SystemTime) {
        if self.target_region_size == 0.0 {
            self.progress.progress_percent = 1.0;
            self.progress.left_second = 0.0;
            self.progress.current_speed = 0.0;
            self.complete_at = Some(now);
            return;
        }
        let mut current = *self
            .history
            .back()
            .expect("history always contains a sample");
        let mut speed = if self.current_window_length <= 1 {
            0.0
        } else {
            (current - self.front())
                / ((self.current_window_length - 1) as f64 * self.update_interval.as_secs_f64())
        };
        if speed < 0.0 {
            speed = 0.0;
        }
        if current >= self.target_region_size {
            current = self.target_region_size;
            self.complete_at = Some(now);
        }
        self.progress.progress_percent = current / self.target_region_size;
        self.progress.current_speed = speed;
        let left = (self.target_region_size - current) / speed;
        self.progress.left_second = if left.is_finite() { left } else { f64::MAX };
    }
}
