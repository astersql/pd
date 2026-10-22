// Copyright 2026 AsterSQL.
//! Token bucket with monotonic time and cancellation-safe reservations.
use astersql_pd_syncutil::asynchronous::Mutex;
use std::time::Duration;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

/// Go's rate.Inf sentinel (math.MaxFloat64).
pub const INF: f64 = f64::MAX;

/// A failed wait never consumes its reservation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WaitError {
    #[error("request exceeds limiter burst")]
    ExceedsBurst,
    #[error("context canceled")]
    Cancelled,
    #[error("wait would exceed context deadline")]
    Deadline,
}

struct State {
    limit: f64,
    burst: i64,
    tokens: f64,
    last: Instant,
    last_event: Instant,
}
impl State {
    fn advance(&self, now: Instant) -> f64 {
        let added = if self.limit > 0.0 {
            now.saturating_duration_since(self.last).as_secs_f64() * self.limit
        } else {
            0.0
        };
        (self.tokens + added).min(self.burst as f64)
    }
    fn delay(&self, tokens: f64) -> Duration {
        if tokens >= 0.0 {
            Duration::ZERO
        } else if self.limit <= 0.0 {
            Duration::from_nanos(i64::MAX as u64)
        } else {
            Duration::from_nanos(((-tokens / self.limit * 1e9).min(i64::MAX as f64)) as u64)
        }
    }
}

/// Token bucket serialized like Go's RateLimiter, including pending waits.
/// All operations are asynchronous so lock contention yields to the caller's
/// runtime. No private runtime is created. Dropping a wait refunds its tokens.
pub struct RateLimiter {
    state: Mutex<State>,
}
impl RateLimiter {
    /// Creates a bucket initially containing `burst` tokens.
    pub fn new(limit: f64, burst: i64) -> Self {
        let now = Instant::now();
        Self {
            state: Mutex::new(State {
                limit,
                burst,
                tokens: burst as f64,
                last: now,
                last_event: now,
            }),
        }
    }
    /// Reports availability without consuming tokens.
    pub async fn available(&self, n: i64) -> bool {
        let mut s = self.state.lock().await;
        if s.limit == INF {
            return true;
        }
        if n > s.burst {
            return false;
        }
        let now = Instant::now();
        let tokens = s.advance(now) - n as f64;
        let delay = s.delay(tokens);
        let at = now + delay;
        s.tokens = tokens;
        s.last = now;
        s.last_event = at;
        let reservation = Reservation {
            n,
            limit: s.limit,
            at,
            active: true,
            cancel_at: Some(now),
            state: s,
        };
        drop(reservation);
        delay.is_zero()
    }
    /// Consumes one token when immediately available.
    pub async fn allow(&self) -> bool {
        self.allow_n(1).await
    }
    /// Consumes `n` tokens only when no waiting is necessary.
    pub async fn allow_n(&self, n: i64) -> bool {
        let mut s = self.state.lock().await;
        if s.limit == INF {
            return true;
        }
        let now = Instant::now();
        let tokens = s.advance(now) - n as f64;
        if n > s.burst || !s.delay(tokens).is_zero() {
            return false;
        }
        s.tokens = tokens;
        s.last = now;
        s.last_event = now;
        true
    }
    /// Changes refill rate after accruing tokens at the previous rate.
    pub async fn set_limit(&self, limit: f64) {
        let mut s = self.state.lock().await;
        let now = Instant::now();
        s.tokens = s.advance(now);
        s.last = now;
        s.limit = limit;
    }
    /// Changes capacity without immediately filling additional space.
    pub async fn set_burst(&self, burst: i64) {
        let mut s = self.state.lock().await;
        let now = Instant::now();
        s.tokens = s.advance(now);
        s.last = now;
        s.burst = burst;
    }
    /// Returns the refill rate.
    pub async fn limit(&self) -> f64 {
        self.state.lock().await.limit
    }
    /// Returns the maximum request size for finite rates.
    pub async fn burst(&self) -> i64 {
        self.state.lock().await.burst
    }
    /// Waits for tokens, rejecting cancellation, oversized requests and waits
    /// exceeding `deadline`. The deadline uses the same monotonic clock.
    pub async fn wait_n(
        &self,
        n: i64,
        cancel: &CancellationToken,
        deadline: Option<Instant>,
    ) -> Result<(), WaitError> {
        let reservation = {
            let mut s = self.state.lock().await;
            if n > s.burst && s.limit != INF {
                return Err(WaitError::ExceedsBurst);
            }
            if cancel.is_cancelled() {
                return Err(WaitError::Cancelled);
            }
            let now = Instant::now();
            let tokens = s.advance(now) - n as f64;
            let delay = if s.limit == INF {
                Duration::ZERO
            } else {
                s.delay(tokens)
            };
            if deadline.is_some_and(|d| d < now || delay > d.saturating_duration_since(now)) {
                return Err(WaitError::Deadline);
            }
            if s.limit == INF {
                return Ok(());
            }
            let at = now + delay;
            s.tokens = tokens;
            s.last = now;
            s.last_event = at;
            Reservation {
                n,
                limit: s.limit,
                at,
                active: !delay.is_zero(),
                cancel_at: None,
                state: s,
            }
        };
        if !reservation.active {
            return Ok(());
        }
        let mut reservation = reservation;
        tokio::select! {
            _ = tokio::time::sleep_until(reservation.at) => { reservation.active = false; Ok(()) }
            _ = cancel.cancelled() => Err(WaitError::Cancelled),
        }
    }
}

struct Reservation<'a> {
    state: tokio::sync::MutexGuard<'a, State>,
    n: i64,
    limit: f64,
    at: Instant,
    active: bool,
    cancel_at: Option<Instant>,
}
impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let s = &mut self.state;
        let now = self.cancel_at.unwrap_or_else(Instant::now);
        if s.limit == INF || self.n == 0 || self.at < now {
            return;
        }
        let elapsed = if s.last_event >= self.at {
            s.last_event.duration_since(self.at).as_secs_f64()
        } else {
            -self.at.duration_since(s.last_event).as_secs_f64()
        };
        let restore = self.n as f64
            - if self.limit > 0.0 {
                elapsed * self.limit
            } else {
                0.0
            };
        if restore <= 0.0 {
            return;
        }
        s.tokens = (s.advance(now) + restore).min(s.burst as f64);
        s.last = now;
        if s.last_event == self.at && self.limit > 0.0 && self.n >= 0 {
            let duration = Duration::from_nanos(
                (self.n as f64 / self.limit * 1e9).min(i64::MAX as f64) as u64,
            );
            if let Some(previous) = self.at.checked_sub(duration) {
                if previous >= now {
                    s.last_event = previous;
                }
            }
        }
    }
}
