// Copyright 2026 AsterSQL.
use super::{DimStat, HotPeerStat};
use astersql_pd_statistics_utils::{ActionType, CPU_DIM, DIM_LEN, KEY_DIM, RWType, TopN, TopNItem};
const SECOND: i64 = 1_000_000_000;

#[test]
fn state_task_18_hot_peer_bounds_and_missing_rolling() {
    let mut stat = HotPeerStat {
        loads: vec![1.0, 2.0],
        ..Default::default()
    };
    for (dim, expected) in [
        (0, 1.0),
        (1, 2.0),
        (2, 0.0),
        (-1, 0.0),
        (4, 0.0),
        (isize::MAX, 0.0),
    ] {
        assert_eq!(stat.get_load(dim), expected);
    }
    assert_eq!(stat.get_loads(), vec![1.0, 2.0]);
    stat.loads = vec![3.0, 4.0];
    stat.rolling_loads = Some(vec![None, None, None, None]);
    assert_eq!(stat.get_loads(), vec![3.0, 4.0, 0.0, 0.0]);
    assert!(!stat.is_hot(&[])); // Missing rolling does not index thresholds.
    let mut hot = DimStat::new(SECOND);
    hot.add(10.0, SECOND);
    stat.rolling_loads.as_mut().unwrap()[KEY_DIM] = Some(hot);
    assert!(stat.is_hot(&[5.0; DIM_LEN]));
    assert_eq!(stat.get_load(1), 10.0);
    stat.rolling_loads = Some(vec![]);
    assert_eq!(stat.get_loads(), vec![3.0, 4.0, 0.0, 0.0]);
    stat.rolling_loads = None;
    stat.loads = vec![1.5, -1.5, f64::INFINITY, f64::NAN, 999.0];
    assert_eq!(stat.get_load(0), 2.0);
    assert_eq!(stat.get_load(1), -2.0);
    assert!(stat.get_load(2).is_infinite());
    assert!(stat.get_load(CPU_DIM as isize).is_nan());
    assert_eq!(stat.get_load(4), 0.0);
    assert_eq!(stat.get_loads()[0], 1.5);
}

#[test]
fn state_task_18_hot_peer_cpu_last_average() {
    let mut cpu = DimStat::new(10 * SECOND);
    cpu.add(100.0, 10 * SECOND);
    assert!(cpu.is_last_average_hot(10.0));
    let mut low = DimStat::new(10 * SECOND);
    low.add(0.0, 10 * SECOND);
    let mut stat = HotPeerStat {
        rolling_loads: Some(vec![
            Some(low.clone()),
            Some(low.clone()),
            Some(low.clone()),
            Some(cpu),
        ]),
        ..Default::default()
    };
    assert!(stat.is_hot(&[10.0; DIM_LEN]));
    stat.rolling_loads.as_mut().unwrap()[CPU_DIM] = Some(low);
    assert!(!stat.is_hot(&[10.0; DIM_LEN]));
    // Median stays high while last average becomes cold.
    let cpu = stat.rolling_loads.as_mut().unwrap()[CPU_DIM]
        .as_mut()
        .unwrap();
    for _ in 0..5 {
        cpu.add(1000.0, 10 * SECOND);
    }
    cpu.clear_last_average();
    cpu.add(0.0, 10 * SECOND);
    assert!(cpu.is_hot(10.0));
    assert!(!stat.is_hot(&[10.0; DIM_LEN]));
    stat.clear_last_average();
    assert_eq!(stat.interval_sum(), 0);
    assert!(!stat.is_hot(&[0.0; DIM_LEN])); // 0/0 is NaN, not hot.
}

#[test]
fn state_task_18_hot_peer_window_full_clear_and_clone() {
    let mut dim = DimStat::new(10 * SECOND);
    assert!(!dim.is_last_average_hot(0.0));
    dim.add(60.0, 4 * SECOND);
    assert!(!dim.is_full(10 * SECOND));
    assert_eq!(dim.get(), 0.0);
    dim.add(60.0, 4 * SECOND);
    assert!(!dim.is_full(10 * SECOND));
    dim.add(60.0, 4 * SECOND);
    assert!(dim.is_full(10 * SECOND));
    assert_eq!(dim.get(), 15.0);
    let mut cloned = dim.clone();
    assert!(cloned.is_full(10 * SECOND));
    assert!(!cloned.is_last_average_hot(1.0)); // Go clone resets lastDelta.
    cloned.clear_last_average();
    cloned.add(600.0, 10 * SECOND);
    assert_eq!(cloned.get(), 37.5);
    assert_eq!(dim.get(), 15.0);
    assert!(dim.is_last_average_hot(15.0));
    dim.clear_last_average();
    assert!(!dim.is_full(SECOND));
    assert_eq!(dim.get(), 15.0);
    dim.add(1.0, SECOND / 2);
    dim.add(1.0, SECOND / 2);
    assert!(!dim.is_full(SECOND)); // Each fractional interval truncates separately.
    assert!(dim.is_last_average_hot(f64::INFINITY));
}

#[test]
fn state_task_18_hot_peer_snapshot_and_topn() {
    let mut dim = DimStat::new(SECOND);
    dim.add(2.5, SECOND);
    let mut stat = HotPeerStat {
        store_id: 1,
        region_id: 2,
        hot_degree: 3,
        anti_count: 4,
        loads: vec![99.0, 4.5],
        rolling_loads: Some(vec![Some(dim), None]),
        stores: vec![1, 3],
        action_type: ActionType::UPDATE,
        is_leader: true,
        last_transfer_leader_time: Some(100 * SECOND),
        in_cold: true,
        allow_inherited: true,
    };
    let mut snapshot = stat.clone();
    assert_eq!(snapshot.get_loads(), vec![3.0, 5.0, 0.0, 0.0]);
    assert!(snapshot.rolling_loads.is_none());
    assert_eq!(
        (
            snapshot.store_id,
            snapshot.id(),
            snapshot.hot_degree,
            snapshot.anti_count
        ),
        (1, 2, 3, 4)
    );
    assert_eq!(snapshot.action_type, ActionType::UPDATE);
    assert!(snapshot.is_leader && snapshot.in_cold && snapshot.allow_inherited);
    assert_eq!(snapshot.last_transfer_leader_time, Some(100 * SECOND));
    snapshot.stores.push(9);
    snapshot.loads[0] = 88.0;
    assert_eq!(stat.stores, vec![1, 3]);
    assert_eq!(stat.get_load(0), 3.0);
    stat.rolling_loads.as_mut().unwrap()[0]
        .as_mut()
        .unwrap()
        .add(100.0, SECOND);
    assert_eq!(snapshot.loads[0], 88.0);
    let mut top = TopN::new(DIM_LEN, 1);
    top.put(stat);
    top.put(snapshot);
    assert_eq!(top.len(), 1);
    assert_eq!(top.get(2).unwrap().get_load(0), 88.0);
}

#[test]
fn state_task_18_hot_peer_cooldown_boundaries() {
    let mut stat = HotPeerStat::default();
    assert!(!stat.needs_cooldown_transfer_leader(100 * SECOND, 3, RWType::READ));
    stat.last_transfer_leader_time = Some(100 * SECOND);
    for (rw, seconds) in [(RWType::READ, 30), (RWType::WRITE, 180)] {
        assert!(stat.needs_cooldown_transfer_leader(100 * SECOND, 3, rw));
        assert!(stat.needs_cooldown_transfer_leader((100 + seconds) * SECOND - 1, 3, rw));
        assert!(!stat.needs_cooldown_transfer_leader((100 + seconds) * SECOND, 3, rw));
        assert!(!stat.needs_cooldown_transfer_leader((100 + seconds) * SECOND + 1, 3, rw));
        assert!(stat.needs_cooldown_transfer_leader(99 * SECOND, 3, rw));
        assert!(!stat.needs_cooldown_transfer_leader(100 * SECOND, 0, rw));
    }
}
