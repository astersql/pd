// Copyright 2026 AsterSQL.

#![forbid(unsafe_code)]

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

/// Creates timers driven by the caller's Tokio runtime.
///
/// The pool does not create or own a runtime. Timer recycling is added
/// separately; each timer checked out by [`TimerPool::get`] is independent.
#[derive(Debug, Default)]
pub struct TimerPool;

impl TimerPool {
    /// Creates an empty timer pool.
    pub const fn new() -> Self {
        Self
    }

    /// Returns a timer that becomes ready after `duration`.
    pub fn get(&self, duration: Duration) -> Timer {
        Timer {
            sleep: Box::pin(tokio::time::sleep(duration)),
        }
    }
}

/// A checked-out timer that resolves once its configured duration expires.
pub struct Timer {
    sleep: Pin<Box<tokio::time::Sleep>>,
}

impl Future for Timer {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        self.sleep.as_mut().poll(context)
    }
}

#[cfg(test)]
mod pool_test;
