// Copyright 2026 AsterSQL.

use std::time::{Duration as StdDuration, SystemTime, UNIX_EPOCH};

use crate::{parse_timestamp, sub_real_time_by_wall_clock, sub_tso_physical_by_wall_clock};

fn wall(nanos: i64) -> SystemTime {
    if nanos >= 0 {
        UNIX_EPOCH + StdDuration::from_nanos(nanos as u64)
    } else {
        UNIX_EPOCH - StdDuration::from_nanos(nanos.unsigned_abs())
    }
}

#[test]
fn timestamp_and_wall_clock_parse() {
    for nanos in [0, 1, -1, 1_619_398_665_682_000_000, i64::MIN, i64::MAX] {
        assert_eq!(
            parse_timestamp(&(nanos as u64).to_be_bytes()).unwrap(),
            wall(nanos)
        );
    }
    for len in [0, 2, 7, 9, 16] {
        assert_eq!(
            parse_timestamp(&vec![0; len]).unwrap_err().actual_len(),
            len
        );
    }
}

#[test]
fn timestamp_and_wall_clock_differences() {
    // Go TestSmallTimeDifference: truncate each physical timestamp before subtraction.
    let before = wall(1_619_398_665_681_918_000);
    let after = wall(1_619_398_665_682_000_000);
    assert_eq!(
        sub_real_time_by_wall_clock(after, before).as_nanos(),
        82_000
    );
    assert_eq!(
        sub_real_time_by_wall_clock(before, after).as_nanos(),
        -82_000
    );
    assert_eq!(sub_tso_physical_by_wall_clock(after, before), 1);
    assert_eq!(sub_tso_physical_by_wall_clock(before, after), -1);
    for (after, before, nanos, millis) in [
        (999_999, 0, 999_999, 0),
        (1_000_000, 999_999, 1, 1),
        (0, -999_999, 999_999, 0),
        (-999_999, -1_000_000, 1, 1),
        (1_000_000, -1_000_000, 2_000_000, 2),
        (i64::MAX, i64::MIN, -1, 18_446_744_073_708),
    ] {
        assert_eq!(
            sub_real_time_by_wall_clock(wall(after), wall(before)).as_nanos(),
            nanos
        );
        assert_eq!(
            sub_tso_physical_by_wall_clock(wall(after), wall(before)),
            millis
        );
    }
    // UnixNano wraps even for wall-clock values outside the signed nanosecond range.
    let outside = wall(i64::MAX) + StdDuration::from_nanos(1);
    assert_eq!(
        sub_real_time_by_wall_clock(outside, UNIX_EPOCH).as_nanos(),
        i64::MIN
    );
    assert_eq!(
        sub_tso_physical_by_wall_clock(outside, UNIX_EPOCH),
        i64::MIN / 1_000_000
    );
}

#[test]
fn time_optional_json() {
    use crate::TimeOptional;

    assert_eq!(
        serde_json::to_string(&TimeOptional::default()).unwrap(),
        "0"
    );
    for seconds in [i64::MIN, -1, 0, 1, 1_729_000_123, i64::MAX - 1, i64::MAX] {
        let json = seconds.to_string();
        assert_eq!(
            serde_json::to_string(&TimeOptional(Some(seconds))).unwrap(),
            json
        );
        let decoded: TimeOptional = serde_json::from_str(&json).unwrap();
        assert_eq!(
            decoded.0,
            (seconds > 0 && seconds < i64::MAX).then_some(seconds)
        );
    }
    assert_eq!(
        serde_json::from_str::<TimeOptional>("null").unwrap(),
        TimeOptional::default()
    );
    for json in [
        "1.0",
        "1e0",
        "\"1\"",
        "true",
        "[]",
        "{}",
        "9223372036854775808",
        "-9223372036854775809",
        "",
        "1 2",
    ] {
        assert!(
            serde_json::from_str::<TimeOptional>(json).is_err(),
            "{json}"
        );
    }
    let mut populated = TimeOptional(Some(42));
    let mut deserializer = serde_json::Deserializer::from_str("\"invalid\"");
    assert!(serde::Deserialize::deserialize_in_place(&mut deserializer, &mut populated).is_err());
    assert_eq!(populated, TimeOptional(Some(42)));
}
