// Copyright 2026 AsterSQL.

use crate::*;
use astersql_pd_core::{BasicCluster, KeyRange, RegionEpoch, RegionInfo, RegionMeta};
use astersql_pd_core_constant::PriorityLevel;
use std::{
    sync::{Arc, Barrier, Mutex},
    time::{Duration, Instant},
};

struct ManualClock(Mutex<Instant>);
impl Clock for ManualClock {
    fn now(&self) -> Instant {
        *self.0.lock().unwrap()
    }
}
fn cluster() -> BasicCluster {
    let mut c = BasicCluster::new();
    for id in 1..=4 {
        c.regions.set_region(Arc::new(RegionInfo::new(
            RegionMeta {
                id,
                epoch: Some(RegionEpoch {
                    version: 3,
                    conf_ver: 4,
                }),
                key_range: KeyRange {
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
    c
}
fn op(id: u64, priority: i32) -> Arc<Operator> {
    let o = Operator::new(
        "admission",
        "",
        id,
        Some(RegionEpoch {
            version: 3,
            conf_ver: 4,
        }),
        OpKind::REGION | OpKind::LEADER,
        1,
        vec![
            AddPeer {
                to_store: 8,
                peer_id: 8,
                ..Default::default()
            }
            .into(),
        ],
    );
    o.set_priority_level(PriorityLevel::from_i32(priority));
    Arc::new(o)
}
fn pair(a: u64, b: u64, priority: i32) -> Vec<Arc<Operator>> {
    let a = op(a, priority);
    let b = op(b, priority);
    a.additional_info()
        .set(RELATED_MERGE_REGION, b.region_id().to_string());
    b.additional_info()
        .set(RELATED_MERGE_REGION, a.region_id().to_string());
    vec![a, b]
}
fn reason(o: &Operator) -> Option<String> {
    o.additional_info().get("cancel-reason")
}
#[test]
fn task_034_behavior_cases() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_034.json"
    ))
    .unwrap();
    assert_eq!(f["evidence_kind"], "source_assertion");
    for case in f["cases"].as_array().unwrap() {
        let c = cluster();
        let ctl = ControllerAdmission::new(1);
        let mut incoming = op(
            case["region"].as_u64().unwrap(),
            case["priority"].as_i64().unwrap() as i32,
        );
        if let Some(p) = case["old_priority"].as_i64() {
            ctl.add_operators(&c, &[op(1, p as i32)]).unwrap();
        }
        match case["setup"].as_str().unwrap_or("") {
            "stale-version" | "stale-confver" | "nil-epoch" => {
                let o = Arc::get_mut(&mut incoming).unwrap();
                *o = Operator::new(
                    "admission",
                    "",
                    1,
                    if case["setup"] == "nil-epoch" {
                        None
                    } else {
                        Some(RegionEpoch {
                            version: if case["setup"] == "stale-version" {
                                2
                            } else {
                                3
                            },
                            conf_ver: if case["setup"] == "stale-confver" {
                                3
                            } else {
                                4
                            },
                        })
                    },
                    OpKind::REGION,
                    1,
                    vec![],
                );
            }
            "started" => {
                incoming.start();
            }
            "canceled" => {
                incoming.cancel();
            }
            "waiting-full" => {
                ctl.enqueue_waiting(&c, &[op(2, 1)]).unwrap();
            }
            _ => {}
        }
        let before = ctl.operators().len();
        let result = ctl.add_operators(&c, &[incoming.clone()]);
        if case["expected"] == "accepted" {
            result.unwrap();
            assert!(Arc::ptr_eq(&ctl.get_operator(1).unwrap(), &incoming));
            assert_eq!(incoming.status(), STARTED);
        } else {
            let err = result.err().expect("must reject");
            assert_eq!(
                err.to_string(),
                case["expected"].as_str().unwrap(),
                "{}",
                case["name"]
            );
            assert_eq!(ctl.operators().len(), before);
        }
        assert_eq!(
            ctl.operator_count(OpKind::REGION),
            ctl.operators().len() as u64
        );
    }
}
#[test]
fn task_034_replace_and_identity_remove() {
    let c = cluster();
    let ctl = ControllerAdmission::new(10);
    let old = op(1, 1);
    ctl.add_operators(&c, &[old.clone()]).unwrap();
    let new = op(1, 2);
    let out = ctl.add_operators(&c, &[new.clone()]).unwrap();
    assert_eq!(old.status(), REPLACED);
    assert_eq!(out.removed.len(), 1);
    assert!(
        ctl.remove_operator(&old, CancelReason::AdminStop)
            .is_empty()
    );
    assert!(Arc::ptr_eq(&ctl.get_operator(1).unwrap(), &new));
    assert_eq!(ctl.operator_count(OpKind::REGION), 1);
    assert_eq!(ctl.operator_count(OpKind::LEADER), 0); // scheduler kind only
    assert_eq!(ctl.operator_count(OpKind::REGION | OpKind::LEADER), 0);
    assert_eq!(ctl.remove_operator(&new, CancelReason::AdminStop).len(), 1);
    assert_eq!(new.status(), CANCELED);
    assert_eq!(reason(&new).as_deref(), Some("admin stop"));
    assert_eq!(ctl.operator_count(OpKind::REGION), 0);
}
#[test]
fn task_034_merge_atomicity() {
    let c = cluster();
    let ctl = ControllerAdmission::new(10);
    let p = pair(1, 2, 1);
    ctl.add_operators(&c, &p).unwrap();
    let bad = pair(3, 9, 2);
    assert!(ctl.add_operators(&c, &bad).is_err());
    assert_eq!(ctl.operators().len(), 2);
    assert!(ctl.get_operator(3).is_none());
    assert!(bad.iter().all(|o| o.status() == CANCELED));
    let fresh = pair(1, 3, 2);
    let out = ctl.add_operators(&c, &fresh).unwrap();
    assert_eq!(out.removed.len(), 2);
    assert_eq!(p[0].status(), REPLACED);
    assert_eq!(p[1].status(), CANCELED);
    assert_eq!(reason(&p[1]).as_deref(), Some("related merge region"));
    assert!(ctl.get_operator(2).is_none());
    assert_eq!(ctl.operators().len(), 2);
    assert_eq!(
        ctl.remove_operator(&fresh[1], CancelReason::AdminStop)
            .len(),
        2
    );
    assert!(ctl.operators().is_empty());
    assert_eq!(ctl.operator_count(OpKind::REGION), 0);
    // A replacement must not accidentally remove its newly admitted partner.
    let old = pair(1, 2, 1);
    ctl.add_operators(&c, &old).unwrap();
    let next = pair(1, 2, 2);
    ctl.add_operators(&c, &next).unwrap();
    assert_eq!(ctl.operators().len(), 2);
    assert!(next.iter().all(|o| o.status() == STARTED));
    assert_eq!(ctl.remove_operators(CancelReason::AdminStop).len(), 2);
    assert_eq!(ctl.operator_count(OpKind::REGION), 0);
}
#[test]
fn task_034_waiting_promote() {
    let c = cluster();
    let ctl = ControllerAdmission::new(1);
    let first = op(1, 1);
    ctl.enqueue_waiting(&c, &[first.clone()]).unwrap();
    assert_eq!(ctl.waiting_count("admission"), 1);
    let rejected = op(2, 1);
    assert!(ctl.enqueue_waiting(&c, &[rejected.clone()]).is_err());
    assert_eq!(reason(&rejected).as_deref(), Some("exceed wait limit"));
    assert_eq!(ctl.waiting_count("admission"), 1);
    assert!(ctl.promote_waiting(&c, 0.0).unwrap().is_some());
    assert_eq!(ctl.waiting_count("admission"), 0);
    assert_eq!(first.status(), STARTED);
    let stale = op(2, 1);
    ctl.enqueue_waiting(&c, &[stale.clone()]).unwrap();
    let mut changed = cluster();
    changed.regions = astersql_pd_core::RegionsInfo::new();
    assert!(ctl.promote_waiting(&changed, 0.0).unwrap().is_none());
    assert_eq!(reason(&stale).as_deref(), Some("region not found"));
    assert_eq!(ctl.waiting_count("admission"), 0);
    ctl.remove_operators(CancelReason::AdminStop);
    let p = pair(1, 2, 1);
    ctl.enqueue_waiting(&c, &p).unwrap();
    assert_eq!(ctl.waiting_count("admission"), 1);
    assert!(ctl.promote_waiting(&c, f64::NAN).is_err());
    assert_eq!(ctl.waiting_count("admission"), 1);
    ctl.promote_waiting(&c, 0.5).unwrap().unwrap();
    assert_eq!(ctl.operators().len(), 2);
}
#[test]
fn task_034_expiry_and_invalid_batch() {
    let c = cluster();
    let ctl = ControllerAdmission::new(10);
    let clock = Arc::new(ManualClock(Mutex::new(Instant::now())));
    let expired = Arc::new(Operator::with_clock(
        "admission",
        "",
        1,
        Some(RegionEpoch {
            version: 3,
            conf_ver: 4,
        }),
        OpKind::REGION,
        1,
        vec![],
        clock.clone(),
    ));
    *clock.0.lock().unwrap() += Duration::from_secs(4);
    assert_eq!(
        ctl.add_operators(&c, &[expired.clone()]).err(),
        Some(AdmissionError::Rejected(CancelReason::Expired))
    );
    assert_eq!(expired.status(), EXPIRED);
    assert!(ctl.operators().is_empty());
    let x = op(1, 1);
    assert!(ctl.add_operators(&c, &[x.clone(), x]).is_err());
    assert!(ctl.add_operators(&c, &[]).is_err());
    assert!(ctl.add_operators(&c, &pair(1, 1, 1)).is_err());
    let p = pair(1, 2, 1);
    assert!(ctl.enqueue_waiting(&c, &p[..1]).is_err());
    assert_eq!(ctl.waiting_count("admission"), 0);
    // nil and explicit zero epoch are semantically equivalent in Go getters.
    let mut zeros = BasicCluster::new();
    zeros.regions.set_region(Arc::new(RegionInfo::new(
        RegionMeta {
            id: 1,
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    )));
    let nil = Arc::new(Operator::new("nil", "", 1, None, OpKind::REGION, 1, vec![]));
    ctl.add_operators(&zeros, &[nil]).unwrap();
}
#[test]
fn task_034_concurrent_admission_remove() {
    let ctl = Arc::new(ControllerAdmission::new(100));
    let c = Arc::new(cluster());
    let barrier = Arc::new(Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let ctl = ctl.clone();
            let c = c.clone();
            let b = barrier.clone();
            std::thread::spawn(move || {
                b.wait();
                for _ in 0..100 {
                    let p = pair(1, 2, 1);
                    if ctl.add_operators(&c, &p).is_ok() {
                        ctl.remove_operator(&p[0], CancelReason::AdminStop);
                    }
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    let remaining = ctl.operators();
    assert!(remaining.len() == 0 || remaining.len() == 2);
    assert_eq!(ctl.operator_count(OpKind::REGION), remaining.len() as u64);
    ctl.remove_operators(CancelReason::AdminStop);
    assert_eq!(ctl.operator_count(OpKind::REGION), 0);
}

#[test]
fn task_034_retry_expired_pairs_and_queue_errors() {
    let c = cluster();
    let ctl = ControllerAdmission::new(10);
    let stale = op(1, 1);
    let next = op(2, 1);
    ctl.enqueue_waiting(&c, &[stale.clone()]).unwrap();
    ctl.enqueue_waiting(&c, &[next.clone()]).unwrap();
    // Invalidate only the first queued region. The same call must skip it and
    // promote the second batch, even after its epoch changes in the registry.
    let mut changed = cluster();
    let mut meta = changed.regions.get_region(1).unwrap().meta().clone();
    meta.epoch.as_mut().unwrap().version += 1;
    changed
        .regions
        .set_region(Arc::new(RegionInfo::new(meta, None, vec![], vec![])));
    let out = ctl.promote_waiting(&changed, 0.0).unwrap().unwrap();
    assert!(Arc::ptr_eq(&out.admitted[0], &next));
    assert_eq!(reason(&stale).as_deref(), Some("epoch not match"));
    assert_eq!(ctl.waiting_count("admission"), 0);
    ctl.remove_operators(CancelReason::AdminStop);
    let clock = Arc::new(ManualClock(Mutex::new(Instant::now())));
    let p: Vec<_> = (1..=2)
        .map(|id| {
            Arc::new(Operator::with_clock(
                "expired",
                "",
                id,
                Some(RegionEpoch {
                    version: 3,
                    conf_ver: 4,
                }),
                OpKind::MERGE,
                1,
                vec![],
                clock.clone(),
            ))
        })
        .collect();
    p[0].additional_info().set(RELATED_MERGE_REGION, "2");
    p[1].additional_info().set(RELATED_MERGE_REGION, "1");
    *clock.0.lock().unwrap() += Duration::from_secs(3);
    assert_eq!(
        ctl.add_operators(&c, &p).err(),
        Some(AdmissionError::Rejected(CancelReason::Expired))
    );
    assert!(p.iter().all(|o| o.status() == EXPIRED));
    assert!(ctl.operators().is_empty());
    let invalid = op(3, 4);
    assert_eq!(
        ctl.enqueue_waiting(&c, &[invalid]).err(),
        Some(AdmissionError::Queue(QueueError::InvalidPriority(4)))
    );
    assert_eq!(ctl.waiting_count("admission"), 0);
    assert!(ctl.promote_waiting(&c, 0.0).unwrap().is_none());
    let zero = ControllerAdmission::new(0);
    let rejected = op(1, 1);
    assert_eq!(
        zero.add_operators(&c, &[rejected.clone()]).err(),
        Some(AdmissionError::Rejected(CancelReason::ExceedWaitLimit))
    );
    assert_eq!(reason(&rejected).as_deref(), Some("exceed wait limit"));
    assert!(zero.operators().is_empty());
}

#[tokio::test]
async fn task_034_factory_merge_and_affinity() {
    let c = cluster();
    let config = astersql_pd_schedule_config::ProviderSnapshot::<()>::default();
    for kind in [OpKind::MERGE, OpKind::AFFINITY] {
        let source = c.regions.get_region(1).unwrap();
        let target = c.regions.get_region(2).unwrap();
        let factory = RangeOperatorFactory::new(
            "factory-admission",
            BuilderContext {
                stores: &c.stores,
                config: &config,
                rules: &[],
                now_unix_nanos: 0,
            },
            &source,
            true,
        );
        let p = factory
            .create_merge(&target, kind, || {
                panic!("matching peers need no allocation")
            })
            .await
            .unwrap()
            .map(Arc::new);
        assert!(matches!(
            p[0].step(0),
            Some(OpStep::MergeRegion(MergeRegion {
                is_passive: false,
                ..
            }))
        ));
        assert!(matches!(
            p[1].step(0),
            Some(OpStep::MergeRegion(MergeRegion {
                is_passive: true,
                ..
            }))
        ));
        let ctl = ControllerAdmission::new(1);
        ctl.enqueue_waiting(&c, &p).unwrap();
        assert_eq!(ctl.waiting_count("factory-admission"), 1);
        let out = ctl.promote_waiting(&c, 0.0).unwrap().unwrap();
        assert_eq!(out.admitted.len(), 2);
        assert!(p.iter().all(|op| op.status() == STARTED));
        assert_eq!(ctl.operator_count(kind), 2);
        if kind == OpKind::AFFINITY {
            assert_eq!(ctl.operator_count(OpKind::MERGE), 0);
        }
        assert_eq!(ctl.remove_operator(&p[0], CancelReason::AdminStop).len(), 2);
        assert!(p.iter().all(|op| op.status() == CANCELED));
        assert_eq!(ctl.operator_count(kind), 0);
    }
}
