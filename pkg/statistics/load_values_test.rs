// Copyright 2026 AsterSQL.
use crate::{
    Influence, RegionKindLoads, StoreKindLoads, StoreLoad, StoreSummaryInfo, max_load, min_load,
    summary_store_infos,
};
use astersql_pd_core::{StoreInfo, StoreMeta};
use astersql_pd_statistics_utils::RWType;

fn current() -> StoreLoad {
    StoreLoad {
        loads: [100.0, 20.0, 30.0, 40.0],
        hot_peer_count: 5.0,
        history_loads: [vec![1.0, 2.0], vec![3.0], vec![], vec![4.0]],
    }
}
#[test]
fn state_task_13_load_values_weighted_influence() {
    let infl = Influence {
        loads: vec![2.0, -4.0, 6.0, 8.0, 10.0, -12.0, 14.0, -16.0],
        hot_peer_count: 2.0,
    };
    let snapshot = infl.clone();
    let mut summary = StoreSummaryInfo::default();
    summary.add_influence(None, 1.0);
    summary.add_influence(Some(&infl), 0.0);
    assert!(summary.pending_sum.is_none());
    for (weight, total) in [(0.5, 0.5), (-1.5, -1.0), (0.0, -1.0), (1.0, 0.0)] {
        summary.add_influence(Some(&infl), weight);
        let sum = summary.pending_sum.as_ref().unwrap();
        assert_eq!(
            sum.loads,
            infl.loads.iter().map(|v| v * total).collect::<Vec<_>>()
        );
        assert_eq!(sum.hot_peer_count, 2.0 * total);
    }
    assert_eq!(infl, snapshot);
    summary.add_influence(
        Some(&Influence {
            loads: vec![7.0],
            hot_peer_count: -1.0,
        }),
        2.0,
    );
    assert_eq!(
        summary.pending_sum.as_ref().unwrap().loads,
        vec![14.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]
    );
    assert_eq!(summary.pending_sum.unwrap().hot_peer_count, -2.0);
    let stores = [StoreInfo::new(StoreMeta {
        id: 9,
        ..Default::default()
    })];
    let mut summaries = summary_store_infos(&stores);
    assert_eq!(summaries[&9].store_info.id(), 9);
    assert!(summaries[&9].pending_sum.is_none());
    summaries
        .get_mut(&9)
        .unwrap()
        .store_info
        .set_last_heartbeat_unix_nanos(99);
    assert_eq!(stores[0].meta().unwrap().last_heartbeat, 0);
}
#[test]
fn state_task_13_load_values_predictions_and_snapshot_isolation() {
    let load = current();
    let before = load.clone();
    let infl = Influence {
        loads: vec![1.0, -2.0, 3.0, -4.0, 5.0, -6.0, 7.0, -8.0],
        hot_peer_count: -2.0,
    };
    let original = infl.clone();
    for (rw, expected) in [
        (RWType::READ, [101.0, 18.0, 33.0, 47.0]),
        (RWType::WRITE, [96.0, 25.0, 24.0, 32.0]),
        (RWType::from_i32(9), load.loads),
    ] {
        let mut pred = load.to_load_pred(rw, Some(&infl));
        assert_eq!(pred.current, load);
        assert_eq!(pred.future.loads, expected);
        assert_eq!(pred.future.hot_peer_count, 3.0);
        assert_eq!(pred.future.history_loads, crate::HistoryLoads::default());
        assert_eq!(pred.expect, StoreLoad::default());
        assert_eq!(pred.stddev, StoreLoad::default());
        pred.current.history_loads[0][0] = 999.0;
        pred.future.loads[0] = 999.0;
        assert_eq!(load, before);
    }
    let no_infl = load.to_load_pred(RWType::READ, None);
    assert_eq!(no_infl.future.loads, load.loads);
    assert_eq!(no_infl.future.hot_peer_count, load.hot_peer_count);
    assert_eq!(no_infl.future.history_loads, crate::HistoryLoads::default());
    assert_eq!(infl, original);
    assert_eq!(RegionKindLoads::default().len(), 8);
    assert_eq!(StoreKindLoads::default().len(), 12);
}
#[test]
fn state_task_13_load_values_min_max_pending_diff() {
    let a = current();
    let b = StoreLoad {
        loads: [110.0, 10.0, 30.0, -4.0],
        hot_peer_count: 2.0,
        ..current()
    };
    for (first, second) in [(&a, &b), (&b, &a)] {
        let pred = crate::StoreLoadPred {
            current: first.clone(),
            future: second.clone(),
            ..Default::default()
        };
        let before = pred.clone();
        for (actual, loads, peers) in [
            (pred.min(), [100.0, 10.0, 30.0, -4.0], 2.0),
            (pred.max(), [110.0, 20.0, 30.0, 40.0], 5.0),
            (pred.pending(), [10.0, 10.0, 0.0, 44.0], 0.0),
            (pred.diff(), [10.0, 10.0, 0.0, 44.0], 3.0),
        ] {
            assert_eq!(actual.loads, loads);
            assert_eq!(actual.hot_peer_count, peers);
            assert_eq!(actual.history_loads, crate::HistoryLoads::default());
        }
        assert_eq!(pred.min(), min_load(first, second));
        assert_eq!(pred.max(), max_load(first, second));
        assert_eq!(pred, before);
    }
    let equal = a.to_load_pred(RWType::READ, None);
    assert_eq!(equal.pending(), StoreLoad::default());
    assert_eq!(equal.diff(), StoreLoad::default());
    assert_eq!(equal.min().loads, a.loads);
}
#[test]
fn state_task_13_load_values_go_pending_sequence() {
    let mib = 1_048_576.0;
    for rw in [RWType::READ, RWType::WRITE] {
        for dim in [0, 1] {
            let mut source = StoreLoad::default();
            let mut target = StoreLoad::default();
            source.loads[dim] = 7.1 * mib;
            target.loads[dim] = 5.0 * mib;
            source.hot_peer_count = 8.0;
            let mut src = StoreSummaryInfo::default();
            let mut dst = StoreSummaryInfo::default();
            let mut origin = Influence {
                loads: vec![0.0; 8],
                hot_peer_count: 1.0,
            };
            origin.loads[rw.region_stats()[dim].as_i32() as usize] = 0.5 * mib;
            for (weight, total) in [(1.0, 0.5), (1.0, 1.0), (-1.0, 0.5)] {
                src.add_influence(Some(&origin), -weight);
                dst.add_influence(Some(&origin), weight);
                let s = source.to_load_pred(rw, src.pending_sum.as_ref());
                let t = target.to_load_pred(rw, dst.pending_sum.as_ref());
                assert!((s.min().loads[dim] - (7.1 - total) * mib).abs() < 1e-8);
                assert_eq!(s.max().loads[dim], source.loads[dim]);
                assert_eq!(t.min().loads[dim], target.loads[dim]);
                assert_eq!(t.max().loads[dim], (5.0 + total) * mib);
                assert_eq!(s.pending().loads[dim], total * mib);
                assert_eq!(s.diff().hot_peer_count, total * 2.0);
            }
        }
    }
}
#[test]
fn state_task_13_load_values_ieee_extrema() {
    for (a, b, mn, mx) in [
        (0.0, -0.0, -0.0_f64, 0.0_f64),
        (-0.0, 0.0, -0.0, 0.0),
        (f64::NEG_INFINITY, f64::NAN, f64::NEG_INFINITY, f64::NAN),
        (f64::INFINITY, f64::NAN, f64::NAN, f64::INFINITY),
        (1.0, f64::NAN, f64::NAN, f64::NAN),
        (2.0, 2.0, 2.0, 2.0),
    ] {
        for (a, b) in [(a, b), (b, a)] {
            let x = StoreLoad {
                loads: [a; 4],
                hot_peer_count: a,
                ..Default::default()
            };
            let y = StoreLoad {
                loads: [b; 4],
                hot_peer_count: b,
                ..Default::default()
            };
            for (v, expected) in [(min_load(&x, &y), mn), (max_load(&x, &y), mx)] {
                for actual in v.loads.into_iter().chain([v.hot_peer_count]) {
                    if expected.is_nan() {
                        assert!(actual.is_nan());
                    } else {
                        assert_eq!(actual.to_bits(), expected.to_bits());
                    }
                }
            }
        }
    }
}
#[test]
fn state_task_13_load_values_slice_contracts() {
    let mut sum = StoreSummaryInfo::default();
    sum.add_influence(Some(&Influence::default()), 1.0);
    assert!(sum.pending_sum.is_some());
    let mut sum = StoreSummaryInfo::default();
    sum.add_influence(
        Some(&Influence {
            loads: vec![1.0],
            hot_peer_count: 0.0,
        }),
        1.0,
    );
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sum.add_influence(
            Some(&Influence {
                loads: vec![1.0, 2.0],
                hot_peer_count: 0.0
            }),
            1.0
        )))
        .is_err()
    );
    let short = Influence {
        loads: vec![],
        hot_peer_count: 2.0,
    };
    assert_eq!(
        current()
            .to_load_pred(RWType::from_i32(-1), Some(&short))
            .future
            .hot_peer_count,
        7.0
    );
    assert!(
        std::panic::catch_unwind(|| current().to_load_pred(RWType::READ, Some(&short))).is_err()
    );
}
