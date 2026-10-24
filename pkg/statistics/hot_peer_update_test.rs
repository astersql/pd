// Copyright 2026 AsterSQL.
use super::HotPeerCache;
use astersql_pd_core::{Peer, RegionInfo, RegionMeta};
use astersql_pd_statistics_utils::{ActionType, RWType};

fn region() -> RegionInfo {
    let peers: Vec<_> = (1..=3)
        .map(|id| Peer {
            id,
            store_id: id,
            ..Default::default()
        })
        .collect();
    RegionInfo::new(
        RegionMeta {
            id: 2,
            peers: peers.clone(),
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![],
    )
}
fn delta(kind: RWType, value: f64) -> [f64; 8] {
    let mut d = [0.; 8];
    for k in kind.region_stats() {
        d[k.as_i32() as usize] = value;
    }
    d
}
fn step(
    cache: &mut HotPeerCache,
    kind: RWType,
    seconds: u64,
    value: f64,
    threshold: f64,
) -> (i64, i64, ActionType) {
    let mut items =
        cache.check_peer_flow(&region(), Some(&[1]), &delta(kind, value), seconds, |_| {
            [threshold; 4]
        });
    assert_eq!(items.len(), 1);
    let item = items.pop().unwrap();
    let result = (item.hot_degree, item.anti_count, item.action_type);
    cache.update_stat(item);
    result
}
#[test]
fn state_task_19_hot_peer_update_heat_cold_remove() {
    let kind = RWType::READ;
    let mut c = HotPeerCache::new(kind);
    assert!(
        c.check_peer_flow(&region(), None, &[0.; 8], 0, |_| [0.; 4])
            .is_empty()
    );
    assert!(
        c.check_peer_flow(&region(), None, &[0.; 8], 10, |_| [1.; 4])
            .is_empty()
    );
    assert_eq!(step(&mut c, kind, 4, 60., 0.), (0, 0, ActionType::ADD));
    let mut items = c.check_peer_flow(&region(), Some(&[1]), &delta(kind, 60.), 4, |_| [0.; 4]);
    assert_eq!((items[0].hot_degree, items[0].anti_count), (0, 0));
    // Original Go test manually sets anti-count before applying second report.
    items[0].anti_count = 12;
    c.update_stat(items.pop().unwrap());
    assert_eq!(step(&mut c, kind, 4, 60., 0.), (1, 12, ActionType::UPDATE));
    assert_eq!(step(&mut c, kind, 4, 60., 0.), (1, 12, ActionType::UPDATE));
    assert_eq!(step(&mut c, kind, 10, 60., 0.), (2, 12, ActionType::UPDATE));
    for cold in 1..=12 {
        assert_eq!(
            step(&mut c, kind, 10, 60., 10.),
            (
                2 - cold,
                12 - cold,
                if cold == 12 {
                    ActionType::REMOVE
                } else {
                    ActionType::UPDATE
                }
            )
        );
    }
    assert!(c.get(2, 1).is_none());
}
#[test]
fn state_task_19_hot_peer_update_report_intervals_and_explicit_peers() {
    for kind in [RWType::READ, RWType::WRITE] {
        let mut c = HotPeerCache::new(kind);
        for interval in [120, 60, 30] {
            for _ in 0..3 {
                let items = c.check_peer_flow(
                    &region(),
                    None,
                    &delta(kind, interval as f64 * 10000.),
                    interval,
                    |_| [1.; 4],
                );
                assert_eq!(items.len(), 3);
                for item in items {
                    assert!(item.anti_count > 0);
                    c.update_stat(item);
                }
            }
        }
        assert!(
            c.check_peer_flow(&region(), Some(&[]), &[0.; 8], 10, |_| [1.; 4])
                .is_empty()
        );
        let items = c.check_peer_flow(&region(), Some(&[9]), &delta(kind, 1200000.), 120, |_| {
            [1.; 4]
        });
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].store_id, 9);
        assert!(!items[0].is_leader);
        assert_eq!(items[0].stores, vec![1, 2, 3]);
    }
}
#[test]
fn state_task_19_hot_peer_update_recovery_and_cpu() {
    for kind in [RWType::READ, RWType::WRITE] {
        let mut c = HotPeerCache::new(kind);
        let interval = kind.report_interval();
        assert_eq!(
            step(&mut c, kind, interval, 100., 1.),
            (1, kind.default_anti_count() as i64, ActionType::ADD)
        );
        let cold = step(&mut c, kind, interval, 0., 1.);
        assert_eq!(cold.1, kind.default_anti_count() as i64 - 1);
        let mut d = [0.; 8];
        d[kind.region_stats()[3].as_i32() as usize] = interval as f64;
        let items = c.check_peer_flow(&region(), Some(&[1]), &d, interval, |_| [1.; 4]);
        assert_eq!(items[0].hot_degree, 1);
        assert_eq!(items[0].anti_count, kind.default_anti_count() as i64);
        assert!(items[0].allow_inherited);
        assert_eq!(c.get(2, 1).unwrap().interval_sum(), 0); // check does not mutate stored state
        for item in items {
            c.update_stat(item);
        }
    }
}
#[test]
fn state_task_19_hot_peer_update_unstable_data() {
    use astersql_pd_movingaverage::TimeMedian;
    for (input, expected) in [
        (
            [1., 1., 1., 1., 1., 1., 1., 1., 1., 1.],
            [1., 1., 1., 1., 1., 1., 1., 1., 1., 1.],
        ),
        ([0., 0., 0., 0., 0., 1., 0., 0., 0., 0.], [0.; 10]),
        ([0., 0., 0., 0., 0., 1., 1., 0., 0., 0.], [0.; 10]),
        (
            [0., 0., 0., 0., 1., 1., 1., 0., 0., 0.],
            [0., 0., 0., 0., 0., 0., 1., 1., 1., 0.],
        ),
        ([0., 0., 0., 0., 0., 1., 0., 1., 0., 0.], [0.; 10]),
        (
            [0., 0., 0., 0., 0., 1., 0., 1., 0., 1.],
            [0., 0., 0., 0., 0., 0., 0., 0., 0., 1.],
        ),
    ] {
        let mut tm = TimeMedian::new(1, 5, 1_000_000_000);
        let actual = input.map(|v| {
            tm.add(v, 1_000_000_000);
            tm.get()
        });
        assert_eq!(actual, expected);
    }
}

#[test]
fn state_task_19_hot_peer_update_denoising_action_isolation() {
    let r = region();
    let mut read = HotPeerCache::new(RWType::READ);
    let write = HotPeerCache::new(RWType::WRITE);
    assert_eq!(read.minimum_thresholds(), [8192., 128., 128., 1.]);
    assert_eq!(write.minimum_thresholds(), [1024., 32., 32., 1.]);
    for interval in [0, 1, 2] {
        assert!(
            read.check_peer_flow(&r, None, &delta(RWType::READ, 1e6), interval, |_| [1.; 4])
                .is_empty()
        );
    }
    let mut items =
        read.check_peer_flow(&r, Some(&[1]), &delta(RWType::READ, 60.), 4, |_| [10.; 4]);
    read.update_stat(items.pop().unwrap());
    // Dropped report leaves both accumulated seconds and delta unchanged.
    drop(read.check_peer_flow(&r, Some(&[1]), &delta(RWType::READ, 0.), 6, |_| [10.; 4]));
    assert_eq!(read.get(2, 1).unwrap().interval_sum(), 4_000_000_000);
    let mut items =
        read.check_peer_flow(&r, Some(&[1]), &delta(RWType::READ, 60.), 6, |_| [10.; 4]);
    assert_eq!((items[0].hot_degree, items[0].anti_count), (1, 1));
    read.update_stat(items.pop().unwrap());
    assert!(write.get(2, 1).is_none());
    // Hot recovery increments anti-count once per full window, without jumping to 12.
    assert_eq!(
        step(&mut read, RWType::READ, 10, 100., 10.),
        (2, 2, ActionType::UPDATE)
    );
    let mut ignored = read.get(2, 1).unwrap().clone();
    ignored.action_type = ActionType::from_i32(99);
    ignored.hot_degree = 999;
    read.update_stat(ignored);
    assert_eq!(read.get(2, 1).unwrap().hot_degree, 2);
    let mut remove = read.get(2, 1).unwrap().clone();
    remove.action_type = ActionType::REMOVE;
    read.update_stat(remove);
    assert!(read.get(2, 1).is_none());
    read.set_denoising(false);
    assert_eq!(
        step(&mut read, RWType::READ, 1, 100., 1.),
        (0, 0, ActionType::ADD)
    );
    // Partial first window that becomes cold has no anti-count reserve.
    assert_eq!(
        step(&mut read, RWType::READ, 9, 0., 100.),
        (-1, -1, ActionType::REMOVE)
    );
}
