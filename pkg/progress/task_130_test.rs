// Copyright 2026 AsterSQL.
use super::*;
use std::time::{Duration, SystemTime};

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= expected.abs().max(1.0) * 1e-12,
        "{actual} != {expected}"
    );
}

#[test]
fn task_130_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_130.json"
    ))
    .unwrap();
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(7);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 11);
    for c in cases {
        let mut p = ProgressIndicator::new(
            c["id"].as_str().unwrap(),
            c["current"].as_f64().unwrap(),
            c["total"].as_f64().unwrap(),
            Duration::from_secs_f64(c["interval_seconds"].as_f64().unwrap()),
        )
        .unwrap();
        assert_eq!(p.progress().current_speed, 0.0);
        assert_eq!(p.progress().left_second, f64::MAX);
        assert_eq!(p.complete_at(), None);
        let initial = c["current"].as_f64().unwrap() / c["total"].as_f64().unwrap();
        assert!(
            p.progress().progress_percent == initial
                || (initial.is_nan() && p.progress().progress_percent.is_nan())
        );
        for (sample, expected) in c["samples"]
            .as_array()
            .unwrap()
            .iter()
            .zip(c["expected"].as_array().unwrap())
        {
            p.push_at(sample.as_f64().unwrap(), now);
            let v = p.progress();
            close(v.progress_percent, expected[0].as_f64().unwrap());
            close(v.current_speed, expected[1].as_f64().unwrap());
            close(v.left_second, expected[2].as_f64().unwrap_or(f64::MAX));
            assert_eq!(v.action, c["id"].as_str().unwrap());
            assert_eq!(
                p.complete_at(),
                if sample.as_f64().unwrap() >= c["total"].as_f64().unwrap()
                    || c["total"].as_f64().unwrap() == 0.0
                {
                    Some(now)
                } else {
                    None
                }
            );
        }
    }
    println!("11 source-derived sequences, 19 sample assertions");
}

#[test]
fn task_130_window_capacity_front_and_dynamic_patrol() {
    let mut p = ProgressIndicator::new("removing", 0.0, 1000.0, Duration::from_secs(60)).unwrap();
    assert_eq!((p.window_capacity(), p.window_length()), (121, 10));
    for i in 1..=120 {
        p.adjust_window_length(Duration::from_secs(10));
        p.push(i as f64);
    }
    assert_eq!(
        (p.history_len(), p.current_window_length(), p.front()),
        (121, 11, 110.0)
    );
    close(p.progress().progress_percent, 0.12);
    close(p.progress().current_speed * 60.0, 1.0);
    close(p.progress().left_second / 60.0, 880.0);
    for _ in 0..5 {
        p.push(120.0);
    }
    close(p.progress().current_speed * 60.0, 0.5);
    close(p.progress().left_second / 60.0, 1760.0);
    p.adjust_window_length(Duration::from_secs(19 * 60));
    for _ in 0..4 {
        p.push(120.0);
    }
    assert_eq!(
        (p.window_length(), p.current_window_length(), p.front()),
        (20, 20, 110.0)
    );
    close(p.progress().current_speed * 60.0, 10.0 / 19.0);
    close(p.progress().left_second / 60.0, 1672.0);
    for i in 1..=10 {
        p.push(120.0 + i as f64);
        assert_eq!(p.front(), 110.0 + i as f64);
        close(p.progress().current_speed * 60.0, 10.0 / 19.0);
    }
    for (seconds, length) in [(60, 11), (10800, 121), (1800, 31), (600, 11), (7200, 121)] {
        let previous = p.current_window_length();
        p.adjust_window_length(Duration::from_secs(seconds));
        assert_eq!(p.current_window_length(), previous);
        assert_eq!(p.window_length(), length);
        p.push(130.0);
        assert_eq!(p.current_window_length(), length);
        assert_eq!(p.history_len(), 121);
    }
}

#[test]
fn task_130_completion_target_and_invalid_interval() {
    assert_eq!(
        ProgressIndicator::new("test", 0.0, 100.0, Duration::ZERO).unwrap_err(),
        InvalidUpdateInterval
    );
    let mut p = ProgressIndicator::new("preparing", 0.0, 100.0, Duration::from_secs(1)).unwrap();
    p.push_at(100.0, SystemTime::UNIX_EPOCH);
    p.raise_target(200.0);
    p.raise_target(50.0);
    p.push_at(150.0, SystemTime::UNIX_EPOCH + Duration::from_secs(1));
    assert_eq!(p.target_region_size(), 200.0);
    close(p.progress().progress_percent, 0.75);
    close(p.progress().current_speed, 75.0);
    close(p.progress().left_second, 2.0 / 3.0);
    assert_eq!(p.complete_at(), Some(SystemTime::UNIX_EPOCH));
    let later = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
    p.push_at(200.0, later);
    assert_eq!(p.complete_at(), Some(later));
    // Completed zero-speed progress retains max ETA in Go (0/0), not zero.
    let mut p = ProgressIndicator::new("test", 0.0, 100.0, Duration::from_secs(600)).unwrap();
    p.push_at(100.0, later);
    assert_eq!(p.progress().left_second, f64::MAX);
    assert_eq!(p.complete_at(), Some(later));
}
