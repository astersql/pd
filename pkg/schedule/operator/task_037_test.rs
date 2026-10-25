// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{BasicCluster, RegionInfo, RegionMeta};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
struct Manual(Mutex<Instant>);
impl Clock for Manual {
    fn now(&self) -> Instant {
        *self.0.lock().unwrap()
    }
}
fn op(id: u64, clock: Arc<dyn Clock>) -> Arc<Operator> {
    Arc::new(Operator::with_clock(
        "records",
        "",
        id,
        None,
        OpKind::LEADER,
        1,
        vec![
            TransferLeader {
                from_store: 1,
                to_store: 2,
                ..Default::default()
            }
            .into(),
        ],
        clock,
    ))
}
#[test]
fn task_037_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_037.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let clock = Arc::new(Manual(Mutex::new(Instant::now())));
        let o = op(1, clock.clone());
        let state = case["status"].as_u64().unwrap() as u32;
        match OpStatus(state) {
            SUCCESS => {
                o.start();
                o.check(&RegionInfo::new(
                    RegionMeta::default(),
                    Some(astersql_pd_core::Peer {
                        store_id: 2,
                        ..Default::default()
                    }),
                    vec![],
                    vec![],
                ));
            }
            REPLACED => {
                o.start();
                o.replace();
            }
            EXPIRED => {
                *clock.0.lock().unwrap() += Duration::from_secs(601);
                o.check_expired();
            }
            TIMEOUT => {
                o.start();
                *clock.0.lock().unwrap() += Duration::from_secs(61);
                o.check_timeout();
            }
            CANCELED => {
                o.cancel();
            }
            _ => {}
        }
        let now = clock.now();
        let mut records = ControllerRecords::default();
        let called = Arc::new(Mutex::new(Vec::new()));
        let c = called.clone();
        records.set_success_callbacks(vec![
            None,
            Some(Arc::new(move |o| {
                c.lock().unwrap().push(o.region_id());
            })),
        ]);
        let event = records.bury_operator(o.clone(), now).unwrap();
        assert_eq!(event.event, case["event"].as_str().unwrap());
        assert_eq!(event.unexpected, state == CREATED.0);
        assert_eq!(records.get_records(now, now).len(), 1);
        assert!(
            records
                .bury_operator(o.clone(), now + Duration::from_secs(1))
                .is_none()
        );
        assert_eq!(
            called.lock().unwrap().len(),
            usize::from(state == SUCCESS.0)
        );
        assert_eq!(
            records
                .get_status(&ControllerAdmission::new(1), 1, now)
                .unwrap()
                .status,
            if state == CREATED.0 {
                CANCELED
            } else {
                OpStatus(state)
            }
        );
        assert_eq!(records.get_history(now, now).len(), 1);
        assert_eq!(records.get_history(now, now)[0].finish_time, now);
        assert!(
            records
                .get_records(now + Duration::from_nanos(1), now)
                .is_empty()
        );
        assert!(
            records
                .get_status(
                    &ControllerAdmission::new(1),
                    1,
                    now + Duration::from_secs(600)
                )
                .is_some()
        );
        assert!(
            records
                .get_records(now, now + Duration::from_secs(600))
                .is_empty()
        );
        assert!(
            records
                .get_status(
                    &ControllerAdmission::new(1),
                    1,
                    now + Duration::from_secs(601)
                )
                .is_none()
        );
        assert!(
            records
                .bury_operator(o, now + Duration::from_secs(602))
                .is_none()
        );
    }
}
#[test]
fn task_037_merge_cleanup_and_live_precedence() {
    let now = Instant::now();
    let clock = Arc::new(Manual(Mutex::new(now)));
    let mut cluster = BasicCluster::new();
    for id in [1, 2] {
        cluster.regions.set_region(Arc::new(RegionInfo::new(
            RegionMeta {
                id,
                key_range: astersql_pd_core::KeyRange {
                    start_key: vec![id as u8],
                    end_key: vec![id as u8 + 1],
                },
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        )));
    }
    let a = ControllerAdmission::new(10);
    let records = ControllerRecords::default();
    let x = op(1, clock.clone());
    let y = op(2, clock.clone());
    x.additional_info().set(RELATED_MERGE_REGION, "2");
    y.additional_info().set(RELATED_MERGE_REGION, "1");
    a.add_operators(&cluster, &[x.clone(), y.clone()]).unwrap();
    let removed = a.remove_operator(&x, CancelReason::EpochNotMatch);
    assert_eq!(records.bury_removed(&removed, now).len(), 2);
    assert!(records.bury_removed(&removed, now).is_empty());
    assert!(a.operators().is_empty());
    assert_eq!(records.get_records(now, now).len(), 2);
    assert_eq!(
        y.additional_info().get("cancel-reason").as_deref(),
        Some("related merge region")
    );
    let fresh = op(1, clock.clone());
    a.add_operators(&cluster, &[fresh.clone()]).unwrap();
    assert_eq!(records.get_status(&a, 1, now).unwrap().status, STARTED);
    assert!(Arc::ptr_eq(
        &records.get_status(&a, 1, now).unwrap().operator,
        &fresh
    ));
    fresh.cancel();
    records.bury_operator(fresh.clone(), now + Duration::from_secs(1));
    assert_eq!(
        records.get_records(now, now + Duration::from_secs(1)).len(),
        2
    );
    records.clean_all_op_records();
    assert!(records.get_records(now, now).is_empty());
    assert!(records.get_status(&a, 1, now).is_some());
    assert!(records.bury_operator(fresh, now).is_none());
}
#[test]
fn task_037_dispatch_outcome_and_reentrant_callback() {
    let now = Instant::now();
    let clock = Arc::new(Manual(Mutex::new(now)));
    let o = op(1, clock.clone());
    o.start();
    *clock.0.lock().unwrap() += Duration::from_secs(2);
    o.check(&RegionInfo::new(
        RegionMeta::default(),
        Some(astersql_pd_core::Peer {
            store_id: 2,
            ..Default::default()
        }),
        vec![],
        vec![],
    ));
    let seen = Arc::new(Mutex::new(0));
    let count = seen.clone();
    let records: Arc<ControllerRecords> =
        Arc::new_cyclic(|weak: &std::sync::Weak<ControllerRecords>| {
            let weak = weak.clone();
            ControllerRecords::default().with_success_callbacks(vec![Some(Arc::new(move |o| {
                let owner = weak.upgrade().unwrap();
                let stored = owner.get_records(now, now + Duration::from_secs(2));
                assert_eq!(stored[0].status, SUCCESS);
                assert!(
                    owner
                        .bury_operator(stored[0].operator.clone(), now)
                        .is_none()
                );
                assert_eq!(o.status(), SUCCESS);
                *count.lock().unwrap() += 1;
            }))])
        });
    let out = DispatchOutcome {
        completed: Some(o.clone()),
        removed: vec![o.clone()],
        ..Default::default()
    };
    let events = records.record_dispatch(&out, clock.now());
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].running_time, Some(Duration::from_secs(2)));
    assert!(records.record_dispatch(&out, clock.now()).is_empty());
    assert_eq!(*seen.lock().unwrap(), 1);
    // A completion without successful identity removal must not notify or archive.
    let fresh = op(3, clock);
    fresh.start();
    assert!(
        records
            .record_dispatch(
                &DispatchOutcome {
                    completed: Some(fresh),
                    ..Default::default()
                },
                now
            )
            .is_empty()
    );
}

#[test]
fn task_037_concurrent_burial_and_callback_failure() {
    let now = Instant::now();
    let o = op(1, Arc::new(Manual(Mutex::new(now))));
    o.cancel();
    let records = ControllerRecords::default();
    std::thread::scope(|scope| {
        let joins: Vec<_> = (0..8)
            .map(|_| {
                let records = &records;
                let o = o.clone();
                scope.spawn(move || usize::from(records.bury_operator(o, now).is_some()))
            })
            .collect();
        assert_eq!(
            joins.into_iter().map(|j| j.join().unwrap()).sum::<usize>(),
            1
        );
    });
    assert_eq!(records.get_records(now, now).len(), 1);
    let weak = Arc::downgrade(&o);
    drop(o);
    records.clean_all_op_records();
    assert!(weak.upgrade().is_none()); // weak guards do not retain operator state
    let o = Arc::new(Operator::new(
        "panic",
        "",
        2,
        None,
        OpKind::LEADER,
        1,
        vec![],
    ));
    o.start();
    o.check_success();
    let mut records = ControllerRecords::default();
    records.set_success_callbacks(vec![Some(Arc::new(|_| panic!("host callback fault")))]);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || records.bury_operator(o.clone(), now)
        ))
        .is_err()
    );
    assert_eq!(records.get_records(now, now)[0].status, SUCCESS);
    assert!(records.bury_operator(o, now).is_none()); // published before fault; mutex not poisoned
}

#[tokio::test]
async fn task_037_real_dispatch_archival() {
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_hbstream::routing::StreamRouter;
    let now = Instant::now();
    let clock = Arc::new(Manual(Mutex::new(now)));
    let region = RegionInfo::new(
        RegionMeta {
            id: 1,
            ..Default::default()
        },
        Some(astersql_pd_core::Peer {
            store_id: 2,
            ..Default::default()
        }),
        vec![],
        vec![],
    );
    let mut cluster = BasicCluster::new();
    cluster.regions.set_region(Arc::new(region.clone()));
    let a = ControllerAdmission::new(1);
    let cfg = ProviderSnapshot::<()>::default();
    let (h, _router) = StreamRouter::new(Arc::new(astersql_pd_core::StoresInfo::new()));
    let d = ControllerDispatch::new(&a, &cluster, &cfg, &h, 1);
    let o = op(1, clock);
    a.add_operators(&cluster, &[o.clone()]).unwrap();
    let callbacks = Arc::new(Mutex::new(vec![]));
    let capture = callbacks.clone();
    let mut records = ControllerRecords::default();
    // Set replaces, rather than appends to, the configured list.
    records.set_success_callbacks(vec![Some(Arc::new(|_| panic!("replaced hook")))]);
    records.set_success_callbacks(vec![
        None,
        Some(Arc::new(move |o| {
            capture.lock().unwrap().push(o.region_id())
        })),
    ]);
    let outcome = d
        .dispatch(&region, DispatchSource::Heartbeat, now, 0, true)
        .await
        .unwrap();
    assert_eq!(records.record_dispatch(&outcome, now)[0].event, "finish");
    assert!(a.operators().is_empty());
    assert_eq!(*callbacks.lock().unwrap(), vec![1]);
    assert_eq!(records.get_status(&a, 1, now).unwrap().status, SUCCESS);
    let again = d
        .dispatch(&region, DispatchSource::Heartbeat, now, 0, true)
        .await
        .unwrap();
    assert!(records.record_dispatch(&again, now).is_empty());
    let retained = records.get_records(now, now);
    assert_eq!(retained[0].record().duration_nanos, Some(0));
    records.clean_all_op_records();
    assert!(records.get_status(&a, 1, now).is_none());
}
