// Copyright 2026 AsterSQL.
//! Resource-group refill, client shares, and loan state with a caller-supplied clock.
use crate::rm_slot::{Demand, Slot, go_min};
use crate::{Bucket, BucketState};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Default)]
pub struct BucketRuntime {
    pub(crate) has_tracker: bool,
    pub initialized: bool,
    pub last_update: Option<i128>,
    pub override_fill_rate: Option<f64>,
    pub override_burst_limit: Option<i64>,
    pub setting_changed: bool,
    pub reserved_burst: f64,
    pub reserved_service: f64,
    pub(crate) slots: BTreeMap<u64, Slot>,
    pub(crate) demands: BTreeMap<u64, Demand>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Limited,
    RateControlled,
    Unlimited,
    Moderated,
}
impl BucketRuntime {
    pub fn from_state(state: &BucketState) -> Self {
        Self {
            initialized: state.initialized,
            last_update: state
                .last_update
                .as_ref()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|t| {
                    t.timestamp() as i128 * 1_000_000_000 + t.timestamp_subsec_nanos() as i128
                }),
            ..Self::default()
        }
    }
    pub fn state(&self, bucket: &Bucket) -> BucketState {
        BucketState {
            tokens: bucket.tokens,
            initialized: self.initialized,
            last_update: self.last_update.and_then(|ns| {
                let seconds = i64::try_from(ns.div_euclid(1_000_000_000)).ok()?;
                let nanos = ns.rem_euclid(1_000_000_000) as u32;
                chrono::DateTime::from_timestamp(seconds, nanos)
                    .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true))
            }),
        }
    }
    fn mode(&self, bucket: &Bucket) -> Mode {
        if self.override_burst_limit.is_some_and(|v| v >= 0) {
            return Mode::Limited;
        }
        match bucket.settings.as_ref().map_or(0, |s| s.burst_limit) {
            -1 => Mode::Unlimited,
            -2 => Mode::Moderated,
            0 => Mode::RateControlled,
            _ => Mode::Limited,
        }
    }
    fn fill(&self, bucket: &Bucket) -> f64 {
        self.override_fill_rate
            .filter(|v| *v >= 0.0)
            .unwrap_or_else(|| bucket.settings.as_ref().map_or(0.0, |s| s.fill_rate as f64))
    }
    fn burst(&self, bucket: &Bucket) -> i64 {
        self.override_burst_limit
            .filter(|v| *v >= 0)
            .unwrap_or_else(|| bucket.settings.as_ref().map_or(0, |s| s.burst_limit))
    }
    fn effective(&self, bucket: &Bucket) -> (f64, i64) {
        if self.mode(bucket) == Mode::Moderated {
            let fill = go_min(self.fill(bucket) + 10000.0, i32::MAX as f64);
            (fill, fill as i64)
        } else {
            (self.fill(bucket), self.burst(bucket))
        }
    }
    fn reset_loan(&mut self, bucket: &mut Bucket) {
        self.setting_changed = false;
        bucket.tokens = 0.0;
        for slot in self.slots.values_mut() {
            slot.capacity = 0.0;
            slot.last_capacity = 0.0;
        }
    }
    fn balance(&mut self, bucket: &mut Bucket, now: i64, client: u64, required: f64, added: f64) {
        if required == 0.0 {
            self.slots.remove(&client);
        } else {
            self.slots.entry(client).or_default().last_request = now;
        }
        self.slots
            .retain(|_, s| now.saturating_sub(s.last_request) < 600_000_000_000);
        let count = self.slots.len();
        if count == 0 {
            return;
        }
        let (fill, burst) = self.effective(bucket);
        let ratio = 1.0 / count as f64;
        if matches!(self.mode(bucket), Mode::RateControlled | Mode::Unlimited) {
            let normal_fill = self.fill(bucket);
            let normal_burst = self.burst(bucket);
            for slot in self.slots.values_mut() {
                slot.fill_rate = normal_fill * ratio;
                slot.burst_limit = normal_burst;
            }
            return;
        }
        if count == 1 {
            for slot in self.slots.values_mut() {
                slot.capacity = if fill == 0.0 { 0.0 } else { bucket.tokens };
                slot.last_capacity = slot.capacity;
                slot.fill_rate = fill;
                slot.burst_limit = burst;
            }
            return;
        }
        if fill == 0.0 {
            bucket.tokens = 0.0;
            self.reserved_burst = 0.0;
            self.reserved_service = 0.0;
            for slot in self.slots.values_mut() {
                slot.fill_rate = 0.0;
                slot.burst_limit = 0;
                slot.capacity = 0.0;
                slot.last_capacity = 0.0;
            }
            return;
        }
        let configured_burst = bucket.settings.as_ref().map_or(0, |s| s.burst_limit);
        let budget = if self.override_burst_limit.is_some_and(|v| v > 0) && configured_burst < 0 {
            go_min(fill, self.override_burst_limit.unwrap() as f64)
        } else {
            fill
        };
        let basic = budget * ratio;
        let mut allocations = BTreeMap::new();
        let mut extra = BTreeMap::new();
        let mut extra_sum = 0.0;
        let mut allocated = 0.0;
        for id in self.slots.keys() {
            let demand = self.demands.entry(*id).or_default();
            let mut allocation = demand.rate;
            if allocation > basic {
                let value = allocation - basic;
                extra.insert(*id, value);
                extra_sum += value;
                allocation = basic;
            } else if !demand.initialized {
                allocation = basic;
            }
            allocations.insert(*id, allocation);
            allocated += allocation;
        }
        let remaining = budget - allocated;
        if remaining > 0.0 {
            if extra.is_empty() {
                for value in allocations.values_mut() {
                    *value += remaining / count as f64;
                }
            } else {
                for (id, value) in extra {
                    *allocations.get_mut(&id).unwrap() += remaining * (value / extra_sum);
                }
            }
        }
        for (id, slot) in &mut self.slots {
            let allocation = allocations[id];
            let ratio = allocation / budget;
            let slot_burst = burst as f64 * ratio;
            let mut tokens = added * ratio;
            if slot_burst > 0.0 && slot.capacity > slot_burst {
                let reserved = slot.capacity - slot_burst;
                self.reserved_burst += reserved;
                bucket.tokens -= reserved;
                tokens -= reserved;
            }
            slot.capacity += tokens;
            slot.last_capacity += tokens;
            slot.fill_rate = if budget != fill {
                fill * ratio
            } else {
                allocation
            };
            slot.burst_limit = slot_burst as i64;
        }
    }
    /// Request a client's RU grant. Non-finite grants reset loans and are withheld.
    pub fn request(
        &mut self,
        bucket: &mut Bucket,
        now: i64,
        required: f64,
        period_ms: u64,
        client: u64,
    ) -> Option<(Bucket, i64)> {
        bucket.settings.as_ref()?;
        self.has_tracker = true;
        self.demands
            .entry(client)
            .or_default()
            .sample(now, required);
        let burst = self.burst(bucket);
        let mut added = 0.0;
        if !self.initialized {
            if self.fill(bucket) == 0.0 {
                bucket.settings.as_mut().unwrap().fill_rate = 10000;
            }
            if bucket.tokens < 100000.0 && self.burst(bucket) > 0 {
                bucket.tokens = 100000.0;
            }
            self.last_update = Some(now as i128);
            self.initialized = true;
        } else if burst >= 0 && self.mode(bucket) == Mode::Limited {
            let delta = (now as i128 - self.last_update.unwrap_or(now as i128))
                .clamp(i64::MIN as i128, i64::MAX as i128) as i64;
            if delta > 0 {
                added = self.fill(bucket) * delta as f64 / 1e9
                    + self.reserved_burst
                    + self.reserved_service;
                self.reserved_burst = 0.0;
                self.reserved_service = 0.0;
                bucket.tokens += added;
            }
            if bucket.tokens > burst as f64 {
                let excess = bucket.tokens - burst as f64;
                added -= excess;
                bucket.tokens = burst as f64;
            }
            self.last_update = Some(now as i128);
        }
        if self.setting_changed && bucket.tokens <= 0.0 {
            added = 0.0;
            self.reset_loan(bucket);
        }
        self.balance(bucket, now, client, required, added);
        let Some(slot) = self.slots.get_mut(&client) else {
            return Some((
                Bucket {
                    settings: Some(crate::TokenLimits {
                        burst_limit: burst,
                        ..Default::default()
                    }),
                    tokens: 0.0,
                },
                0,
            ));
        };
        let (result, mut trickle) = slot.assign(required, period_ms);
        if result.tokens < 0.0 || !result.tokens.is_finite() || !slot.capacity.is_finite() {
            self.reset_loan(bucket);
            return None;
        }
        bucket.tokens -= slot.last_capacity - slot.capacity;
        slot.last_capacity = slot.capacity;
        if result.tokens > 0.0 && self.fill(bucket) >= i32::MAX as f64 {
            trickle = 0;
        }
        Some((result, trickle))
    }
}
