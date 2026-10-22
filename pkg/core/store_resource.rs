// Copyright 2026 AsterSQL.
//! Store resource counters and weighted leader scoring, matching store.go.
use crate::StoreInfo;
use astersql_pd_core_constant::{ResourceKind, SchedulePolicy};

const MIN_WEIGHT: f64 = 1e-6;

impl StoreInfo {
    pub fn leader_count(&self) -> i64 {
        self.leader_count
    }
    pub fn region_count(&self) -> i64 {
        self.region_count
    }
    pub fn learner_count(&self) -> i64 {
        self.learner_count
    }
    pub fn witness_count(&self) -> i64 {
        self.witness_count
    }
    pub fn pending_peer_count(&self) -> i64 {
        self.pending_peer_count
    }
    pub fn leader_size(&self) -> i64 {
        self.leader_size
    }
    pub fn region_size(&self) -> i64 {
        self.region_size
    }
    /// Assign the resource value without normalization, as Go options do.
    pub fn set_leader_count(&mut self, value: i64) {
        self.leader_count = value;
    }
    /// Assign the resource value without normalization, as Go options do.
    pub fn set_region_count(&mut self, value: i64) {
        self.region_count = value;
    }
    /// Assign the resource value without normalization, as Go options do.
    pub fn set_learner_count(&mut self, value: i64) {
        self.learner_count = value;
    }
    /// Assign the resource value without normalization, as Go options do.
    pub fn set_witness_count(&mut self, value: i64) {
        self.witness_count = value;
    }
    /// Assign the resource value without normalization, as Go options do.
    pub fn set_pending_peer_count(&mut self, value: i64) {
        self.pending_peer_count = value;
    }
    /// Assign the resource value without normalization, as Go options do.
    pub fn set_leader_size(&mut self, value: i64) {
        self.leader_size = value;
    }
    /// Assign the resource value without normalization, as Go options do.
    pub fn set_region_size(&mut self, value: i64) {
        self.region_size = value;
    }
    /// Assign the resource value without normalization, as Go options do.
    pub fn set_leader_weight(&mut self, value: f64) {
        self.leader_weight = value;
    }
    /// Assign the resource value without normalization, as Go options do.
    pub fn set_region_weight(&mut self, value: f64) {
        self.region_weight = value;
    }
    /// Project signed counts with Go's wrapping uint64 conversion.
    pub fn resource_count(&self, kind: ResourceKind) -> u64 {
        match kind {
            ResourceKind::LEADER => self.leader_count as u64,
            ResourceKind::REGION => self.region_count as u64,
            _ => 0,
        }
    }
    pub fn resource_size(&self, kind: ResourceKind) -> i64 {
        match kind {
            ResourceKind::LEADER => self.leader_size,
            ResourceKind::REGION => self.region_size,
            _ => 0,
        }
    }
    /// Only non-positive weights are clamped; tiny positive values and NaN survive.
    pub fn resource_weight(&self, kind: ResourceKind) -> f64 {
        let weight = match kind {
            ResourceKind::LEADER => self.leader_weight,
            ResourceKind::REGION => self.region_weight,
            _ => return 0.0,
        };
        if weight <= 0.0 { MIN_WEIGHT } else { weight }
    }
    /// Add delta before conversion, preserving Go signed overflow and NaN rules.
    pub fn leader_score(&self, policy: SchedulePolicy, delta: i64) -> f64 {
        let value = match policy {
            SchedulePolicy::BY_SIZE => self.leader_size,
            SchedulePolicy::BY_COUNT => self.leader_count,
            _ => return 0.0,
        };
        let divisor = if self.leader_weight.is_nan() {
            f64::NAN
        } else {
            self.leader_weight.max(MIN_WEIGHT)
        };
        value.wrapping_add(delta) as f64 / divisor
    }
}
