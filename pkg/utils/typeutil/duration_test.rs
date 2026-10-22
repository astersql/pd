// Copyright 2026 AsterSQL.

use std::{str::FromStr, time::Duration as StdDuration};

use super::*;

#[test]
fn duration_serde_and_saturation() {
    let duration = Duration::from_str("1h1m1s").unwrap();
    assert_eq!(duration.as_nanos(), 3_661_000_000_000);
    assert_eq!(duration.to_string(), "1h1m1s");
    assert_eq!(serde_json::to_string(&duration).unwrap(), r#""1h1m1s""#);
    assert_eq!(
        serde_json::from_str::<Duration>(r#""1h1m1s""#).unwrap(),
        duration
    );

    let negative = serde_json::from_str::<Duration>(r#""-1.5s""#).unwrap();
    assert_eq!(negative.as_nanos(), -1_500_000_000);
    assert_eq!(negative.to_string(), "-1.5s");

    assert_eq!(saturating_std_duration_from_seconds(-2), StdDuration::ZERO);
    assert_eq!(
        saturating_std_duration_from_seconds(2),
        StdDuration::from_secs(2)
    );
    assert_eq!(
        saturating_std_duration_from_seconds(9_223_372_036),
        StdDuration::from_secs(9_223_372_036)
    );
    assert_eq!(
        saturating_std_duration_from_seconds(9_223_372_037),
        StdDuration::from_nanos(i64::MAX as u64)
    );
    assert_eq!(
        saturating_std_duration_from_seconds(i64::MAX),
        StdDuration::from_nanos(i64::MAX as u64)
    );
}
