// Copyright 2026 AsterSQL.
use crate::{DimStat, HotPeerCache, HotPeerStat};
use astersql_pd_core::{Peer, RegionInfo, RegionMeta};
use astersql_pd_statistics_utils::{ActionType, RWType};

fn region(id: u64) -> RegionInfo {
    let peer = Peer {
        id,
        store_id: 1,
        ..Default::default()
    };
    RegionInfo::new(
        RegionMeta {
            id,
            peers: vec![peer.clone()],
            ..Default::default()
        },
        Some(peer),
        vec![],
        vec![],
    )
}
fn report(c: &mut HotPeerCache, id: u64, kind: RWType, rates: [f64; 4], seconds: u64, now: i64) {
    let mut delta = [0.; 8];
    for (dim, k) in kind.region_stats().iter().enumerate() {
        delta[k.as_i32() as usize] = rates[dim] * seconds as f64;
    }
    for item in c.check_peer_flow_dynamic(&region(id), None, &delta, seconds, now) {
        c.update_stat(item);
    }
}
fn put(c: &mut HotPeerCache, id: u64, store: u64, loads: [f64; 4], action: ActionType) {
    c.update_stat(HotPeerStat {
        region_id: id,
        store_id: store,
        loads: loads.to_vec(),
        action_type: action,
        ..Default::default()
    });
}

#[test]
fn state_task_20_hot_threshold_report_intervals() {
    for interval in [120, 60, 30, 17, 1] {
        let mut c = HotPeerCache::new(RWType::READ);
        c.set_denoising(false); // Go testMetrics bypasses CheckPeerFlow's short filter.
        c.set_thresholds_update_interval(0);
        let mut now = 0;
        for id in 1..70 {
            // Fill real TimeMedian to a stable denoised sample, including 1s reports.
            for _ in 0..(30 / interval + 10) {
                now += 1;
                report(
                    &mut c,
                    id,
                    RWType::READ,
                    [16384., 0., 0., 0.],
                    interval,
                    now,
                );
            }
            now += 1;
            assert_eq!(
                c.calc_hot_thresholds(1, now)[0],
                if id < 60 { 8192. } else { 13107.2 },
                "interval={interval} id={id}"
            );
            assert!(c.get(id, 1).is_some());
        }
    }
}

#[test]
fn state_task_20_hot_threshold_go_topn_and_refresh() {
    for refresh in [8_000_000_000, 0] {
        let mut c = HotPeerCache::new(RWType::WRITE);
        c.set_thresholds_update_interval(refresh);
        let mut now = 0;
        for id in 0..100 {
            for _ in 0..10 {
                now += 1;
                report(
                    &mut c,
                    id,
                    RWType::WRITE,
                    [id as f64 * 100., id as f64 * 100., id as f64 * 100., 0.],
                    60,
                    now,
                );
            }
            if id < 60 {
                assert_eq!(c.calc_hot_thresholds(1, now)[1], 32.);
            }
        }
        assert!(c.get(0, 1).is_none());
        if refresh > 0 {
            assert_eq!(c.calc_hot_thresholds(1, now)[1], 32.);
        }
        c.set_thresholds_update_interval(0);
        assert_eq!(c.calc_hot_thresholds(1, now + 1), [3200., 3200., 3200., 1.]);
    }
}

#[test]
fn state_task_20_hot_threshold_rank_replacement_remove_and_stores() {
    let mut c = HotPeerCache::new(RWType::WRITE);
    c.set_thresholds_update_interval(0);
    for id in 1..=61 {
        put(
            &mut c,
            id,
            1,
            [
                id as f64 * 2000.,
                (62 - id) as f64 * 100.,
                1.,
                id as f64 * 10.,
            ],
            ActionType::ADD,
        );
    }
    assert_eq!(c.calc_hot_thresholds(1, 1), [3200., 160., 32., 16.]);
    assert_eq!(c.calc_hot_thresholds(2, 1), [1024., 32., 32., 1.]);
    put(&mut c, 1, 1, [200000., 0., 0., 2000.], ActionType::UPDATE);
    assert_eq!(c.calc_hot_thresholds(1, 2), [4800., 80., 32., 24.]);
    put(&mut c, 2, 1, [0.; 4], ActionType::REMOVE);
    assert_eq!(c.calc_hot_thresholds(1, 3), [4800., 32., 32., 24.]);
    put(&mut c, 3, 1, [0.; 4], ActionType::REMOVE);
    assert_eq!(c.calc_hot_thresholds(1, 4), c.minimum_thresholds());
    put(&mut c, 4, 1, [1e9; 4], ActionType::from_i32(99));
    assert_eq!(c.calc_hot_thresholds(1, 5), c.minimum_thresholds());
    // A pending action is not a member of TopN until applied.
    let mut delta = [0.; 8];
    for k in RWType::WRITE.region_stats() {
        delta[k.as_i32() as usize] = 600000.;
    }
    let pending = c.check_peer_flow_dynamic(&region(100), None, &delta, 60, 6);
    assert_eq!(pending.len(), 1);
    assert_eq!(c.calc_hot_thresholds(1, 7), c.minimum_thresholds());
    assert!(c.get(100, 1).is_none());
    for item in pending {
        c.update_stat(item);
    }
    assert!(c.calc_hot_thresholds(1, 8)[0] > c.minimum_thresholds()[0]);
    for id in 1..=60 {
        put(&mut c, id, 2, [100000.; 4], ActionType::ADD);
    }
    assert_eq!(c.calc_hot_thresholds(2, 9), [80000.; 4]);
    assert!(c.calc_hot_thresholds(1, 9)[0] < 80000.);
    let read = HotPeerCache::new(RWType::READ);
    assert_eq!(read.calc_hot_thresholds(1, 0), [8192., 128., 128., 1.]);
}

#[test]
fn state_task_20_hot_threshold_clock_boundary() {
    let mut c = HotPeerCache::new(RWType::WRITE);
    assert_eq!(c.calc_hot_thresholds(1, 100), c.minimum_thresholds());
    for id in 0..60 {
        put(&mut c, id, 1, [10000.; 4], ActionType::ADD);
    }
    assert_eq!(c.calc_hot_thresholds(1, 99), c.minimum_thresholds());
    assert_eq!(
        c.calc_hot_thresholds(1, 8_000_000_100),
        c.minimum_thresholds()
    );
    assert_eq!(c.calc_hot_thresholds(1, 8_000_000_101), [8000.; 4]);
}

#[test]
fn state_task_20_hot_threshold_cpu_last_average_and_dynamic_admission() {
    let mut c = HotPeerCache::new(RWType::READ);
    c.set_thresholds_update_interval(0);
    for id in 0..60 {
        put(&mut c, id, 1, [0., 0., 0., 12.5], ActionType::ADD);
    }
    // GetLoad rounds 12.5 -> 13, so the CPU threshold is 10.4.
    assert_eq!(c.calc_hot_thresholds(1, 1), [8192., 128., 128., 10.4]);
    report(&mut c, 100, RWType::READ, [0., 0., 0., 10.3], 10, 2);
    assert!(c.get(100, 1).is_none());
    report(&mut c, 100, RWType::READ, [0., 0., 0., 10.4], 10, 3);
    assert_eq!(c.get(100, 1).unwrap().hot_degree, 1);
    report(&mut c, 100, RWType::READ, [0., 0., 0., 10.4], 10, 4);
    assert_eq!(c.get(100, 1).unwrap().hot_degree, 2);
    let mut dims: Vec<_> = (0..4).map(|_| Some(DimStat::new(10_000_000_000))).collect();
    for d in dims.iter_mut().flatten() {
        d.add(0., 10_000_000_000);
    }
    dims[3].as_mut().unwrap().add(200., 10_000_000_000);
    let mut stat = HotPeerStat {
        rolling_loads: Some(dims),
        ..Default::default()
    };
    assert!(stat.is_hot(&[10.; 4])); // CPU last average 10; median need not match.
    stat.clear_last_average();
    assert!(!stat.is_hot(&[10.; 4]));
}
