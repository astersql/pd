// Copyright 2026 AsterSQL.
//! Store time decisions with explicit Unix nanosecond clocks. Durations retain
//! Go's signed int64 nanosecond range and saturating time.Time.Sub semantics.

use crate::StoreInfo;

const SECOND: i64 = 1_000_000_000;
// Go time.Time's zero value is year 1, distinct from Unix epoch metadata.
const ZERO_TIME_UNIX_NANOS: i128 = -62_135_596_800_000_000_000;

fn elapsed_nanos(later: i128, earlier: i128) -> i64 {
    later
        .saturating_sub(earlier)
        .clamp(i64::MIN as i128, i64::MAX as i128) as i64
}

impl StoreInfo {
    pub fn last_heartbeat_unix_nanos(&self) -> i64 {
        self.meta.as_ref().map_or(0, |meta| meta.last_heartbeat)
    }

    /// Replace heartbeat metadata, detaching metadata shared by shallow clones.
    /// Like the Go setter, this requires store metadata to be present.
    pub fn set_last_heartbeat_unix_nanos(&mut self, timestamp: i64) {
        std::sync::Arc::make_mut(self.meta.as_mut().expect("store metadata must be present"))
            .last_heartbeat = timestamp;
    }

    /// Set the last persisted time; signed i128 permits Go's year-1 zero time.
    pub fn set_last_persist_unix_nanos(&mut self, timestamp: i128) {
        self.last_persist_unix_nanos = Some(timestamp);
    }

    /// Set the last awaken time; signed i128 permits Go's year-1 zero time.
    pub fn set_last_awaken_unix_nanos(&mut self, timestamp: i128) {
        self.last_awaken_unix_nanos = Some(timestamp);
    }

    /// Signed elapsed time; a future heartbeat yields a negative duration.
    pub fn down_time_nanos(&self, now_unix_nanos: i128) -> i64 {
        elapsed_nanos(now_unix_nanos, self.last_heartbeat_unix_nanos() as i128)
    }

    /// Persist only when heartbeat exceeds the last persisted time by 5 minutes.
    pub fn need_persist(&self) -> bool {
        elapsed_nanos(
            self.last_heartbeat_unix_nanos() as i128,
            self.last_persist_unix_nanos.unwrap_or(ZERO_TIME_UNIX_NANOS),
        ) > 300 * SECOND
    }

    pub fn is_disconnected(&self, now_unix_nanos: i128) -> bool {
        self.down_time_nanos(now_unix_nanos) > 20 * SECOND
    }

    pub fn is_unhealthy(&self, now_unix_nanos: i128) -> bool {
        self.down_time_nanos(now_unix_nanos) > 600 * SECOND
    }

    /// Heartbeat minus start time, clamped to zero; independent of current time.
    pub fn uptime_nanos(&self) -> i64 {
        elapsed_nanos(
            self.last_heartbeat_unix_nanos() as i128,
            self.start_time_unix_seconds() as i128 * SECOND as i128,
        )
        .max(0)
    }

    /// Awaken only when heartbeat exceeds the last awaken time by 10 minutes.
    pub fn need_awaken_store(&self) -> bool {
        elapsed_nanos(
            self.last_heartbeat_unix_nanos() as i128,
            self.last_awaken_unix_nanos.unwrap_or(ZERO_TIME_UNIX_NANOS),
        ) > 600 * SECOND
    }
}
