// Copyright 2026 AsterSQL.
use super::*;
#[test]
fn task_062_clock_projection() {
    let origin = Instant::now();
    let clock = ClockMap {
        monotonic: origin,
        wall: SystemTime::UNIX_EPOCH + Duration::from_nanos(123_456_700),
        zone: Some(chrono_tz::UTC),
    };
    assert_eq!(clock.render(None), "0001-01-01 00:00:00 +0000 UTC");
    assert_eq!(
        clock.render(Some(origin)),
        "1970-01-01 00:00:00.1234567 +0000 UTC m=+0.000000000"
    );
    assert_eq!(
        clock.render(Some(origin + Duration::from_secs(1))),
        "1970-01-01 00:00:01.1234567 +0000 UTC m=+1.000000000"
    );
    assert_eq!(
        clock.render(Some(origin - Duration::from_secs(1))),
        "1969-12-31 23:59:59.1234567 +0000 UTC m=-1.000000000"
    );
    let local = ClockMap {
        zone: Some(chrono_tz::Asia::Shanghai),
        ..clock.clone()
    };
    assert_eq!(
        local.render(Some(origin)),
        "1970-01-01 08:00:00.1234567 +0800 CST m=+0.000000000"
    );
    assert_eq!(clock.seconds(origin - Duration::from_secs(1)), -1);
    assert_eq!(
        clock.render(Some(origin)),
        "1970-01-01 00:00:00.1234567 +0000 UTC m=+0.000000000"
    );
}
