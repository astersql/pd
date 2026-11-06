// Copyright 2026 AsterSQL.
//! Per-client loan accounting; callers supply time and enforce synchronization.
use crate::{Bucket, TokenLimits};
#[derive(Clone, Debug, Default)]
pub(crate) struct Demand {
    pub last_sample: Option<i64>,
    pub initialized: bool,
    pub rate: f64,
}
impl Demand {
    pub fn sample(&mut self, now: i64, amount: f64) {
        let previous = self.last_sample.replace(now);
        let Some(previous) = previous else {
            return;
        };
        let elapsed = now.saturating_sub(previous) as f64 / 1e9;
        if elapsed <= 0.0 {
            return;
        }
        let rate = go_max(0.0, amount) / elapsed;
        if !self.initialized {
            self.initialized = true;
            self.rate = rate;
            return;
        }
        let decay = (-std::f64::consts::LN_2 / 5.0 * elapsed).exp();
        self.rate = decay * self.rate + (1.0 - decay) * rate;
        if rate == 0.0 && self.rate < 1.0 {
            self.rate = 0.0;
        }
    }
}
pub(crate) fn go_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}
pub(crate) fn go_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}
#[derive(Clone, Debug, Default)]
pub(crate) struct Slot {
    pub fill_rate: f64,
    pub burst_limit: i64,
    pub capacity: f64,
    pub last_capacity: f64,
    pub last_request: i64,
}
impl Slot {
    pub fn assign(&mut self, mut required: f64, period_ms: u64) -> (Bucket, i64) {
        let mut result = Bucket {
            settings: Some(TokenLimits {
                burst_limit: self.burst_limit,
                ..Default::default()
            }),
            tokens: 0.0,
        };
        if self.burst_limit == -1 {
            result.tokens = required;
            return (result, 0);
        }
        if required <= 0.0 {
            return (result, 0);
        }
        if self.fill_rate == 0.0 {
            self.capacity = 0.0;
            self.last_capacity = 0.0;
            return (result, period_ms as i64);
        }
        if self.capacity >= required {
            self.capacity -= required;
            result.tokens = required;
            return (result, 0);
        }
        let mut granted = 0.0;
        let mut consumed_existing = false;
        if self.capacity > 0.0 {
            granted = self.capacity;
            required -= granted;
            self.capacity = 0.0;
            consumed_existing = true;
        }
        // Go time.Duration multiplication wraps before converting to seconds.
        let period_ns = (period_ms as i64).wrapping_mul(1_000_000);
        let seconds = period_ns as f64 / 1e9;
        let coefficient = if self.burst_limit > 0 && self.burst_limit as f64 <= self.fill_rate {
            1
        } else {
            2
        };
        let mut thresholds = [0.0; 2];
        thresholds[0] = coefficient as f64 * self.fill_rate * seconds;
        for i in 1..coefficient {
            thresholds[i] = (coefficient - i) as f64 * self.fill_rate * seconds + thresholds[i - 1];
        }
        let mut trickle = 0.0;
        for (i, threshold) in thresholds.iter().enumerate().take(coefficient) {
            if !(required > 0.0 && trickle < seconds) {
                break;
            }
            let loan = -self.capacity;
            if loan >= *threshold {
                continue;
            }
            let reserve = *threshold - loan;
            let rate = (coefficient - i) as f64 * self.fill_rate;
            if reserve > required {
                self.capacity -= required;
                granted += required;
                trickle += granted / rate;
                required = 0.0;
            } else {
                let reserve_time = reserve / rate;
                if reserve_time + trickle >= seconds {
                    let tokens = (seconds - trickle) * rate;
                    required -= tokens;
                    self.capacity -= tokens;
                    granted += tokens;
                    trickle = seconds;
                } else {
                    granted += reserve;
                    required -= reserve;
                    self.capacity -= reserve;
                    trickle += reserve_time;
                }
            }
        }
        if required > 0.0 && granted < 0.5 * self.fill_rate * seconds {
            let reserved = go_min(required + granted, 0.5 * self.fill_rate * seconds);
            self.capacity -= reserved - granted;
            granted = reserved;
        }
        result.tokens = granted;
        let nanos = if consumed_existing {
            (go_min(trickle, seconds) * 1e9) as i64
        } else {
            period_ns
        };
        (result, nanos / 1_000_000)
    }
}
