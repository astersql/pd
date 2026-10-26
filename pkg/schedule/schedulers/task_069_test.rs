// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{BasicCluster, StoreInfo, StoreMeta};
use astersql_pd_core_constant::Direction;
use astersql_pd_schedule_operator::*;
use astersql_pd_statistics::{StoreLoad, StoreLoadPred};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

fn load(rate: f64, count: f64) -> StoreLoad {
    StoreLoad {
        loads: [rate, 0., 0., 0.],
        hot_peer_count: count,
        ..Default::default()
    }
}
fn cmp() -> StoreLoadCmp {
    st_ld_rank_cmp(|ld| ld.loads[0], |v| (v / 10.) as i64)
}
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_069.json"
    ))
    .unwrap()
}
#[test]
fn task_069_behavior_cases() {
    let f = fixture();
    for c in f["rank"].as_array().unwrap() {
        assert_eq!(
            cmp()(
                &load(c[0].as_f64().unwrap(), 0.),
                &load(c[1].as_f64().unwrap(), 0.)
            ),
            c[2].as_i64().unwrap() as i32,
            "{c}"
        );
    }
    let a = load(11., 3.);
    let b = load(19., 2.);
    assert_eq!(neg_load_cmp(cmp())(&a, &load(30., 0.)), 1);
    assert_eq!(slice_load_cmp(vec![])(&a, &b), 0);
    assert_eq!(
        slice_load_cmp(vec![
            cmp(),
            st_ld_rank_cmp(|ld| ld.hot_peer_count, |v| v as i64)
        ])(&a, &b),
        1
    );
    assert_eq!(
        slice_load_cmp(vec![
            st_ld_rank_cmp(|ld| ld.hot_peer_count, |v| v as i64),
            neg_load_cmp(cmp())
        ])(&a, &load(30., 2.)),
        1
    );
    assert_eq!(
        slice_load_cmp(vec![
            Box::new(|_, _| 7),
            Box::new(|_, _| panic!("must short circuit"))
        ])(&a, &b),
        7
    );
    assert_eq!(neg_load_cmp(Box::new(|_, _| 7))(&a, &b), -7);
    let projected = StoreLoadPred {
        current: StoreLoad {
            loads: [4., -2., 9., -10.],
            hot_peer_count: 4.,
            ..Default::default()
        },
        future: StoreLoad {
            loads: [-1., 7., 3., -12.],
            hot_peer_count: 1.,
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(projected.min().loads, [-1., -2., 3., -12.]);
    assert_eq!(projected.max().loads, [4., 7., 9., -10.]);
    assert_eq!(projected.diff().loads, [5., 9., 6., 2.]);
    let a = StoreLoadPred {
        current: load(-20., 4.),
        future: load(30., 1.),
        ..Default::default()
    };
    let b = StoreLoadPred {
        current: load(0., 2.),
        future: load(10., 6.),
        ..Default::default()
    };
    assert_eq!(min_lp_cmp(cmp())(&a, &b), -1);
    assert_eq!(max_lp_cmp(cmp())(&a, &b), 1);
    assert_eq!(diff_cmp(cmp())(&a, &b), 1);
    assert_eq!(
        slice_lp_cmp(vec![min_lp_cmp(cmp()), max_lp_cmp(cmp())])(&a, &b),
        -1
    );
    assert_eq!(slice_lp_cmp(vec![])(&a, &b), 0);
    assert_eq!(
        (
            a.min().hot_peer_count,
            a.max().hot_peer_count,
            a.diff().hot_peer_count
        ),
        (1., 4., 3.)
    );
}
struct ManualClock(Mutex<Instant>);
impl Clock for ManualClock {
    fn now(&self) -> Instant {
        *self.0.lock().unwrap()
    }
}
fn op(clock: Arc<ManualClock>) -> Operator {
    Operator::with_clock(
        "pending",
        "",
        1,
        None,
        OpKind::LEADER,
        1,
        vec![OpStep::TransferLeader(TransferLeader {
            from_store: 1,
            to_store: 2,
            to_stores: vec![],
        })],
        clock,
    )
}
#[test]
fn task_069_pending_lifecycle() {
    for c in fixture()["pending"].as_array().unwrap() {
        let clock = Arc::new(ManualClock(Mutex::new(Instant::now())));
        let op = op(clock.clone());
        let status = c[0].as_str().unwrap();
        if status != "created" && status != "expired" {
            assert!(op.start());
        }
        match status {
            "success" => {
                let region = astersql_pd_core::RegionInfo::new(
                    Default::default(),
                    Some(astersql_pd_core::Peer {
                        store_id: 2,
                        ..Default::default()
                    }),
                    vec![],
                    vec![],
                );
                op.check(&region);
            }
            "canceled" => {
                assert!(op.cancel());
            }
            "replaced" => {
                assert!(op.replace());
            }
            "timeout" => {
                *clock.0.lock().unwrap() += Duration::from_secs(100);
                assert!(op.check_timeout());
            }
            "expired" => {
                *clock.0.lock().unwrap() += Duration::from_secs(4);
                assert!(op.check_expired());
            }
            _ => {}
        }
        *clock.0.lock().unwrap() += Duration::from_nanos(c[1].as_u64().unwrap());
        assert_eq!(
            calc_pending_influence(
                &op,
                Duration::from_nanos(c[2].as_u64().unwrap()),
                clock.as_ref()
            ),
            (c[3].as_f64().unwrap(), c[4].as_bool().unwrap()),
            "{c}"
        );
    }
    let clock = Arc::new(ManualClock(Mutex::new(Instant::now())));
    let op = op(clock.clone());
    *clock.0.lock().unwrap() += Duration::from_secs(4);
    assert_eq!(
        calc_pending_influence(&op, Duration::from_secs(1), clock.as_ref()),
        (0., false)
    );
    assert_eq!(op.status(), EXPIRED);
}
#[test]
fn task_069_pause_resume() {
    let cluster = BasicCluster::new();
    for id in 1..=3 {
        cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        })));
    }
    for direction in [Direction::IN, Direction::OUT] {
        let old = HashSet::from([1, 2]);
        let new = HashSet::from([2, 3, 99]);
        for id in &old {
            cluster
                .stores
                .pause_leader_transfer(*id, direction)
                .unwrap();
        }
        assert_eq!(
            pause_and_resume_leader_transfer(&cluster, direction, &old, &new),
            vec![astersql_pd_core::StoreControlError::StoreNotFound(99)]
        );
        let allowed = |id| {
            let s = cluster.stores.get_store(id).unwrap();
            if direction == Direction::IN {
                s.allow_leader_transfer_in()
            } else {
                s.allow_leader_transfer_out()
            }
        };
        assert!(allowed(1));
        assert!(!allowed(2));
        assert!(!allowed(3));
        assert!(pause_and_resume_leader_transfer(&cluster, direction, &new, &new).is_empty());
        assert!(
            pause_and_resume_leader_transfer(&cluster, direction, &new, &HashSet::new()).is_empty()
        );
        assert!(allowed(2));
        assert!(allowed(3));
    }
}

#[test]
fn task_069_pending_record_and_refresh() {
    use astersql_pd_statistics::{Influence, StoreSummaryInfo};
    let clock = Arc::new(ManualClock(Mutex::new(Instant::now())));
    let pending = PendingInfluence {
        op: Arc::new(op(clock.clone())),
        froms: vec![1],
        to: 2,
        origin: Influence {
            loads: vec![12., -4.],
            hot_peer_count: 1.,
        },
        max_zombie_duration: Duration::from_secs(10),
    };
    assert!(pending.op.start());
    assert_eq!(pending.froms, vec![1]);
    assert_eq!(pending.to, 2);
    let mut source = StoreSummaryInfo::default();
    let mut target = StoreSummaryInfo::default();
    let (weight, gc) = pending.weight(clock.as_ref());
    assert!(!gc);
    source.add_influence(Some(&pending.origin), -weight);
    target.add_influence(Some(&pending.origin), weight);
    assert_eq!(source.pending_sum.unwrap().loads, vec![-12., 4.]);
    assert_eq!(target.pending_sum.unwrap().hot_peer_count, 1.);
    *clock.0.lock().unwrap() += Duration::from_secs(100);
    assert_eq!(pending.weight(clock.as_ref()), (0., false));
    assert_eq!(pending.op.status(), TIMEOUT);
    *clock.0.lock().unwrap() += Duration::from_secs(10);
    assert_eq!(pending.weight(clock.as_ref()), (0., true));
    let mut fresh = StoreSummaryInfo::default();
    fresh.add_influence(Some(&pending.origin), pending.weight(clock.as_ref()).0);
    assert!(fresh.pending_sum.is_none());
}

// Advancing clock catches sampling age before a newly detected terminal state.
struct TickingClock(Mutex<Instant>);
impl Clock for TickingClock {
    fn now(&self) -> Instant {
        let mut time = self.0.lock().unwrap();
        *time += Duration::from_secs(4);
        *time
    }
}
#[test]
fn task_069_zero_window_after_refresh() {
    let clock = Arc::new(TickingClock(Mutex::new(Instant::now())));
    let op = Operator::with_clock(
        "expiry",
        "",
        1,
        None,
        OpKind::LEADER,
        1,
        vec![OpStep::TransferLeader(TransferLeader {
            from_store: 1,
            to_store: 2,
            to_stores: vec![],
        })],
        clock.clone(),
    );
    assert_eq!(
        calc_pending_influence(&op, Duration::ZERO, clock.as_ref()),
        (0., true)
    );
    assert_eq!(op.status(), EXPIRED);
}
