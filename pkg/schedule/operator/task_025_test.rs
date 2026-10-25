// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{Peer, RegionInfo, RegionMeta};
use astersql_pd_core_constant::ResourceKind;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
struct Manual(Mutex<Instant>);
impl Clock for Manual {
    fn now(&self) -> Instant {
        *self.0.lock().unwrap()
    }
}
fn op(kind: OpKind, steps: Vec<OpStep>, c: Arc<dyn Clock>) -> Operator {
    Operator::with_clock("test", "test", 1, None, kind, 1, steps, c)
}
fn leader() -> OpStep {
    TransferLeader {
        from_store: 1,
        to_store: 2,
        ..Default::default()
    }
    .into()
}
fn clock() -> Arc<Manual> {
    Arc::new(Manual(Mutex::new(Instant::now())))
}
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_025.json"
    ))
    .unwrap()
}
#[test]
fn task_025_behavior_cases() {
    for v in fixture()["scheduler_cases"].as_array().unwrap() {
        let o = op(
            OpKind(v[0].as_u64().unwrap() as u32),
            vec![leader()],
            clock(),
        );
        assert_eq!(o.scheduler_kind().0, v[1].as_u64().unwrap() as u32);
    }
    let c = clock();
    let o = op(OpKind::LEADER, vec![leader()], c.clone());
    assert!(o.cancel_with_reasons(&[CancelReason::AdminStop, CancelReason::Unknown]));
    assert!(!o.cancel_with_reasons(&[CancelReason::EpochNotMatch]));
    assert_eq!(
        o.additional_info().get("cancel-reason").as_deref(),
        Some("admin stop")
    );
    assert!(!o.replace());
    assert!(!o.start());
    assert_eq!(o.status(), CANCELED);
    assert_eq!(o.to_json_object()["status"], 3);
    let o = op(OpKind::LEADER, vec![leader()], c.clone());
    assert!(o.cancel());
    assert!(!o.cancel_with_reasons(&[CancelReason::Unknown]));
    assert_eq!(
        o.additional_info().get("cancel-reason").as_deref(),
        Some("unknown")
    );
    let mut a = op(OpKind::MERGE, vec![leader()], c.clone());
    let b = Operator::new("b", "b", 99, None, OpKind::MERGE, 1, vec![]);
    a.sync(&b);
    assert_eq!(a.timeout_nanos(), 0);
    assert_eq!(a.additional_info().related_merge_region(), 99);
    assert_eq!(b.additional_info().related_merge_region(), 1);
    let a = op(OpKind::LEADER, vec![leader()], c);
    assert!(!a.replace());
    a.start();
    assert!(a.replace());
    assert!(!a.cancel_with_reasons(&[CancelReason::StaleStatus]));
    assert_eq!(a.status(), REPLACED);
    assert_eq!(a.to_json_object()["status"], 4);
}
#[test]
fn task_025_history_record_and_output() {
    let c = clock();
    let steps = vec![
        leader(),
        AddPeer {
            to_store: 3,
            ..Default::default()
        }
        .into(),
        AddLearner {
            to_store: 4,
            ..Default::default()
        }
        .into(),
        RemovePeer {
            from_store: 5,
            ..Default::default()
        }
        .into(),
        RemovePeer {
            from_store: 6,
            ..Default::default()
        }
        .into(),
        RemovePeer {
            from_store: 7,
            ..Default::default()
        }
        .into(),
    ];
    let o = op(OpKind::REGION | OpKind::LEADER, steps, c.clone());
    let now = c.now();
    let h = o.history_at(now);
    for (h, v) in h.iter().zip(fixture()["history"].as_array().unwrap()) {
        assert_eq!(
            (h.from, h.to, h.kind.as_i32()),
            (
                v[0].as_u64().unwrap(),
                v[1].as_u64().unwrap(),
                v[2].as_i64().unwrap() as i32
            )
        );
        assert_eq!(h.finish_time, now);
    }
    assert_eq!(h.len(), 3);
    assert_eq!(h[0].kind, ResourceKind::LEADER);
    o.start();
    *c.0.lock().unwrap() += Duration::from_secs(2);
    let peer = Peer {
        id: 2,
        store_id: 2,
        ..Default::default()
    };
    let r = RegionInfo::new(
        RegionMeta {
            id: 1,
            peers: vec![peer.clone()],
            ..Default::default()
        },
        Some(peer),
        vec![],
        vec![],
    );
    o.check(&r);
    *c.0.lock().unwrap() += Duration::from_secs(3);
    o.cancel();
    let rec = o.record(c.now());
    assert_eq!(rec.duration_nanos, Some(3_000_000_000));
    assert_eq!(rec.finish_time, c.now());
    let render = |t: Option<Instant>| {
        if t.is_some() {
            "TIME".to_owned()
        } else {
            "ZERO".to_owned()
        }
    };
    let text = o.render(&render);
    assert!(text.contains(
        "kind:region,leader, region:1(0, 0), createAt:TIME, startAt:TIME, currentStep:1"
    ));
    assert_eq!(
        serde_json::from_str::<String>(&o.json_string(&render)).unwrap(),
        text
    );
    assert!(
        rec.render(&render)
            .unwrap()
            .ends_with("(finishAt:TIME, duration:3s)")
    );
    assert_eq!(
        serde_json::from_str::<String>(&rec.json_string(&render).unwrap()).unwrap(),
        rec.render(&render).unwrap()
    );
    let fresh = op(OpKind::LEADER, vec![], clock());
    assert_eq!(fresh.record(Instant::now()).duration_nanos, None);
    assert_eq!(
        fresh.record(Instant::now()).render(&render),
        Err(UnreachedStart)
    );
    assert!(
        fresh
            .record(Instant::now())
            .with_zero_start_duration(i64::MAX)
            .render(&render)
            .unwrap()
            .ends_with("duration:2562047h47m16.854775807s)")
    );
    fresh.start();
    fresh.check_success();
    assert!(fresh.render(&render).ends_with(" finished"));
    assert_eq!(fresh.to_json_object()["status"], 2);
    assert!(
        fresh
            .record(fresh.reach_time_of(STARTED).unwrap() - Duration::from_secs(1))
            .duration_nanos
            .unwrap()
            < 0
    );
}
#[test]
fn task_025_terminal_once_and_metadata_race() {
    let o = op(OpKind::LEADER, vec![leader()], clock());
    o.start();
    let wins = std::thread::scope(|s| {
        let handles: Vec<_> = (0..32)
            .map(|i| {
                let o = &o;
                s.spawn(move || {
                    if i % 2 == 0 {
                        o.cancel_with_reasons(&[CancelReason::AdminStop])
                    } else {
                        o.replace()
                    }
                })
            })
            .collect();
        handles
            .into_iter()
            .filter(|h| h.thread().id() != std::thread::current().id())
            .map(|h| h.join().unwrap() as usize)
            .sum::<usize>()
    });
    assert_eq!(wins, 1);
    assert!(o.is_end());
    let terminal = o.reach_time_of(o.status());
    assert!(!o.replace());
    assert!(!o.cancel());
    assert_eq!(o.reach_time_of(o.status()), terminal);
    o.additional_info().set("x", "\"<&\n");
    assert!(o.additional_info().log().contains("\\u003c"));
}

#[test]
fn task_025_output_boundaries() {
    let render = |_: Option<Instant>| "TIME".to_string();
    let c = clock();
    let o = op(OpKind::LEADER, vec![leader()], c.clone());
    assert_eq!(
        o.render(&render),
        "test {test} (kind:leader, region:1(0, 0), createAt:TIME, startAt:TIME, currentStep:0, size:1, steps:[0:{transfer leader from store 1 to store 2}], timeout:[1m0s])"
    );
    assert_eq!(
        o.to_json_object(),
        serde_json::json!({"desc":"test","brief":"test","region_id":1,"region_epoch":null,"kind":256,"timeout":"1m0s","status":0})
    );
    o.start();
    *c.0.lock().unwrap() += Duration::from_secs(60);
    assert_eq!(o.to_json_object()["status"], 6);
    assert!(o.render(&render).ends_with(" timeout"));
    let o = op(OpKind::LEADER, vec![leader()], clock());
    assert!(o.history_at(Instant::now()).len() == 1);
    let c = clock();
    let o = op(OpKind::LEADER, vec![leader()], c.clone());
    o.start();
    *c.0.lock().unwrap() += Duration::from_secs(2);
    let peer = Peer {
        id: 2,
        store_id: 2,
        ..Default::default()
    };
    let r = RegionInfo::new(
        RegionMeta {
            peers: vec![peer.clone()],
            ..Default::default()
        },
        Some(peer),
        vec![],
        vec![],
    );
    o.check(&r);
    *c.0.lock().unwrap() += Duration::from_secs(3);
    assert_eq!(o.record(c.now()).duration_nanos, Some(5_000_000_000));
    assert!(!o.cancel_with_reasons(&[CancelReason::Timeout]));
    assert_eq!(o.status(), SUCCESS);
    for (reason, text) in [
        (CancelReason::RegionNotFound, "region not found"),
        (CancelReason::EpochNotMatch, "epoch not match"),
        (CancelReason::AlreadyExist, "already exist"),
        (CancelReason::AdminStop, "admin stop"),
        (CancelReason::NotInRunningState, "not in running state"),
        (CancelReason::Timeout, "timeout"),
        (CancelReason::Expired, "expired"),
        (CancelReason::NotInCreateStatus, "not in create status"),
        (CancelReason::StaleStatus, "stale status"),
        (CancelReason::ExceedStoreLimit, "exceed store limit"),
        (CancelReason::ExceedWaitLimit, "exceed wait limit"),
        (CancelReason::RelatedMergeRegion, "related merge region"),
        (CancelReason::StartFailed, "start failed"),
        (CancelReason::Unknown, "unknown"),
    ] {
        let o = op(OpKind::REGION, vec![], clock());
        assert!(o.cancel_with_reasons(&[reason]));
        assert_eq!(o.additional_info().get("cancel-reason").unwrap(), text);
    }
    let o = Operator::new(
        "a\"b",
        "test",
        1,
        Some(astersql_pd_core::RegionEpoch {
            conf_ver: 0,
            version: 3,
        }),
        OpKind::REGION,
        1,
        vec![],
    );
    assert_eq!(
        o.to_json_object()["region_epoch"],
        serde_json::json!({"version":3})
    );
    // Go MarshalJSON directly wraps String without escaping; retain its invalid-JSON boundary.
    assert!(serde_json::from_str::<serde_json::Value>(&o.json_string(&render)).is_err());
}

#[test]
fn task_025_duration_saturation() {
    let c = clock();
    let o = op(OpKind::LEADER, vec![], c.clone());
    o.start();
    o.check_success();
    let span = Duration::from_secs(10_000_000_000);
    let start = c.now();
    assert_eq!(
        o.record(start.checked_add(span).unwrap()).duration_nanos,
        Some(i64::MAX)
    );
    let negative = o.record(start.checked_sub(span).unwrap());
    assert_eq!(negative.duration_nanos, Some(i64::MIN));
    assert!(
        negative
            .render(&|_| "TIME".into())
            .unwrap()
            .ends_with("duration:-2562047h47m16.854775808s)")
    );
}
