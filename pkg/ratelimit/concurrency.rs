// Copyright 2026 AsterSQL.
use astersql_pd_syncutil::blocking::Mutex;
use std::collections::VecDeque;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

/// A token belongs to one limiter and one acquisition; clones share release state.
#[derive(Debug, Clone)]
pub struct TaskToken(Arc<TokenState>);

#[derive(Debug)]
struct TokenState {
    owner: Arc<()>,
    released: AtomicBool,
}

/// Acquisition was cancelled before admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("context canceled")]
pub struct AcquireCancelled;

struct Waiting<'a> {
    limiter: &'a ConcurrencyLimiter,
    notify: Arc<Notify>,
}

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        let mut state = self.limiter.state.lock().unwrap();
        state
            .waiters
            .retain(|waiter| !Arc::ptr_eq(waiter, &self.notify));
        if let Some(next) = state.waiters.front() {
            next.notify_one();
        }
    }
}

#[derive(Debug)]
struct State {
    current: u64,
    limit: u64,
    max_concurrency: u64,
    waiters: VecDeque<Arc<Notify>>,
}

/// Limits concurrent tasks using immediate admission decisions.
///
/// A zero limit permits unlimited admission. Lowering the limit leaves running
/// tasks intact; callers must pair each successful admission with a release.
#[derive(Debug)]
pub struct ConcurrencyLimiter {
    state: Mutex<State>,
    identity: Arc<()>,
}

impl ConcurrencyLimiter {
    /// Creates a limiter with no running tasks.
    pub fn new(limit: u64) -> Self {
        Self {
            state: Mutex::new(State {
                current: 0,
                limit,
                max_concurrency: 0,
                waiters: VecDeque::new(),
            }),
            identity: Arc::new(()),
        }
    }

    /// Acquires in FIFO order, or waits for cancellation. Unlike `allow`, zero
    /// blocks admission, matching Go's token interface. Dropping a pending future
    /// removes its queue entry. Acquired tokens require explicit release.
    pub async fn acquire_token(
        &self,
        cancel: &CancellationToken,
    ) -> Result<TaskToken, AcquireCancelled> {
        let waiting = {
            let mut state = self.state.lock().unwrap();
            if state.waiters.is_empty() && state.current < state.limit {
                return Ok(self.admit(&mut state));
            }
            let notify = Arc::new(Notify::new());
            state.waiters.push_back(notify.clone());
            Waiting {
                limiter: self,
                notify,
            }
        };
        loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(AcquireCancelled),
                _ = waiting.notify.notified() => {}
            }
            let mut state = self.state.lock().unwrap();
            if state.current < state.limit
                && state
                    .waiters
                    .front()
                    .is_some_and(|front| Arc::ptr_eq(front, &waiting.notify))
            {
                state.waiters.pop_front();
                let token = self.admit(&mut state);
                if state.current < state.limit {
                    if let Some(next) = state.waiters.front() {
                        next.notify_one();
                    }
                }
                drop(state);
                return Ok(token);
            }
        }
    }

    fn admit(&self, state: &mut State) -> TaskToken {
        state.current += 1;
        // Go's token interface does not update the legacy peak statistic.
        TaskToken(Arc::new(TokenState {
            owner: self.identity.clone(),
            released: AtomicBool::new(false),
        }))
    }

    /// Releases once; duplicate and foreign tokens have no effect.
    pub fn release_token(&self, token: &TaskToken) {
        let mut state = self.state.lock().unwrap();
        if !Arc::ptr_eq(&token.0.owner, &self.identity)
            || token.0.released.swap(true, Ordering::Relaxed)
        {
            return;
        }
        state.current = state.current.saturating_sub(1);
        if let Some(next) = state.waiters.front() {
            next.notify_one();
        }
    }

    /// Returns pending acquisitions, excluding cancelled and dropped futures.
    pub fn get_waiting_tasks_num(&self) -> u64 {
        self.state.lock().unwrap().waiters.len() as u64
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
