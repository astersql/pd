// Copyright 2026 AsterSQL.

#![forbid(unsafe_code)]

use std::{
    future::Future,
    pin::Pin,
    sync::{Mutex, MutexGuard},
    task::{Context, Poll},
    time::Duration,
};

/// Creates timers driven by the caller's Tokio runtime.
///
/// The pool does not create or own a runtime. Returning a timer cancels its
/// Tokio sleep before retaining the timer storage for reuse.
#[derive(Debug, Default)]
pub struct TimerPool {
    timers: Mutex<Vec<Box<TimerState>>>,
}

impl TimerPool {
    /// Creates an empty timer pool.
    pub const fn new() -> Self {
        Self {
            timers: Mutex::new(Vec::new()),
        }
    }

    /// Returns a timer that becomes ready after `duration`.
    pub fn get(&self, duration: Duration) -> Timer {
        let mut state = self.lock_timers().pop().unwrap_or_default();
        state.sleep = Some(Box::pin(tokio::time::sleep(duration)));
        Timer { state: Some(state) }
    }

    /// Cancels `timer` and returns its storage to the pool.
    pub fn put(&self, mut timer: Timer) {
        let mut state = timer
            .state
            .take()
            .expect("a checked-out timer always owns its state");
        state.sleep = None;
        self.lock_timers().push(state);
    }

    fn lock_timers(&self) -> MutexGuard<'_, Vec<Box<TimerState>>> {
        self.timers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[cfg(test)]
    fn pooled_timer_count(&self) -> usize {
        self.lock_timers().len()
    }
}

/// A checked-out timer that resolves once its configured duration expires.
pub struct Timer {
    state: Option<Box<TimerState>>,
}

#[derive(Debug, Default)]
struct TimerState {
    sleep: Option<Pin<Box<tokio::time::Sleep>>>,
}

impl Timer {
    #[cfg(test)]
    fn storage_id(&self) -> *const TimerState {
        self.state
            .as_deref()
            .expect("a checked-out timer always owns its state")
    }
}

impl Future for Timer {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        self.state
            .as_mut()
            .expect("a checked-out timer always owns its state")
            .sleep
            .as_mut()
            .expect("a checked-out timer always has an active sleep")
            .as_mut()
            .poll(context)
    }
}

#[cfg(test)]
mod pool_test;
