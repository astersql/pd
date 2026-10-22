// Copyright 2026 AsterSQL.
//! Space and slow-heartbeat predicates, independent of transport.
impl super::StoreInfo {
    /// Return the witness count plus delta, preserving Go signed wrapping.
    pub fn witness_score(&self, delta: i64) -> f64 {
        self.witness_count().wrapping_add(delta) as f64
    }

    pub fn storage_size(&self) -> u64 {
        self.used_size()
    }
    pub fn available_ratio(&self) -> f64 {
        let stats = self.stats.read().unwrap();
        if stats.raw.capacity == 0 {
            0.0
        } else {
            stats.raw.available as f64 / stats.raw.capacity as f64
        }
    }
    pub fn is_low_space(&self, low_space_ratio: f64) -> bool {
        let stats = self.stats.read().unwrap();
        let raw = &stats.raw;
        if raw.capacity == 0 || (self.region_count < 100 && raw.available > 8 * 1024 * 1024 * 1024)
        {
            return false;
        }
        (raw.available as f64 / raw.capacity as f64) < 1.0 - low_space_ratio
    }
    pub fn slow_score(&self) -> u64 {
        self.stats.read().unwrap().raw.slow_score
    }
    /// Return an independent snapshot; callers cannot mutate the heartbeat map.
    pub fn network_slow_scores(&self) -> std::collections::HashMap<u64, u64> {
        self.stats.read().unwrap().raw.network_slow_scores.clone()
    }
    pub fn is_slow(&self) -> bool {
        self.is_evicted_as_slow_trend() || self.slow_score() >= 80
    }
    pub fn min_resolved_ts(&self) -> u64 {
        self.min_resolved_ts
    }
    pub fn set_min_resolved_ts(&mut self, ts: u64) {
        self.min_resolved_ts = ts;
    }
    pub fn is_available_for_min_resolved_ts(&self) -> bool {
        !self.is_removed() && self.is_tikv() && self.leader_count() != 0
    }
}
