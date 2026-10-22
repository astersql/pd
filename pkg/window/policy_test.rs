// Copyright 2026 AsterSQL.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use super::{RollingPolicy, Window};

#[test]
fn rolling_policy_rotation() {
    let start = Instant::now();
    let now = Arc::new(Mutex::new(start));
    let clock = Arc::clone(&now);
    let policy = RollingPolicy::with_clock(Window::new(3), Duration::from_millis(100), move || {
        *clock.lock().unwrap()
    });
    let check = |offset, expected: &[(&[f64], i64)]| {
        let state = policy.state.read().unwrap();
        assert_eq!(state.offset, offset);
        for (i, (points, count)) in expected.iter().enumerate() {
            assert_eq!(state.window.bucket(i).points, *points, "bucket {i}");
            assert_eq!(state.window.bucket(i).count, *count, "bucket {i}");
        }
    };
    policy.add(1.0);
    *now.lock().unwrap() = start + Duration::from_millis(99);
    policy.add(2.0);
    check(0, &[(&[3.0], 2), (&[], 0), (&[], 0)]);
    *now.lock().unwrap() = start + Duration::from_millis(150);
    policy.add(4.0);
    check(1, &[(&[3.0], 2), (&[4.0], 1), (&[], 0)]);
    *now.lock().unwrap() = start + Duration::from_millis(200);
    policy.add(5.0); // Preserve the 50 ms remainder rather than restarting at 150.
    check(2, &[(&[3.0], 2), (&[4.0], 1), (&[5.0], 1)]);
    *now.lock().unwrap() = start + Duration::from_millis(400);
    policy.add(6.0); // Less than a window, crossing offset zero.
    check(1, &[(&[], 0), (&[6.0], 1), (&[5.0], 1)]);
    *now.lock().unwrap() = start + Duration::from_millis(700);
    policy.add(7.0); // Exactly a window: clear all, retain offset.
    check(1, &[(&[], 0), (&[7.0], 1), (&[], 0)]);
    *now.lock().unwrap() = start + Duration::from_millis(1250);
    policy.add(8.0); // Five buckets: clear three, advance five.
    check(0, &[(&[8.0], 1), (&[], 0), (&[], 0)]);
    assert_eq!(
        policy.state.read().unwrap().last_append_time,
        start + Duration::from_millis(1200)
    );
    *now.lock().unwrap() = start + Duration::from_millis(1300);
    policy.add(9.0);
    check(1, &[(&[8.0], 1), (&[9.0], 1), (&[], 0)]);
    // Go truncates a negative fractional bucket toward zero; a full backward
    // bucket returns window size and resets all buckets.
    *now.lock().unwrap() = start + Duration::from_millis(1250);
    policy.add(1.0);
    check(1, &[(&[8.0], 1), (&[10.0], 2), (&[], 0)]);
    *now.lock().unwrap() = start + Duration::from_millis(1200);
    policy.add(2.0);
    check(1, &[(&[], 0), (&[2.0], 1), (&[], 0)]);
}
