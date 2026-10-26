// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{Peer, RegionInfo, StoreInfo, StoreMeta};
use astersql_pd_core_constant::ResourceKind as K;
use astersql_pd_schedule_operator::*;
use astersql_pd_statistics::*;
use astersql_pd_statistics_utils::RWType as R;
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_086.json"
    ))
    .unwrap()
}
fn values(v: &Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect()
}
fn detail(current: Loads, future: Loads) -> StoreLoadDetail {
    StoreLoadDetail {
        store_summary_info: StoreSummaryInfo::default(),
        hot_peers: vec![],
        load_pred: StoreLoadPred {
            current: StoreLoad {
                loads: current,
                ..Default::default()
            },
            future: StoreLoad {
                loads: future,
                ..Default::default()
            },
            ..Default::default()
        },
    }
}
#[test]
fn task_086_behavior_cases() {
    let f = fixture();
    assert_eq!(f["evidence_kind"], "source_assertion");
    let src = detail([100., 80., 60., 40.], [90., 100., 60., 30.]);
    let dst = detail([20., 30., 40., 50.], [25., 10., 45., 55.]);
    let cases = f["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    for c in cases {
        let main = HotPeerStat {
            loads: values(&c["main"]),
            ..Default::default()
        };
        let revert = (!c["revert"].is_null()).then(|| HotPeerStat {
            loads: values(&c["revert"]),
            ..Default::default()
        });
        let mut s = HotSolution::new(&src, &dst, &main, revert.as_ref());
        let dims: Vec<_> = c["dims"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_u64().unwrap() as usize)
            .collect();
        s.calc_peers_rate(&dims);
        for d in 0..4 {
            assert_eq!(s.peers_rate(d), c["rates"][d].as_f64().unwrap(), "{c}");
            for (actual, key) in [
                (s.current_load(d), "current"),
                (s.pending_load(d), "pending"),
                (s.extreme_load(d), "extreme"),
            ] {
                assert_eq!(
                    actual,
                    (
                        c[key][d][0].as_f64().unwrap(),
                        c[key][d][1].as_f64().unwrap()
                    ),
                    "{} dim={d} {key}",
                    c["name"]
                );
            }
        }
        s.calc_peers_rate(&[]);
        for d in 0..4 {
            assert_eq!(s.peers_rate(d), 0.);
        }
    }
    println!("task_086: 6 projection fixtures x 4 dimensions, recache clears all dimensions");
}
struct ManualClock(Mutex<Instant>);
impl Clock for ManualClock {
    fn now(&self) -> Instant {
        *self.0.lock().unwrap()
    }
}
impl ManualClock {
    fn advance(&self, seconds: u64) {
        *self.0.lock().unwrap() += Duration::from_secs(seconds);
    }
}
fn operator(clock: Arc<ManualClock>) -> Arc<Operator> {
    Arc::new(Operator::with_clock(
        "hot",
        "",
        1,
        None,
        OpKind::HOT_REGION | OpKind::LEADER,
        1,
        vec![OpStep::TransferLeader(TransferLeader {
            from_store: 1,
            to_store: 2,
            to_stores: vec![],
        })],
        clock,
    ))
}
fn stores() -> HashMap<u64, StoreSummaryInfo> {
    summary_store_infos(
        &(1..=3)
            .map(|id| {
                StoreInfo::new(StoreMeta {
                    id,
                    ..Default::default()
                })
            })
            .collect::<Vec<_>>(),
    )
}
fn pending(op: Arc<Operator>, froms: Vec<u64>, to: u64) -> PendingInfluence {
    PendingInfluence {
        op,
        froms,
        to,
        origin: Influence {
            loads: vec![10., 20., 30., 40., 50., 60., 70., 80.],
            hot_peer_count: 2.,
        },
        max_zombie_duration: Duration::from_secs(30),
    }
}
#[test]
fn task_086_pending_lifecycle_and_faults() {
    let f = fixture();
    let cases = f["pending"].as_array().unwrap();
    assert_eq!(cases.len(), 15);
    for c in cases {
        let clock = Arc::new(ManualClock(Mutex::new(Instant::now())));
        let op = operator(clock.clone());
        let status = c[0].as_str().unwrap();
        if status != "created" && status != "expired" {
            assert!(op.start());
        }
        match status {
            "success" => {
                op.check(&RegionInfo::new(
                    Default::default(),
                    Some(Peer {
                        store_id: 2,
                        ..Default::default()
                    }),
                    vec![],
                    vec![],
                ));
                assert_eq!(op.status(), SUCCESS);
            }
            "canceled" => {
                assert!(op.cancel());
            }
            "replaced" => {
                assert!(op.replace());
            }
            "timeout" => {
                clock.advance(100);
                assert!(op.check_timeout());
            }
            "expired" => {
                clock.advance(4);
                assert!(op.check_expired());
            }
            _ => {}
        }
        clock.advance(c[1].as_u64().unwrap());
        let mut p = pending(op, vec![1], 2);
        p.max_zombie_duration = Duration::from_secs(c[2].as_u64().unwrap());
        let mut base = HotLoadProjection::default();
        base.region_pendings.insert(1, p);
        let mut infos = stores();
        base.summary_pending_influence(
            &mut infos,
            Some(&HotRegionParams {
                pending_weight: 2.,
                ..Default::default()
            }),
            clock.as_ref(),
        );
        let weight = c[3].as_f64().unwrap() * 2.;
        assert_eq!(
            !base.region_pendings.contains_key(&1),
            c[4].as_bool().unwrap(),
            "{c}"
        );
        if weight > 0. {
            assert_eq!(
                infos[&1].pending_sum.as_ref().unwrap().loads[0],
                -10. * weight,
                "{c}"
            );
            assert_eq!(
                infos[&2].pending_sum.as_ref().unwrap().hot_peer_count,
                2. * weight
            );
        } else {
            assert!(infos.values().all(|i| i.pending_sum.is_none()), "{c}");
        }
    }
    // Summary must itself refresh expiration and timeout, without prior calls.
    for started in [false, true] {
        let clock = Arc::new(ManualClock(Mutex::new(Instant::now())));
        let op = operator(clock.clone());
        if started {
            op.start();
        }
        clock.advance(100);
        let mut base = HotLoadProjection::default();
        base.region_pendings
            .insert(1, pending(op.clone(), vec![1], 2));
        let mut infos = stores();
        base.summary_pending_influence(&mut infos, None, clock.as_ref());
        assert_eq!(op.status(), if started { TIMEOUT } else { EXPIRED });
        assert!(infos.values().all(|i| i.pending_sum.is_none()));
        assert!(base.region_pendings.contains_key(&1));
        clock.advance(30);
        base.summary_pending_influence(&mut stores(), None, clock.as_ref());
        assert!(base.region_pendings.is_empty());
    }
    println!("task_086: 15 lifecycle fixtures, 2 automatic fault transitions");
}
#[test]
fn task_086_summary_weights_missing_and_multiple_sources() {
    for (configured, expected) in [
        (None, 1.),
        (Some(-1.), 1.),
        (Some(0.), 1.),
        (Some(0.5), 1.),
        (Some(1.), 1.),
        (Some(2.5), 2.5),
    ] {
        let config = configured.map(|w| HotRegionParams {
            pending_weight: w,
            ..Default::default()
        });
        assert_eq!(effective_hot_pending_weight(config.as_ref()), expected);
        let clock = Arc::new(ManualClock(Mutex::new(Instant::now())));
        let op = operator(clock.clone());
        op.start();
        let mut base = HotLoadProjection::default();
        base.region_pendings
            .insert(1, pending(op, vec![1, 3, 99], 2));
        let mut infos = stores();
        base.summary_pending_influence(&mut infos, config.as_ref(), clock.as_ref());
        assert_eq!(
            infos[&1].pending_sum.as_ref().unwrap().loads,
            vec![
                -10. * expected,
                -20. * expected,
                -30. * expected,
                -40. * expected,
                -50. * expected,
                -60. * expected,
                -70. * expected,
                -80. * expected
            ]
        );
        assert_eq!(
            infos[&2].pending_sum.as_ref().unwrap().hot_peer_count,
            6. * expected
        );
        assert_eq!(
            infos[&3].pending_sum.as_ref().unwrap().hot_peer_count,
            -2. * expected
        );
        let src = StoreLoad {
            loads: [100.; 4],
            hot_peer_count: 8.,
            ..Default::default()
        }
        .to_load_pred(R::READ, infos[&1].pending_sum.as_ref());
        assert_eq!(
            src.future.loads,
            [
                100. - 10. * expected,
                100. - 20. * expected,
                100. - 30. * expected,
                100. - 70. * expected
            ]
        );
        assert_eq!(src.future.hot_peer_count, 8. - 2. * expected);
        let mut missing = HashMap::new();
        base.summary_pending_influence(&mut missing, config.as_ref(), clock.as_ref());
        assert_eq!(base.region_pendings.len(), 1);
        let same = pending(operator(clock.clone()), vec![1], 1);
        same.op.start();
        base.region_pendings.clear();
        base.region_pendings.insert(2, same);
        let mut infos = stores();
        base.summary_pending_influence(&mut infos, None, clock.as_ref());
        assert_eq!(infos[&1].pending_sum.as_ref().unwrap().hot_peer_count, 0.);
    }
    println!(
        "task_086: 6 effective weights, multi/missing/same-store sources, real READ projection"
    );
}
#[test]
fn task_086_history_configuration() {
    let mut base = HotLoadProjection::default();
    base.history
        .add(1, R::READ, K::LEADER, [5.; 4], Duration::ZERO);
    assert!(!base.update_history_load_config(Duration::from_secs(300), Duration::from_secs(30)));
    assert_eq!(base.history.get(1, R::READ, K::LEADER)[0][0], 5.);
    assert!(base.update_history_load_config(Duration::from_secs(10), Duration::from_secs(5)));
    assert!(base.history.get(1, R::READ, K::LEADER)[0].is_empty());
    base.history
        .add(1, R::READ, K::LEADER, [7.; 4], Duration::ZERO);
    base.history
        .add(1, R::READ, K::LEADER, [9.; 4], Duration::from_secs(4));
    assert_eq!(base.history.get(1, R::READ, K::LEADER)[0], vec![7., 0.]);
    base.history
        .add(1, R::READ, K::LEADER, [9.; 4], Duration::from_secs(5));
    assert_eq!(base.history.get(1, R::READ, K::LEADER)[0], vec![7., 9.]);
    assert!(base.update_history_load_config(Duration::ZERO, Duration::ZERO));
    base.history
        .add(1, R::READ, K::LEADER, [9.; 4], Duration::ZERO);
    assert!(base.history.get(1, R::READ, K::LEADER)[0].is_empty());
}
#[test]
fn task_086_cache_precondition() {
    let src = detail([0.; 4], [0.; 4]);
    let main = HotPeerStat::default();
    let mut s = HotSolution::new(&src, &src, &main, None);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| s.extreme_load(0))).is_err());
    s.calc_peers_rate(&[0]);
    assert_eq!(s.extreme_load(0), (0., 0.));
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| s.calc_peers_rate(&[4]))).is_err()
    );
}

#[test]
fn task_086_cache_rolling_and_summary_cancellation() {
    let store = detail([0.; 4], [0.; 4]);
    let mut rolling = DimStat::new(1_000_000_000);
    rolling.add(12.6, 1_000_000_000);
    let main = HotPeerStat {
        loads: vec![999., 5.],
        rolling_loads: Some(vec![Some(rolling)]),
        ..Default::default()
    };
    let revert = HotPeerStat {
        loads: vec![3.4],
        ..Default::default()
    };
    let mut s = HotSolution::new(&store, &store, &main, Some(&revert));
    s.calc_peers_rate(&[0, 1, 2, 3]);
    assert_eq!(
        (
            s.peers_rate(0),
            s.peers_rate(1),
            s.peers_rate(2),
            s.peers_rate(3)
        ),
        (10., 5., 0., 0.)
    );
    s.revert_peer = None;
    s.calc_peers_rate(&[0]);
    assert_eq!(s.peers_rate(0), 13.);
    assert_eq!(s.peers_rate(1), 0.);
    let clock = Arc::new(ManualClock(Mutex::new(Instant::now())));
    let mut base = HotLoadProjection::default();
    let op1 = operator(clock.clone());
    let op2 = operator(clock.clone());
    op1.start();
    op2.start();
    base.region_pendings
        .insert(1, pending(op1.clone(), vec![1], 2));
    base.region_pendings
        .insert(2, pending(op2.clone(), vec![2], 1));
    let empty = operator(clock.clone());
    empty.cancel();
    clock.advance(30);
    base.region_pendings.insert(3, pending(empty, vec![], 2));
    let mut infos = stores();
    base.summary_pending_influence(&mut infos, None, clock.as_ref());
    // Clock is still before transfer timeout. Opposing records cancel all eight rates and counts.
    for id in [1, 2] {
        assert_eq!(
            infos[&id].pending_sum.as_ref().unwrap(),
            &Influence {
                loads: vec![0.; 8],
                hot_peer_count: 0.
            }
        );
    }
    assert!(base.region_pendings.contains_key(&3));
    assert!(op1.cancel());
    assert!(op2.cancel());
    let mut infos = stores();
    base.summary_pending_influence(&mut infos, None, clock.as_ref());
    assert!(infos.values().all(|i| i.pending_sum.is_none()));
    clock.advance(30);
    base.summary_pending_influence(&mut stores(), None, clock.as_ref());
    assert_eq!(base.region_pendings.len(), 1);
    assert!(base.region_pendings.contains_key(&3));
}
