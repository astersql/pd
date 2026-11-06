// Copyright 2026 AsterSQL.
//! A keyspace-wide service budget shared by every resource group.
use crate::rm_slot::{go_max, go_min};
#[derive(Clone, Debug)]
pub struct ServiceLimit {
    pub rate: f64,
    pub available: f64,
    pub last_update: i64,
}
impl ServiceLimit {
    pub fn new(rate: f64, now: i64) -> Self {
        Self {
            rate: go_max(0.0, rate),
            available: 0.0,
            last_update: now,
        }
    }
    pub fn refill(&mut self, now: i64) {
        if self.rate <= 0.0 {
            return;
        }
        let elapsed = now.saturating_sub(self.last_update) as f64 / 1e9;
        if elapsed < 0.0 {
            return;
        }
        self.available = go_min(self.available + self.rate * elapsed, self.rate * 5.0);
        self.last_update = now;
    }
    /// Update first, then refill using the new rate and its new burst ceiling.
    pub fn set(&mut self, rate: f64, now: i64) -> bool {
        let rate = go_max(0.0, rate);
        if rate == self.rate {
            return false;
        }
        let old = self.rate;
        self.rate = rate;
        if old == 0.0 || rate == 0.0 {
            self.available = 0.0;
            self.last_update = now;
        } else {
            self.refill(now);
        }
        true
    }
    pub fn apply(&mut self, now: i64, requested: f64) -> (f64, i64) {
        if self.rate <= 0.0 {
            return (requested, 0);
        }
        self.refill(now);
        let granted = if requested <= self.available {
            self.available -= requested;
            requested
        } else {
            let granted = go_max(0.0, self.available);
            self.available = 0.0;
            granted
        };
        (granted, (granted * 1000.0 / self.rate).round() as i64)
    }
}
