// Copyright 2026 AsterSQL.
use crate::{DEFAULT_HISTORY_SAMPLE_DURATION, StoreHistoryLoads};
use astersql_pd_core::{NodeState, StoreInfo, StoreMeta, StoreState};
use astersql_pd_core_constant::ResourceKind as K;
use astersql_pd_statistics_utils::RWType as R;
use std::time::Duration;
fn t(n: u64) -> Duration {
    Duration::from_secs(n)
}
fn store(id: u64) -> StoreInfo {
    StoreInfo::new(StoreMeta {
        id,
        node_state: NodeState::Serving,
        ..Default::default()
    })
}

#[test]
fn state_task_12_store_history_go_loads() {
    let mut h = StoreHistoryLoads::new(DEFAULT_HISTORY_SAMPLE_DURATION, t(0));
    let point = [1., 2., 3., 0.];
    h.add(1, R::READ, K::LEADER, point, t(0));
    assert_eq!(h.get(1, R::READ, K::LEADER)[0].len(), 10);
    for _ in 0..10 {
        h.add(1, R::READ, K::LEADER, point, t(0));
    }
    assert_eq!(h.get(1, R::READ, K::LEADER), point.map(|v| vec![v; 10]));
    for (duration, interval, size) in [(1, 1, 1), (1, 1000, 0), (0, 1000, 0), (0, 0, 0)] {
        let mut h = StoreHistoryLoads::new(
            Duration::from_millis(duration),
            Duration::from_millis(interval),
        );
        h.add(1, R::READ, K::LEADER, point, t(0));
        assert_eq!(h.get(1, R::READ, K::LEADER)[0].len(), size);
    }
}
#[test]
fn state_task_12_store_history_interval_wrap_and_snapshot() {
    let mut h = StoreHistoryLoads::new(t(7), t(3));
    h.add(1, R::READ, K::REGION, [1.; 4], t(10));
    assert_eq!(
        h.get(1, R::READ, K::REGION),
        [vec![1., 0.], vec![1., 0.], vec![1., 0.], vec![1., 0.]]
    );
    h.add(1, R::READ, K::REGION, [99.; 4], t(12));
    assert_eq!(h.get(1, R::READ, K::REGION)[0], vec![1., 0.]);
    h.add(1, R::READ, K::REGION, [99.; 4], t(9));
    assert_eq!(h.get(1, R::READ, K::REGION)[0], vec![1., 0.]);
    h.add(1, R::READ, K::REGION, [2.; 4], t(13));
    assert_eq!(h.get(1, R::READ, K::REGION)[0], vec![1., 2.]);
    h.add(1, R::READ, K::REGION, [3., 4., 5., 6.], t(16));
    let expected = [vec![3., 2.], vec![4., 2.], vec![5., 2.], vec![6., 2.]];
    assert_eq!(h.get(1, R::READ, K::REGION), expected);
    let mut snapshot = h.get(1, R::READ, K::REGION);
    snapshot[0][0] = 999.;
    assert_eq!(h.get(1, R::READ, K::REGION), expected);
}
#[test]
fn state_task_12_store_history_isolation_and_config() {
    let mut h = StoreHistoryLoads::new(t(2), t(1));
    for id in 1..=2 {
        for rw in [R::WRITE, R::READ] {
            for kind in [K::LEADER, K::REGION, K::WITNESS] {
                let v = (100 * id + 10 * rw.as_i32() as u64 + kind.as_i32() as u64) as f64;
                h.add(id, rw, kind, [v; 4], t(0));
            }
        }
    }
    for id in 1..=2 {
        for rw in [R::WRITE, R::READ] {
            for kind in [K::LEADER, K::REGION, K::WITNESS] {
                let v = (100 * id + 10 * rw.as_i32() as u64 + kind.as_i32() as u64) as f64;
                assert_eq!(h.get(id, rw, kind)[3], vec![v, 0.]);
            }
        }
    }
    assert!(!h.update_config(t(2), t(1)));
    assert_eq!(h.get(1, R::WRITE, K::LEADER)[0], vec![100., 0.]);
    assert!(h.update_config(t(2), t(0)));
    assert!(h.get(1, R::WRITE, K::LEADER)[0].is_empty());
    h.add(1, R::READ, K::LEADER, [7.; 4], t(0));
    assert_eq!(h.get(1, R::READ, K::LEADER)[0].len(), 10);
    assert!(h.update_config(t(0), t(0)));
    assert!(h.get(1, R::READ, K::LEADER)[0].is_empty());
    h.add(1, R::READ, K::LEADER, [7.; 4], t(0));
    assert!(h.get(1, R::READ, K::LEADER)[0].is_empty());
}
#[test]
fn state_task_12_store_history_gc_does_not_readd_removed() {
    let mut h = StoreHistoryLoads::new(t(60), t(0));
    let mut removed = store(1);
    h.add_store(&removed, R::READ, K::REGION, [1., 0., 0., 0.], t(0));
    assert!(!h.get(1, R::READ, K::REGION)[0].is_empty());
    removed.set_store_state(StoreState::Tombstone, None);
    h.gc([&removed]);
    assert!(h.get(1, R::READ, K::REGION)[0].is_empty());
    h.add_store(&removed, R::READ, K::REGION, [1.; 4], t(1));
    assert!(h.get(1, R::READ, K::REGION)[0].is_empty());
    let mut removing = store(2);
    removing.set_store_state(StoreState::Offline, Some(true));
    for id in 1..=3 {
        for rw in [R::WRITE, R::READ] {
            for kind in [K::LEADER, K::REGION, K::WITNESS] {
                h.add(id, rw, kind, [id as f64; 4], t(2));
            }
        }
    }
    h.gc([&removed, &removing]);
    h.gc([&removed, &removing]);
    for rw in [R::WRITE, R::READ] {
        for kind in [K::LEADER, K::REGION, K::WITNESS] {
            assert!(h.get(1, rw, kind)[0].is_empty());
            assert_eq!(h.get(2, rw, kind)[0][0], 2.);
            assert!(h.get(3, rw, kind)[0].is_empty());
        }
    }
    removed.set_store_state(StoreState::Up, None);
    h.add_store(&removed, R::READ, K::REGION, [9.; 4], t(3));
    assert_eq!(
        h.get(1, R::READ, K::REGION)[0],
        vec![9., 0., 0., 0., 0., 0., 0., 0., 0., 0.]
    );
    h.gc(std::iter::empty());
    assert!(h.get(2, R::READ, K::REGION)[0].is_empty());
}
