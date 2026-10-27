// Copyright 2026 AsterSQL.
use super::*;
use std::time::{Duration, SystemTime};

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_131.json"
    ))
    .unwrap()
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= expected.abs().max(1.0) * 1e-12,
        "{actual} != {expected}"
    );
}

#[test]
fn task_131_behavior_cases() {
    let f = fixture();
    let rows = f["boundaries"].as_array().unwrap();
    assert_eq!(rows.len(), 12);
    for row in rows {
        let mut p = ProgressIndicator::new(
            "removing",
            0.0,
            100.0,
            Duration::from_secs_f64(row["interval_seconds"].as_f64().unwrap()),
        )
        .unwrap();
        let before = p.progress().clone();
        p.adjust_window_length(Duration::from_secs(row["patrol_seconds"].as_u64().unwrap()));
        assert_eq!(
            p.window_length(),
            row["length"].as_u64().unwrap() as usize,
            "{row}"
        );
        assert_eq!(
            p.window_capacity(),
            row["capacity"].as_u64().unwrap() as usize
        );
        assert_eq!(p.current_window_length(), 1);
        assert_eq!(p.history_len(), 1);
        assert_eq!(p.front(), 0.0);
        assert_eq!(p.progress(), &before);
        p.push_at(10.0, SystemTime::UNIX_EPOCH);
        assert_eq!(p.current_window_length(), 2);
        close(
            p.progress().current_speed,
            10.0 / row["interval_seconds"].as_f64().unwrap().min(600.0),
        );
        close(p.progress().progress_percent, 0.1);
        assert_eq!(p.complete_at(), None);
    }
    println!("12 frozen source-derived duration/interval boundary cases passed");
}

fn checkpoint(p: &ProgressIndicator, expected: &serde_json::Value) {
    assert_eq!(
        p.current_window_length(),
        expected[0].as_u64().unwrap() as usize
    );
    assert_eq!(p.window_length(), p.current_window_length());
    assert_eq!(p.history_len(), 121);
    close(p.front(), expected[1].as_f64().unwrap());
    close(p.progress().progress_percent, expected[2].as_f64().unwrap());
    close(
        p.progress().current_speed * 60.0,
        expected[3].as_f64().unwrap(),
    );
    close(
        p.progress().left_second / 60.0,
        expected[4].as_f64().unwrap(),
    );
    assert_eq!(p.progress().action, "removing");
    assert_eq!(p.complete_at(), None);
}

#[test]
fn task_131_dynamic_continuity_reuses_retained_history() {
    let f = fixture();
    let c = &f["dynamic"]["checkpoints"];
    let mut p = ProgressIndicator::new("removing", 0.0, 1000.0, Duration::from_secs(60)).unwrap();
    for i in 1..=120 {
        p.adjust_window_length(Duration::from_secs(10));
        p.push(i as f64);
    }
    checkpoint(&p, &c[0]);
    for _ in 0..5 {
        p.push(120.0);
    }
    checkpoint(&p, &c[1]);
    let before = p.progress().clone();
    let front = p.front();
    p.adjust_window_length(Duration::from_secs(1140));
    assert_eq!(p.progress(), &before);
    assert_eq!(p.front(), front);
    assert_eq!(p.current_window_length(), 11);
    for _ in 0..4 {
        p.push(120.0);
    }
    checkpoint(&p, &c[2]);
    for i in 1..=10 {
        p.push(120.0 + i as f64);
        close(p.front(), 110.0 + i as f64);
        close(p.progress().current_speed * 60.0, 10.0 / 19.0);
    }
    checkpoint(&p, &c[3]);
    p.adjust_window_length(Duration::from_secs(600));
    p.push(131.0);
    checkpoint(&p, &c[4]);
    p.adjust_window_length(Duration::MAX);
    p.push(132.0);
    checkpoint(&p, &c[5]);
    println!("6 dynamic checkpoints and 10 recovery steps passed");
}

#[test]
fn task_131_insufficient_samples_and_completion() {
    let mut p = ProgressIndicator::new("removing", 0.0, 100.0, Duration::from_secs(600)).unwrap();
    p.push_at(50.0, SystemTime::UNIX_EPOCH);
    // Constructor window contains one sample at the clamped interval.
    assert_eq!(p.progress().current_speed, 0.0);
    assert_eq!(p.progress().left_second, f64::MAX);
    p.adjust_window_length(Duration::from_secs(7200));
    p.push_at(100.0, SystemTime::UNIX_EPOCH + Duration::from_secs(1));
    assert_eq!(
        (
            p.window_length(),
            p.current_window_length(),
            p.history_len()
        ),
        (13, 3, 3)
    );
    close(p.progress().current_speed, 100.0 / 1200.0);
    assert_eq!(p.progress().progress_percent, 1.0);
    assert_eq!(p.progress().left_second, 0.0);
    let completed = p.complete_at();
    let before = p.progress().clone();
    p.adjust_window_length(Duration::ZERO);
    assert_eq!(p.complete_at(), completed);
    assert_eq!(p.progress(), &before);
    p.push_at(100.0, SystemTime::UNIX_EPOCH + Duration::from_secs(2));
    assert_eq!(p.current_window_length(), 2);
    assert_eq!(p.progress().current_speed, 0.0);
    assert_eq!(p.progress().left_second, f64::MAX); // Go's completed 0/0 ETA.
    assert_eq!(
        p.complete_at(),
        Some(SystemTime::UNIX_EPOCH + Duration::from_secs(2))
    );
    p.push_at(90.0, SystemTime::UNIX_EPOCH + Duration::from_secs(3));
    assert_eq!(p.progress().progress_percent, 0.9);
    assert_eq!(p.progress().current_speed, 0.0);
    assert_eq!(p.progress().left_second, f64::MAX);
    assert_eq!(
        p.complete_at(),
        Some(SystemTime::UNIX_EPOCH + Duration::from_secs(2))
    );
}

#[test]
fn task_131_invalid_interval_and_zero_target() {
    assert_eq!(
        ProgressIndicator::new("removing", 0.0, 100.0, Duration::ZERO).unwrap_err(),
        InvalidUpdateInterval
    );
    let mut p = ProgressIndicator::new("removing", 0.0, 0.0, Duration::from_secs(60)).unwrap();
    p.adjust_window_length(Duration::MAX);
    p.push_at(0.0, SystemTime::UNIX_EPOCH);
    assert_eq!(p.window_length(), 121);
    assert_eq!(p.current_window_length(), 2);
    assert_eq!(p.progress().progress_percent, 1.0);
    assert_eq!(p.progress().current_speed, 0.0);
    assert_eq!(p.progress().left_second, 0.0);
    assert_eq!(p.complete_at(), Some(SystemTime::UNIX_EPOCH));
}
