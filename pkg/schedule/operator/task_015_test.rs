// Copyright 2026 AsterSQL.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde_json::Value;

use crate::*;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_015.json"
    ))
    .unwrap()
}

struct ManualClock(Mutex<Instant>);
impl ManualClock {
    fn advance(&self, duration: Duration) {
        *self.0.lock().unwrap() += duration;
    }
}
impl Clock for ManualClock {
    fn now(&self) -> Instant {
        *self.0.lock().unwrap()
    }
}
fn tracker() -> (OpStatusTracker, Arc<ManualClock>) {
    let clock = Arc::new(ManualClock(Mutex::new(Instant::now())));
    (OpStatusTracker::with_clock(clock.clone()), clock)
}
fn at_status(status: u32) -> (OpStatusTracker, Arc<ManualClock>) {
    let (tracker, clock) = tracker();
    if [2, 4, 6].contains(&status) {
        assert!(tracker.to(STARTED));
    }
    if status != 0 {
        assert!(tracker.to(OpStatus(status)));
    }
    (tracker, clock)
}

#[test]
fn task_015_behavior_cases() {
    let data = fixture();
    assert_eq!(data["evidence_kind"], "source_assertion");
    assert_eq!(data["go_execution"], false);
    for case in data["kinds"].as_array().unwrap() {
        let actual = case["input"].as_str().unwrap().parse::<OpKind>();
        if let Some(error) = case["error"].as_str() {
            assert_eq!(actual.unwrap_err().to_string(), error);
        } else {
            assert_eq!(actual.unwrap().0, case["bits"].as_u64().unwrap() as u32);
        }
    }
    for case in data["kind_strings"].as_array().unwrap() {
        assert_eq!(
            OpKind(case["bits"].as_u64().unwrap() as u32).to_string(),
            case["output"].as_str().unwrap()
        );
    }
    let mut kind = OpKind::ADMIN;
    kind |= OpKind::LEADER;
    assert_eq!(kind.0, 257);
    assert_eq!(OpKind::REPLICA | OpKind::LEADER, OpKind(272));
    for case in data["statuses"].as_array().unwrap() {
        let status = OpStatus(case["input"].as_u64().unwrap() as u32);
        assert_eq!(status.is_end(), case["end"].as_bool().unwrap());
        assert_eq!(status.to_string(), case["name"].as_str().unwrap());
    }
    for case in data["transitions"].as_array().unwrap() {
        let from = case["from"].as_u64().unwrap() as u32;
        let dst = OpStatus(case["to"].as_u64().unwrap() as u32);
        let (tracker, clock) = at_status(from);
        let before = tracker.reach_time();
        let created = tracker.reach_time_of(CREATED);
        clock.advance(Duration::from_secs(1));
        let changed = tracker.to(dst);
        assert_eq!(
            changed,
            case["changed"].as_bool().unwrap(),
            "{from} -> {}",
            dst.0
        );
        assert_eq!(tracker.status().0, case["after"].as_u64().unwrap() as u32);
        assert_eq!(tracker.to_string(), tracker.status().to_string());
        assert_eq!(
            tracker.reach_time(),
            if changed { clock.now() } else { before }
        );
        assert_eq!(tracker.reach_time_of(CREATED), created);
        for raw in [0, 1, 2, 3, 4, 5, 6, 7, 8, u32::MAX] {
            let time = tracker.reach_time_of(OpStatus(raw));
            let reached = raw == 0
                || (raw == 1
                    && (from == 1 || [2, 4, 6].contains(&from) || (changed && dst == STARTED)))
                || (raw >= 2 && OpStatus(raw) == tracker.status());
            assert_eq!(
                time.is_some(),
                reached,
                "from={from} to={} queried={raw}",
                dst.0
            );
        }
    }
    println!("task_015: 20 parse + 15 display + 10 status + 70 transition source-derived cases");
}

#[test]
fn task_015_expiration_timeout_and_times() {
    let (tracker, clock) = tracker();
    assert_eq!(tracker.status(), CREATED);
    assert_eq!(tracker.reach_time(), clock.now());
    assert!(!tracker.check_timeout(Duration::ZERO));
    let duration = Duration::from_secs(10);
    clock.advance(duration - Duration::from_nanos(1));
    assert!(!tracker.check_expired(duration));
    clock.advance(Duration::from_nanos(1));
    assert!(tracker.check_expired(duration));
    let reached = tracker.reach_time();
    clock.advance(duration);
    assert!(tracker.check_expired(duration));
    assert!(!tracker.check_timeout(Duration::ZERO));
    assert_eq!(tracker.reach_time(), reached);
    for terminal in 2..7 {
        let (tracker, clock) = at_status(terminal);
        let reached = tracker.reach_time();
        clock.advance(duration);
        assert_eq!(tracker.check_expired(Duration::ZERO), terminal == 5);
        assert_eq!(tracker.check_timeout(Duration::ZERO), terminal == 6);
        assert_eq!(tracker.reach_time(), reached);
    }
    let (tracker, clock) = self::tracker();
    assert!(tracker.to(STARTED));
    assert!(!tracker.check_expired(Duration::ZERO));
    clock.advance(duration - Duration::from_nanos(1));
    assert!(!tracker.check_timeout(duration));
    clock.advance(Duration::from_nanos(1));
    assert!(tracker.check_timeout(duration));
    assert_eq!(tracker.status(), TIMEOUT);
    assert_eq!(tracker.reach_time(), clock.now());
    assert!(tracker.reach_time_of(SUCCESS).is_none());
    let (tracker, _) = self::tracker();
    assert!(tracker.check_expired(Duration::ZERO));
    let before = Instant::now();
    let tracker = OpStatusTracker::default();
    assert!(tracker.reach_time() >= before);
    assert!(tracker.reach_time() <= Instant::now());
}

#[test]
fn task_015_additional_merge_information() {
    let info = Arc::new(AdditionalInfo::default());
    assert_eq!(info.log(), "");
    assert!(!info.has_related_merge_region());
    assert_eq!(info.related_merge_region(), 0);
    assert_eq!(info.get("missing"), None);
    for case in fixture()["merge"].as_array().unwrap() {
        info.set(RELATED_MERGE_REGION, case["input"].as_str().unwrap());
        assert_eq!(
            info.has_related_merge_region(),
            case["has"].as_bool().unwrap()
        );
        assert_eq!(info.related_merge_region(), case["id"].as_u64().unwrap());
    }
    assert_eq!(info.log(), fixture()["merge_log"].as_str().unwrap());
    info.set("escape", "<>&\u{2028}\u{2029}");
    assert_eq!(info.log(), fixture()["escaped_log"].as_str().unwrap());
    std::thread::scope(|scope| {
        for i in 0..32 {
            let info = info.clone();
            scope.spawn(move || {
                info.set(format!("key{i}"), format!("value{i}"));
                assert_eq!(info.get(&format!("key{i}")), Some(format!("value{i}")));
            });
        }
    });
    let logged: Value = serde_json::from_str(&info.log()).unwrap();
    assert_eq!(logged.as_object().unwrap().len(), 34);
}

#[test]
fn task_015_concurrent_terminal_transition() {
    let (tracker, _) = tracker();
    assert!(tracker.to(STARTED));
    let tracker = Arc::new(tracker);
    let winners = std::thread::scope(|scope| {
        let joins: Vec<_> = [SUCCESS, CANCELED, REPLACED, TIMEOUT]
            .into_iter()
            .map(|dst| {
                let tracker = tracker.clone();
                scope.spawn(move || tracker.to(dst))
            })
            .collect();
        joins
            .into_iter()
            .filter_map(|j| j.join().unwrap().then_some(()))
            .count()
    });
    assert_eq!(winners, 1);
    assert!(tracker.is_end());
    assert_eq!(
        tracker.reach_time_of(tracker.status()),
        Some(tracker.reach_time())
    );
    assert!(!tracker.to(STARTED));
}
