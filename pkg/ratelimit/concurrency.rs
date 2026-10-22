// Copyright 2026 AsterSQL.
use astersql_pd_syncutil::blocking::Mutex;

#[derive(Debug)]
struct State {
    current: u64,
    limit: u64,
    max_concurrency: u64,
}

/// Limits concurrent tasks using immediate admission decisions.
///
/// A zero limit permits unlimited admission. Lowering the limit leaves running
/// tasks intact; callers must pair each successful admission with a release.
#[derive(Debug)]
pub struct ConcurrencyLimiter {
    state: Mutex<State>,
}

impl ConcurrencyLimiter {
    /// Creates a limiter with no running tasks.
    pub fn new(limit: u64) -> Self {
        Self {
            state: Mutex::new(State {
                current: 0,
                limit,
                max_concurrency: 0,
            }),
        }
    }

    /// Attempts admission without waiting for running tasks to finish.
    pub fn allow(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.limit != 0 && state.current >= state.limit {
            return false;
        }
        state.current += 1;
        state.max_concurrency = state.max_concurrency.max(state.current);
        true
    }

    /// Releases one running task; releasing at zero has no effect.
    pub fn release(&self) {
        let mut state = self.state.lock().unwrap();
        state.current = state.current.saturating_sub(1);
    }

    /// Returns the configured limit, with zero representing unlimited admission.
    pub fn get_limit(&self) -> u64 {
        self.state.lock().unwrap().limit
    }

    /// Changes the limit without evicting running tasks.
    pub fn set_limit(&self, limit: u64) {
        self.state.lock().unwrap().limit = limit;
    }

    /// Returns the number of admitted tasks awaiting release.
    pub fn get_running_tasks_num(&self) -> u64 {
        self.state.lock().unwrap().current
    }

    /// Returns the peak and resets the next interval's baseline to current tasks.
    pub fn get_max_concurrency(&self) -> u64 {
        let mut state = self.state.lock().unwrap();
        let peak = state.max_concurrency;
        state.max_concurrency = state.current;
        peak
    }
}
