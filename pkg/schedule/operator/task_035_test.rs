// Copyright 2026 AsterSQL.
use crate::*;
use astersql_pd_core::{
    BasicCluster, KeyRange, Peer, RegionInfo, RegionMeta, RegionOption, StoreInfo, StoreMeta,
};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_storelimit::Type;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

fn cluster(size: i64) -> BasicCluster {
    let mut c = BasicCluster::new();
    for id in 1..=6 {
        let mut r = RegionInfo::new(
            RegionMeta {
                id,
                key_range: KeyRange {
                    start_key: vec![id as u8],
                    end_key: vec![id as u8 + 1],
                },
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        );
        r.stats_mut().approximate_size = size;
        r.stats_mut().approximate_keys = 100;
        c.regions.set_region(Arc::new(r));
    }
    c.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 8,
        ..Default::default()
    })));
    c
}
fn config(rate: f64) -> ProviderSnapshot<()> {
    let mut c = ProviderSnapshot::default();
    c.schedule.default_store_limit.add_peer = rate;
    c
}
fn op(id: u64, light: bool) -> Arc<Operator> {
    Arc::new(Operator::new(
        "limit",
        "",
        id,
        None,
        OpKind::REGION | OpKind::LEADER,
        96,
        vec![
            AddPeer {
                to_store: 8,
                peer_id: 8,
                is_light_weight: light,
                ..Default::default()
            }
            .into(),
        ],
    ))
}
#[tokio::test]
async fn task_035_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_035.json"
    ))
    .unwrap();
    assert_eq!(fixture["evidence_kind"], "source_assertion");
    for case in fixture["cost_cases"].as_array().unwrap() {
        let mut c = cluster(case["size"].as_i64().unwrap());
        let mut region = (*c.regions.get_region(1).unwrap()).clone();
        region.stats_mut().approximate_keys = case["keys"].as_i64().unwrap();
        c.regions.set_region(Arc::new(region));
        let o = op(1, case["light"].as_bool().unwrap());
        let inf = new_total_op_influence(&[o.clone()], &c);
        assert_eq!(
            inf.get_store_influence(8).get_step_cost(Type::ADD_PEER),
            case["cost"].as_i64().unwrap(),
            "{case}"
        );
        let ctl = ControllerAdmission::new(10);
        ctl.add_limited_operators(&c, &config(0.000001), &[o.clone()])
            .await
            .unwrap();
        assert_eq!(ctl.operator_count(OpKind::REGION), 1);
        assert_eq!(ctl.operator_count(OpKind::LEADER), 0);
        ctl.remove_limited_operator(&c, &config(120.0), &o, CancelReason::AdminStop);
        assert_eq!(
            c.stores
                .get_store(8)
                .unwrap()
                .limiter()
                .unwrap()
                .read()
                .unwrap()
                .rate(Type::ADD_PEER),
            2.0
        );
    }
}
#[tokio::test]
async fn task_035_tokens_rejection_and_removal() {
    let c = cluster(96);
    let cfg = config(0.000001);
    let ctl = ControllerAdmission::new(10);
    let a = op(1, false);
    assert!(!exceed_store_limit(&[a.clone()], &c, &cfg).await);
    assert!(!exceed_store_limit(&[a.clone()], &c, &cfg).await); // check consumes nothing
    ctl.add_limited_operators(&c, &cfg, &[a.clone()])
        .await
        .unwrap();
    assert_eq!(
        ctl.get_op_influence(Some(&c), &[])
            .get_store_influence(8)
            .region_size,
        96
    );
    let b = op(2, false);
    assert!(matches!(
        ctl.add_limited_operators(&c, &cfg, &[b.clone()]).await,
        Err(AdmissionError::Rejected(CancelReason::ExceedStoreLimit))
    ));
    assert_eq!(
        b.additional_info().get("cancel-reason").as_deref(),
        Some("exceed store limit")
    );
    ctl.remove_limited_operator(&c, &cfg, &a, CancelReason::AdminStop);
    assert!(ctl.get_op_influence(Some(&c), &[]).snapshot().is_empty());
    assert_eq!(ctl.operator_count(OpKind::REGION), 0);
    assert!(exceed_store_limit(&[op(3, false)], &c, &cfg).await); // v1 no refund
    let urgent = op(3, false);
    urgent.set_priority_level(PriorityLevel::URGENT);
    ctl.add_limited_operators(&c, &cfg, &[urgent])
        .await
        .unwrap();
    ctl.add_limited_operators(&c, &cfg, &[op(4, true)])
        .await
        .unwrap();
    ctl.add_limited_operators(&c, &config(120.0), &[op(5, false)])
        .await
        .unwrap(); // rate reset refills
}
#[tokio::test]
async fn task_035_batch_and_missing_store() {
    let c = cluster(96);
    let cfg = config(0.000001);
    let ctl = ControllerAdmission::new(10);
    assert!(exceed_store_limit(&[op(1, false), op(2, false)], &c, &cfg).await);
    assert!(!exceed_store_limit(&[], &c, &cfg).await);
    assert!(matches!(
        ctl.add_limited_operators(&c, &cfg, &[]).await,
        Err(AdmissionError::InvalidBatch)
    ));
    let absent = BasicCluster::new();
    assert!(!exceed_store_limit(&[op(1, false)], &absent, &cfg).await);
    let stale = op(9, false);
    assert!(ctl.add_limited_operators(&c, &cfg, &[stale]).await.is_err());
    ctl.add_limited_operators(&c, &cfg, &[op(1, false)])
        .await
        .unwrap(); // rejected admission didn't take
}
#[test]
fn task_035_range_options() {
    let c = cluster(96);
    let ctl = ControllerAdmission::new(10);
    let a = op(1, false);
    ctl.add_operators(&c, &[a.clone()]).unwrap();
    let range = |s, e| KeyRange {
        start_key: vec![s],
        end_key: vec![e],
    };
    assert!(!with_range_option(
        &[range(0, 1)],
        c.regions.get_region(1).as_deref()
    ));
    assert!(with_range_option(&[], c.regions.get_region(1).as_deref()));
    assert!(!with_range_option(&[], None));
    for (ranges, expected) in [
        (vec![range(1, 2)], true),
        (vec![range(0, 3), range(2, 4)], false),
        (vec![range(0, 0)], false),
    ] {
        let option = |r: &RegionInfo| with_range_option(&ranges, Some(r));
        assert_eq!(
            !ctl.get_op_influence(Some(&c), &[Some(&option), None])
                .snapshot()
                .is_empty(),
            expected
        );
    }
    assert!(ctl.get_op_influence(None, &[]).snapshot().is_empty());
    let mut missing = BasicCluster::new();
    assert!(
        ctl.get_op_influence(Some(&missing), &[])
            .snapshot()
            .is_empty()
    );
    let r = c
        .regions
        .get_region(1)
        .unwrap()
        .clone_with([RegionOption::AddPeer(Peer {
            id: 8,
            store_id: 8,
            ..Default::default()
        })]);
    missing.regions.set_region(Arc::new(r));
    assert!(
        ctl.get_op_influence(Some(&missing), &[])
            .snapshot()
            .is_empty()
    ); // finished step without Check
    a.check(&missing.regions.get_region(1).unwrap());
    assert!(a.check_success());
    assert!(ctl.get_op_influence(Some(&c), &[]).snapshot().is_empty());
}
struct ManualClock(Mutex<Instant>);
impl Clock for ManualClock {
    fn now(&self) -> Instant {
        *self.0.lock().unwrap()
    }
}
#[test]
fn task_035_fast_and_timeout() {
    let c = cluster(96);
    let ctl = ControllerAdmission::new(10);
    let clock = Arc::new(ManualClock(Mutex::new(Instant::now())));
    let a = Arc::new(Operator::with_clock(
        "limit",
        "",
        1,
        None,
        OpKind::REGION,
        96,
        vec![
            AddPeer {
                to_store: 8,
                peer_id: 8,
                ..Default::default()
            }
            .into(),
        ],
        clock.clone(),
    ));
    ctl.add_operators(&c, &[a.clone()]).unwrap();
    *clock.0.lock().unwrap() += Duration::from_nanos(a.timeout_nanos() as u64 + 1);
    assert!(ctl.get_op_influence(Some(&c), &[]).snapshot().is_empty());
    assert_eq!(a.status(), TIMEOUT);
    let fast = FastOperatorInfluence::default();
    let now = Instant::now();
    let b = op(2, true);
    // Dispatch owns success classification; cache always uses total influence.
    fast.put_at(b.clone(), now);
    let inf = OpInfluence::new();
    fast.add_to_at(&c, &inf, now + Duration::from_secs(9));
    assert_eq!(inf.get_store_influence(8).region_size, 96);
    assert_eq!(inf.get_store_influence(8).get_step_cost(Type::ADD_PEER), 0);
    let inf = OpInfluence::new();
    fast.add_to_at(&c, &inf, now + Duration::from_secs(10));
    assert!(inf.snapshot().is_empty());
}
#[tokio::test]
async fn task_035_waiting_limit() {
    let c = cluster(96);
    let cfg = config(0.000001);
    let ctl = ControllerAdmission::new(10);
    ctl.add_limited_operators(&c, &cfg, &[op(1, false)])
        .await
        .unwrap();
    let b = op(2, false);
    ctl.enqueue_waiting(&c, &[b.clone()]).unwrap();
    assert!(
        ctl.promote_limited_waiting(&c, &cfg, 0.0)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(b.status(), CANCELED);
    assert_eq!(ctl.waiting_count("limit"), 0);
    let light = op(3, true);
    ctl.enqueue_waiting(&c, &[light.clone()]).unwrap();
    assert!(
        ctl.promote_limited_waiting(&c, &cfg, 0.0)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(light.status(), STARTED);
}

#[tokio::test]
async fn task_035_snapshot_ack_and_replacement() {
    use astersql_pd_core::StoreLimiter;
    use astersql_pd_storelimit::SlidingWindows;
    let c = cluster(96);
    let cfg = config(60.0);
    c.stores.patch_store(8, |s| {
        s.set_store_limit(Some(StoreLimiter::V2(SlidingWindows::new())))
    });
    let ctl = ControllerAdmission::new(10);
    let learner = |id| {
        let o = Arc::new(Operator::new(
            "snapshot",
            "",
            id,
            None,
            OpKind::REGION,
            96,
            vec![
                AddLearner {
                    to_store: 8,
                    peer_id: 8,
                    send_store: 8,
                    ..Default::default()
                }
                .into(),
            ],
        ));
        o.set_priority_level(PriorityLevel::LOW);
        o
    };
    let a = learner(1);
    ctl.add_limited_operators(&c, &cfg, &[a.clone()])
        .await
        .unwrap();
    let handle = c.stores.get_store(8).unwrap().limiter().unwrap().clone();
    let used = || match &*handle.read().unwrap() {
        StoreLimiter::V2(w) => w.used()[0],
        _ => unreachable!(),
    };
    assert_eq!(used(), 96);
    assert!(exceed_store_limit(&[learner(2)], &c, &cfg).await);
    // Same region, higher priority is charged after the old member is acknowledged.
    let b = learner(1);
    b.set_priority_level(PriorityLevel::HIGH);
    ctl.add_limited_operators(&c, &cfg, &[b.clone()])
        .await
        .unwrap();
    assert_eq!(used(), 96);
    assert!(
        ctl.remove_limited_operator(&c, &cfg, &a, CancelReason::AdminStop)
            .is_empty()
    );
    assert_eq!(used(), 96);
    ctl.remove_limited_operator(&c, &cfg, &b, CancelReason::AdminStop);
    assert_eq!(used(), 0);
    assert!(!exceed_store_limit(&[learner(2)], &c, &cfg).await);
    c.stores.delete_store(8);
    // Region remains: missing store bypass must work with nonzero cached costs.
    assert!(!exceed_store_limit(&[learner(3)], &c, &cfg).await);
}
#[test]
fn task_035_concurrent_add_and_reset() {
    let c = Arc::new(cluster(96));
    let ctl = Arc::new(ControllerAdmission::new(10));
    let barrier = Arc::new(std::sync::Barrier::new(3));
    std::thread::scope(|scope| {
        for id in 1..=2 {
            let (c, ctl, barrier) = (c.clone(), ctl.clone(), barrier.clone());
            scope.spawn(move || {
                let cfg = config(120.0);
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                barrier.wait();
                rt.block_on(async {
                    for _ in 0..100 {
                        let a = op(id, false);
                        if ctl
                            .add_limited_operators(&c, &config(120.0), &[a.clone()])
                            .await
                            .is_ok()
                        {
                            ctl.remove_limited_operator(&c, &cfg, &a, CancelReason::AdminStop);
                        }
                    }
                });
            });
        }
        barrier.wait();
        for i in 0..100 {
            c.stores
                .reset_store_limit(8, Type::ADD_PEER, &[if i % 2 == 0 { 1.0 } else { 2.0 }]);
        }
    });
    assert_eq!(ctl.operator_count(OpKind::REGION), 0);
    assert!(ctl.get_op_influence(Some(&c), &[]).snapshot().is_empty());
}

#[tokio::test]
async fn task_035_small_token_boundary() {
    let c = cluster(20);
    let cfg = config(0.000001);
    let ctl = ControllerAdmission::new(10);
    for id in 1..=5 {
        ctl.add_limited_operators(&c, &cfg, &[op(id, false)])
            .await
            .unwrap();
    }
    assert!(exceed_store_limit(&[op(6, false)], &c, &cfg).await);
    ctl.remove_limited_operators(&c, &cfg, CancelReason::AdminStop);
    assert_eq!(ctl.operator_count(OpKind::REGION), 0);
    assert!(ctl.get_op_influence(Some(&c), &[]).snapshot().is_empty());
    assert!(exceed_store_limit(&[op(6, false)], &c, &cfg).await);
}
#[test]
fn task_035_partial_and_cached_total() {
    let mut c = cluster(20);
    let ctl = ControllerAdmission::new(10);
    let peer = Peer {
        id: 1,
        store_id: 1,
        ..Default::default()
    };
    let r = c.regions.get_region(1).unwrap().clone_with([
        RegionOption::Peers(vec![peer.clone()]),
        RegionOption::Leader(Some(peer)),
    ]);
    c.regions.set_region(Arc::new(r.clone()));
    let a = Arc::new(Operator::new(
        "move",
        "",
        1,
        None,
        OpKind::REGION,
        20,
        vec![
            AddLearner {
                to_store: 8,
                peer_id: 8,
                ..Default::default()
            }
            .into(),
            PromoteLearner {
                to_store: 8,
                peer_id: 8,
                ..Default::default()
            }
            .into(),
            TransferLeader {
                from_store: 1,
                to_store: 8,
                ..Default::default()
            }
            .into(),
            RemovePeer {
                from_store: 1,
                ..Default::default()
            }
            .into(),
        ],
    ));
    ctl.add_operators(&c, &[a.clone()]).unwrap();
    let total = new_total_op_influence(&[a.clone()], &c);
    assert_eq!(total.get_store_influence(8).region_size, 20);
    assert_eq!(
        total.get_store_influence(8).get_step_cost(Type::ADD_PEER),
        200
    );
    let r = r.clone_with([RegionOption::AddPeer(Peer {
        id: 8,
        store_id: 8,
        role: astersql_pd_core::PeerRole::Learner,
        ..Default::default()
    })]);
    a.check(&r);
    c.regions.set_region(Arc::new(r));
    let pending = ctl.get_op_influence(Some(&c), &[]);
    let target = pending.get_store_influence(8);
    assert_eq!(
        (
            target.region_size,
            target.region_count,
            target.leader_size,
            target.leader_count
        ),
        (0, 0, 20, 1)
    );
    assert_eq!(target.get_step_cost(Type::ADD_PEER), 0);
    assert_eq!(pending.get_store_influence(1).region_size, -20);
    assert_eq!(
        pending
            .get_store_influence(1)
            .get_step_cost(Type::REMOVE_PEER),
        200
    );
    let fast = FastOperatorInfluence::default();
    let now = Instant::now();
    fast.put_at(a.clone(), now);
    let inf = OpInfluence::new();
    fast.add_to_at(&c, &inf, now);
    assert_eq!(inf.snapshot(), total.snapshot()); // total, not unfinished
    c.regions = astersql_pd_core::RegionsInfo::new();
    assert_eq!(
        new_total_op_influence(&[a], &c).snapshot(),
        total.snapshot()
    );
}
