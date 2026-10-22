// Copyright 2026 AsterSQL.

const DEFAULT_DECAY: f64 = 0.075;

/// An exponential moving average that uses an arithmetic mean while warming up.
#[derive(Debug)]
pub struct Ema {
    decay: f64,
    wake_num: u64,
    count: u64,
    value: f64,
    instantaneous: f64,
}

impl Ema {
    /// Creates an EMA with the default decay.
    pub fn new() -> Self {
        Self::with_decay(DEFAULT_DECAY)
    }

    /// Creates an EMA with `decay`, falling back to the default when it is not in `(0, 1)`.
    pub fn with_decay(decay: f64) -> Self {
        let decay = if decay > 0.0 && decay < 1.0 {
            decay
        } else {
            DEFAULT_DECAY
        };

        Self {
            decay,
            wake_num: ((2.0 - decay) / decay) as u64,
            count: 0,
            value: 0.0,
            instantaneous: 0.0,
        }
    }

    /// Adds a data point.
    pub fn add(&mut self, value: f64) {
        self.instantaneous = value;
        if self.count < self.wake_num {
            self.count += 1;
            self.value += value;
            return;
        }

        if self.count == self.wake_num {
            self.count += 1;
            self.value /= self.wake_num as f64;
        }
        self.value = value * self.decay + self.value * (1.0 - self.decay);
    }

    /// Returns the current average, or zero before any data has been added.
    pub fn get(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        if self.count <= self.wake_num {
            return self.value / self.count as f64;
        }
        self.value
    }

    /// Clears the accumulated average without changing the last instantaneous value.
    pub fn reset(&mut self) {
        self.count = 0;
        self.value = 0.0;
    }

    /// Replaces the accumulated average with one data point.
    pub fn set(&mut self, value: f64) {
        self.value = value;
        self.count = 1;
    }

    /// Returns the value most recently passed to [`Self::add`].
    pub fn get_instantaneous(&self) -> f64 {
        self.instantaneous
    }
}

impl Default for Ema {
    fn default() -> Self {
        Self::new()
    }
}
