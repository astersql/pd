// Copyright 2026 AsterSQL.
use crate::*;
use astersql_pd_core::{StoreInfo, StoreLabel, StoreMeta};
use astersql_pd_core_constant::{Direction, ResourceKind};
use astersql_pd_statistics_utils::{RWType, StoreLoadKind as S};
use std::{collections::HashMap, time::Duration};
const LEADER: ResourceKind = ResourceKind::LEADER;
const REGION: ResourceKind = ResourceKind::REGION;
fn store(id: u64, engine: &str) -> StoreInfo {
    StoreInfo::new(StoreMeta {
        id,
        labels: vec![StoreLabel {
            key: "engine".into(),
            value: engine.into(),
        }],
        ..Default::default()
    })
}
fn loads(values: &[(S, f64)]) -> StoreKindLoads {
    let mut l = [0.; 12];
    for (k, v) in values {
        l[k.as_i32() as usize] = *v;
    }
    l
}
fn peer(leader: bool, l: Loads) -> HotPeerStat {
    HotPeerStat {
        is_leader: leader,
        loads: l.to_vec(),
        ..Default::default()
    }
}
#[test]
fn state_task_24_engine_load_summary_store_infos() {
    let infos = summary_store_infos(&[store(1, ""), store(3, "")]);
    let l = [1, 3]
        .into_iter()
        .map(|id| {
            (
                id,
                loads(&[
                    (S::READ_BYTES, id as f64),
                    (S::READ_KEYS, 2. * id as f64),
                    (S::READ_QUERY, 5. * id as f64),
                ]),
            )
        })
        .collect();
    let mut h = StoreHistoryLoads::default();
    let d = summary_stores_load(
        &infos,
        &l,
        Some(&mut h),
        &HashMap::new(),
        false,
        RWType::READ,
        LEADER,
        Duration::ZERO,
    );
    assert_eq!(d.len(), 2);
    assert!(
        d[&1]
            .load_pred
            .current
            .history_loads
            .iter()
            .all(Vec::is_empty)
    );
    for t in 1..10 {
        let d = summary_stores_load(
            &infos,
            &l,
            Some(&mut h),
            &HashMap::new(),
            false,
            RWType::READ,
            LEADER,
            Duration::from_secs(t),
        );
        for id in [1, 3] {
            let p = &d[&id].load_pred;
            assert_eq!(p.current.loads, [1., 2., 5., 0.].map(|x| x * id as f64));
            for dim in 0..4 {
                assert_eq!(p.current.history_loads[dim][0], p.current.loads[dim]);
                assert_eq!(p.expect.history_loads[dim][0], [2., 4., 10., 0.][dim]);
            }
        }
    }
}
#[test]
fn state_task_24_engine_load_zero_cpu_stddev() {
    let infos = summary_store_infos(&[store(1, ""), store(2, "")]);
    let l = [1, 2]
        .into_iter()
        .map(|id| {
            (
                id,
                loads(&[
                    (S::READ_BYTES, 100. * id as f64),
                    (S::READ_KEYS, 10. * id as f64),
                    (S::READ_QUERY, 5. * id as f64),
                ]),
            )
        })
        .collect();
    let peers = [1, 2]
        .into_iter()
        .map(|id| (id, vec![peer(true, [1., 1., 1., 0.])]))
        .collect();
    let d = summary_stores_load(
        &infos,
        &l,
        None,
        &peers,
        false,
        RWType::READ,
        LEADER,
        Duration::ZERO,
    );
    assert_eq!(d.len(), 2);
    for p in d.values() {
        assert_eq!(p.load_pred.expect.loads, [150., 15., 7.5, 0.]);
        assert_eq!(p.load_pred.stddev.loads, [1. / 3., 1. / 3., 1. / 3., 0.]);
        assert!(p.is_uniform(3, 0.1));
    }
}
#[test]
fn state_task_24_engine_load_tiflash_compute_excluded() {
    let infos = summary_store_infos(
        &(1..=5)
            .map(|id| {
                store(
                    id,
                    if id <= 3 {
                        "tiflash"
                    } else {
                        "tiflash_compute"
                    },
                )
            })
            .collect::<Vec<_>>(),
    );
    let l = (1..=5)
        .map(|id| {
            (
                id,
                loads(&[
                    (
                        S::REGIONS_WRITE_BYTES,
                        if id <= 3 { 5. * 1048576. } else { 0. },
                    ),
                    (S::REGIONS_WRITE_KEYS, if id <= 3 { 1000. } else { 0. }),
                ]),
            )
        })
        .collect();
    let d = summary_stores_load(
        &infos,
        &l,
        None,
        &HashMap::new(),
        true,
        RWType::WRITE,
        REGION,
        Duration::ZERO,
    );
    assert_eq!(d.keys().copied().collect::<Vec<_>>(), vec![1, 2, 3]);
    for p in d.values() {
        assert_eq!(p.load_pred.expect.loads, [5. * 1048576., 1000., 0., 0.]);
        assert_eq!(p.load_pred.stddev.loads, [0.; 4]);
    }
}
#[test]
fn state_task_24_engine_load_tikv_not_affected_by_compute() {
    let infos = summary_store_infos(
        &(1..=5)
            .map(|id| store(id, if id <= 3 { "" } else { "tiflash_compute" }))
            .collect::<Vec<_>>(),
    );
    let l = (1..=5)
        .map(|id| {
            (
                id,
                loads(&[
                    (S::WRITE_BYTES, if id <= 3 { 10. * 1048576. } else { 0. }),
                    (S::WRITE_KEYS, 2000.),
                    (S::WRITE_QUERY, 100.),
                ]),
            )
        })
        .collect();
    let d = summary_stores_load(
        &infos,
        &l,
        None,
        &HashMap::new(),
        true,
        RWType::WRITE,
        REGION,
        Duration::ZERO,
    );
    assert_eq!(d.keys().copied().collect::<Vec<_>>(), vec![1, 2, 3]);
    for p in d.values() {
        assert_eq!(p.load_pred.expect.loads, [10. * 1048576., 2000., 0., 0.]);
    }
}

#[test]
fn state_task_24_engine_load_collectors_and_pending() {
    let mut infos = summary_store_infos(&[
        store(1, ""),
        store(2, "tiflash"),
        store(3, "tiflash_compute"),
        store(4, ""),
    ]);
    infos.get_mut(&1).unwrap().pending_sum = Some(Influence {
        loads: vec![1., 2., 3., 4., 5., 6., 7., 8.],
        hot_peer_count: 2.,
    });
    let l = (1..=3)
        .map(|id| {
            (
                id,
                loads(&[
                    (S::READ_BYTES, 100.),
                    (S::READ_KEYS, 10.),
                    (S::READ_QUERY, 5.),
                    (S::READ_CPU, 7.),
                    (S::WRITE_BYTES, 200.),
                    (S::WRITE_KEYS, 20.),
                    (S::WRITE_QUERY, 8.),
                    (S::REGIONS_WRITE_BYTES, 300.),
                    (S::REGIONS_WRITE_KEYS, 30.),
                ]),
            )
        })
        .collect();
    let peers: HashMap<_, _> = [1, 2, 3]
        .into_iter()
        .map(|id| {
            (
                id,
                vec![
                    peer(true, [11.4, 12.6, 13., 14.]),
                    peer(false, [21., 22., 23., 24.]),
                ],
            )
        })
        .collect();
    for (rw, kind, trace, expected) in [
        (RWType::READ, LEADER, false, [100., 10., 5., 7.]),
        (RWType::READ, REGION, false, [100., 10., 5., 7.]),
        (RWType::WRITE, LEADER, false, [11., 13., 8., 0.]),
        (RWType::WRITE, REGION, false, [200., 20., 0., 0.]),
    ] {
        let mut d = summary_stores_load(&infos, &l, None, &peers, trace, rw, kind, Duration::ZERO);
        let p = &d[&1].load_pred;
        assert_eq!(p.current.loads, expected);
        let delta = if rw == RWType::READ {
            [1., 2., 3., 7.]
        } else {
            [4., 5., 6., 8.]
        };
        assert_eq!(
            p.future.loads,
            std::array::from_fn(|i| expected[i] + delta[i])
        );
        assert_eq!(p.expect.loads, expected); // TiFlash population is separate
        assert_eq!(p.future.hot_peer_count, p.current.hot_peer_count + 2.);
        assert_eq!(d[&1].hot_peers.len(), if kind == LEADER { 1 } else { 2 });
        assert!(!d.contains_key(&3));
        assert!(!d.contains_key(&4));
        if kind == LEADER {
            assert!(!d.contains_key(&2));
        } else {
            assert_eq!(
                d[&2].load_pred.current.loads,
                if rw == RWType::READ {
                    [0.; 4]
                } else {
                    [32., 35., 0., 0.]
                }
            );
        }
        d.get_mut(&1).unwrap().hot_peers[0].loads[0] = 999.;
        assert_eq!(peers[&1][0].loads[0], 11.4);
    }
    let d = summary_stores_load(
        &infos,
        &l,
        None,
        &peers,
        true,
        RWType::WRITE,
        REGION,
        Duration::ZERO,
    );
    assert_eq!(d[&2].load_pred.current.loads, [300., 30., 0., 0.]);
    assert!(
        summary_stores_load(
            &infos,
            &l,
            None,
            &peers,
            true,
            RWType::READ,
            ResourceKind::from_i32(99),
            Duration::ZERO
        )
        .is_empty()
    );
}

#[test]
fn state_task_24_engine_load_transfer_filter_and_removed_history() {
    use astersql_pd_core::StoreState;
    let mut a = store(1, "");
    let mut b = store(2, "");
    a.pause_leader_transfer(Direction::IN);
    b.pause_leader_transfer(Direction::OUT);
    let mut removed = store(3, "");
    removed.set_store_state(StoreState::Tombstone, None);
    let infos = summary_store_infos(&[a, b, removed]);
    let l = (1..=3)
        .map(|id| (id, loads(&[(S::READ_BYTES, 100.)])))
        .collect();
    let mut h = StoreHistoryLoads::default();
    h.gc(infos.values().map(|i| &i.store_info));
    let d = summary_stores_load(
        &infos,
        &l,
        Some(&mut h),
        &HashMap::new(),
        false,
        RWType::READ,
        LEADER,
        Duration::ZERO,
    );
    assert_eq!(d.keys().copied().collect::<Vec<_>>(), vec![3]);
    let d = summary_stores_load(
        &infos,
        &l,
        Some(&mut h),
        &HashMap::new(),
        false,
        RWType::READ,
        REGION,
        Duration::ZERO,
    );
    assert_eq!(d.len(), 3);
    assert_eq!(h.get(1, RWType::READ, REGION)[0][0], 100.);
    assert!(h.get(3, RWType::READ, REGION).iter().all(Vec::is_empty));
}

#[test]
fn state_task_24_engine_load_summary_ragged_history_and_no_hot_peers() {
    let empty = StoreLoadSummary::default();
    assert_eq!(
        empty.result(&[]),
        (StoreLoad::default(), StoreLoad::default())
    );
    for count in [0., 1.] {
        let currents = [
            StoreLoad {
                loads: [10., -10., 0., 2.],
                hot_peer_count: count,
                history_loads: [vec![2., 4.], vec![6.], vec![], vec![]],
            },
            StoreLoad {
                loads: [30., -30., 0., 6.],
                hot_peer_count: count * 3.,
                history_loads: [vec![6.], vec![], vec![], vec![]],
            },
        ];
        let mut summary = StoreLoadSummary::default();
        let mut details = Vec::new();
        for current in currents {
            summary.add(&current);
            details.push(StoreLoadDetail {
                store_summary_info: StoreSummaryInfo::default(),
                load_pred: current.to_load_pred(RWType::READ, None),
                hot_peers: vec![],
            });
        }
        let (expect, stddev) = summary.result(&details);
        assert_eq!(expect.loads, [20., -20., 0., 4.]);
        assert_eq!(
            expect.history_loads,
            [vec![4., 2.], vec![3.], vec![], vec![]]
        );
        assert_eq!(
            stddev.loads,
            if count == 0. {
                [0.; 4]
            } else {
                [0.5, -0.5, 0., 0.5]
            }
        );
        assert_eq!(stddev.hot_peer_count, count * 2.);
        assert!(stddev.history_loads.iter().all(Vec::is_empty));
    }
}
