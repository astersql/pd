// Copyright 2026 AsterSQL.

use std::time::{Duration as StdDuration, SystemTime, UNIX_EPOCH};

use crate::{BytesToU64Error, Duration, bytes_to_u64};

/// Parses exactly eight big-endian bytes as Unix nanoseconds.
///
/// The unsigned wire value is reinterpreted as signed, matching Go's
/// `time.Unix(0, int64(nano))`. Invalid input returns the byte decoder's error.
pub fn parse_timestamp(data: &[u8]) -> Result<SystemTime, BytesToU64Error> {
    let nanos = bytes_to_u64(data)? as i64;
    let delta = StdDuration::from_nanos(nanos.unsigned_abs());
    Ok(if nanos >= 0 {
        UNIX_EPOCH + delta
    } else {
        UNIX_EPOCH - delta
    })
}

// SystemTime contains only wall-clock time, so no monotonic component participates.
// Narrowing to i64 preserves Go UnixNano's wrapping representation.
fn unix_nanos(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(delta) => delta.as_nanos() as i64,
        Err(error) => (-(error.duration().as_nanos() as i128)) as i64,
    }
}

/// Returns a signed wall-clock duration with Go's wrapping nanosecond arithmetic.
pub fn sub_real_time_by_wall_clock(after: SystemTime, before: SystemTime) -> Duration {
    Duration::from_nanos(unix_nanos(after).wrapping_sub(unix_nanos(before)))
}

/// Subtracts TSO physical timestamps after separately truncating to milliseconds.
///
/// Division truncates toward zero, including before the Unix epoch. This can
/// differ from truncating the elapsed duration to milliseconds.
pub fn sub_tso_physical_by_wall_clock(after: SystemTime, before: SystemTime) -> i64 {
    unix_nanos(after) / 1_000_000 - unix_nanos(before) / 1_000_000
}
