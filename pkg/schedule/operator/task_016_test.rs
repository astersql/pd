// Copyright 2026 AsterSQL.

use std::{
    sync::{Arc, Barrier, Mutex},
    time::{Duration, Instant},
};

use crate::*;

struct ManualClock(Mutex<Instant>);
impl ManualClock {
    fn advance(&self, nanos: u64) {
        *self.0.lock().unwrap() += Duration::from_nanos(nanos);
    }
}
impl Clock for ManualClock {
    fn now(&self) -> Instant {
        *self.0.lock().unwrap()
    }
}
fn tracker(status: u32) -> (OpStatusTracker, Arc<ManualClock>) {
    let clock = Arc::new(ManualClock(Mutex::new(Instant::now())));
    let tracker = OpStatusTracker::with_clock(clock.clone());
    // Creation and start are deliberately separated to detect using creation
    // time for a running timeout. No sleeping or private state mutation.
    clock.advance(100);
    if [1, 2, 4, 6].contains(&status) {
        assert!(tracker.to(STARTED));
    }
    if status > 1 {
        assert!(tracker.to(OpStatus(status)));
    }
    (tracker, clock)
}

#[test]
fn task_016_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_016.json"
    ))
    .unwrap();
    assert_eq!(fixture["evidence_kind"], "source_assertion");
    assert_eq!(fixture["go_execution"], false);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 44);
    for case in cases {
        let status = case["status"].as_u64().unwrap() as u32;
        let (tracker, clock) = tracker(status);
        let before = tracker.reach_time();
        let created = tracker.reach_time_of(CREATED);
        let started = tracker.reach_time_of(STARTED);
        // CREATED elapsed is measured from creation, others from their reach time.
        if status == 0 {
            *clock.0.lock().unwrap() = created.unwrap();
        }
        clock.advance(case["elapsed_ns"].as_u64().unwrap());
        let duration = Duration::from_nanos(case["duration_ns"].as_u64().unwrap());
        let check = || match case["method"].as_str().unwrap() {
            "expired" => tracker.check_expired(duration),
            "timeout" => tracker.check_timeout(duration),
            _ => unreachable!(),
        };
        assert_eq!(check(), case["result"].as_bool().unwrap(), "{case}");
        assert_eq!(
            tracker.status().0,
            case["after"].as_u64().unwrap() as u32,
            "{case}"
        );
        let reached = tracker.reach_time();
        assert_eq!(
            reached,
            if case["changed"].as_bool().unwrap() {
                clock.now()
            } else {
                before
            },
            "{case}"
        );
        assert_eq!(tracker.reach_time_of(CREATED), created);
        assert_eq!(tracker.reach_time_of(STARTED), started);
        // Repeating with a later clock must preserve the first terminal time.
        // Active cases below the threshold must remain unchanged at the same instant.
        assert_eq!(check(), case["result"].as_bool().unwrap());
        assert_eq!(tracker.reach_time(), reached);
        if tracker.is_end() {
            clock.advance(1000);
            assert_eq!(check(), case["result"].as_bool().unwrap());
            assert_eq!(tracker.reach_time(), reached);
            assert_eq!(tracker.reach_time_of(tracker.status()), Some(reached));
            for terminal in [SUCCESS, CANCELED, REPLACED, EXPIRED, TIMEOUT] {
                if terminal != tracker.status() {
                    assert_eq!(tracker.reach_time_of(terminal), None);
                }
            }
        }
    }
    println!("task_016: 44 source-derived timing cases passed");
}

#[test]
fn task_016_delayed_start_uses_started_time() {
    let (tracker, clock) = tracker(1);
    let created = tracker.reach_time_of(CREATED).unwrap();
    let started = tracker.reach_time_of(STARTED).unwrap();
    assert_eq!(started.duration_since(created), Duration::from_nanos(100));
    assert!(!tracker.check_expired(Duration::ZERO));
    assert!(!tracker.check_timeout(Duration::from_nanos(10)));
    clock.advance(9);
    assert!(!tracker.check_timeout(Duration::from_nanos(10)));
    clock.advance(1);
    assert!(tracker.check_timeout(Duration::from_nanos(10)));
    assert_eq!(tracker.status(), TIMEOUT);
    assert_eq!(tracker.reach_time(), clock.now());
}

#[test]
fn task_016_concurrent_deadline_and_terminal_transition() {
    // Race actual deadline checks against cancellation; terminal state/time must
    // be a single atomic decision and remain stable after all readers finish.
    for initial in [0, 1] {
        let (tracker, clock) = tracker(initial);
        let tracker = Arc::new(tracker);
        let barrier = Arc::new(Barrier::new(17));
        clock.advance(20);
        let results = std::thread::scope(|scope| {
            let joins: Vec<_> = (0..16)
                .map(|i| {
                    let tracker = tracker.clone();
                    let barrier = barrier.clone();
                    scope.spawn(move || {
                        barrier.wait();
                        if i % 2 == 0 {
                            tracker.to(CANCELED)
                        } else if initial == 0 {
                            tracker.check_expired(Duration::from_nanos(10))
                        } else {
                            tracker.check_timeout(Duration::from_nanos(10))
                        }
                    })
                })
                .collect();
            barrier.wait();
            joins
                .into_iter()
                .map(|j| j.join().unwrap())
                .collect::<Vec<_>>()
        });
        let deadline = if initial == 0 { EXPIRED } else { TIMEOUT };
        assert!([CANCELED, deadline].contains(&tracker.status()));
        let reached = tracker.reach_time();
        assert_eq!(reached, clock.now());
        if tracker.status() == CANCELED {
            assert_eq!(results.iter().filter(|r| **r).count(), 1);
        } else {
            assert_eq!(results.iter().filter(|r| **r).count(), 8);
        }
        clock.advance(1000);
        assert!(!tracker.to(STARTED));
        assert_eq!(
            tracker.check_expired(Duration::ZERO),
            tracker.status() == EXPIRED
        );
        assert_eq!(
            tracker.check_timeout(Duration::ZERO),
            tracker.status() == TIMEOUT
        );
        assert_eq!(tracker.reach_time(), reached);
    }
}
