// Copyright 2026 AsterSQL.
use super::{HistoryBuffer, HistoryError, HistoryIndex as I, normalize_history_buffer_capacity};
use astersql_pd_core::{RegionInfo, RegionMeta};
use std::sync::Arc;

fn region(id: u64) -> Arc<RegionInfo> {
    Arc::new(RegionInfo::new(
        RegionMeta {
            id,
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    ))
}
fn ids(records: Vec<Arc<RegionInfo>>) -> Vec<u64> {
    records.iter().map(|r| r.meta().id).collect()
}

#[test]
fn state_task_30_history_buffer_normalization() {
    for (size, unit, expected) in [
        (1, 10000, 10000),
        (10000, 10000, 10000),
        (10001, 10000, 20000),
        (30000, 10000, 40000),
        (3, 1, 4),
        (0, 0, 10000),
        (5, 3, 6),
    ] {
        assert_eq!(
            normalize_history_buffer_capacity(size, unit).unwrap(),
            expected
        );
    }
    assert_eq!(HistoryBuffer::new(1).unwrap().capacity(), 10000);
    assert!(matches!(
        normalize_history_buffer_capacity(usize::MAX, 1),
        Err(HistoryError::CapacityOverflow)
    ));
}

#[test]
fn state_task_30_history_buffer_go_snapshot() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/rust-compat/fixtures/state/task-30.json"
    ))
    .unwrap();
    let mut h = HistoryBuffer::with_capacity_unit(2, 1).unwrap();
    h.reset_with_index(I(f["reset"].as_u64().unwrap()));
    for id in f["input"].as_array().unwrap() {
        h.record(region(id.as_u64().unwrap())).unwrap();
    }
    assert_eq!(h.next_index(), I(f["next"].as_u64().unwrap()));
    assert_eq!(h.first_index(), I(f["first"].as_u64().unwrap()));
    assert_eq!(h.capacity(), f["capacity"].as_u64().unwrap() as usize);
    assert!(h.records_from(I(100)).is_empty());
    assert_eq!(
        ids(h.records_from(I(101))),
        f["records"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_u64().unwrap())
            .collect::<Vec<_>>()
    );
}

#[test]
fn state_task_30_history_buffer_empty_and_reset() {
    let mut h = HistoryBuffer::with_capacity_unit(3, 1).unwrap();
    assert!(h.is_empty());
    assert_eq!((h.first_index(), h.next_index()), (I(0), I(0)));
    assert!(h.get(I(0)).is_none());
    assert!(h.records_from(I(0)).is_empty());
    h.reset_with_index(I(100));
    assert!(h.records_from(I(99)).is_empty());
    assert!(h.records_between(I(100), I(200)).is_empty());
    assert_eq!(h.record(region(9)).unwrap(), I(100));
    assert_eq!(h.record(region(8)).unwrap(), I(101));
    let snapshot = h.records_from(I(100));
    h.reset_with_index(I(2));
    assert!(h.is_empty());
    assert_eq!((h.first_index(), h.next_index()), (I(2), I(2)));
    assert!(h.get(I(100)).is_none());
    assert_eq!(h.capacity(), 4);
    h.record(region(7)).unwrap();
    assert_eq!(ids(h.records_from(I(2))), vec![7]);
    assert_eq!(ids(snapshot), vec![9, 8]);
}

#[test]
fn state_task_30_history_buffer_wrapped_intervals() {
    let mut h = HistoryBuffer::with_capacity_unit(4, 1).unwrap();
    h.reset_with_index(I(10));
    for id in 1..=11 {
        h.record(region(id)).unwrap();
    }
    assert_eq!(h.len(), 4);
    assert_eq!((h.first_index(), h.next_index()), (I(17), I(21)));
    assert_eq!(ids(h.records_from(I(17))), vec![8, 9, 10, 11]);
    assert_eq!(ids(h.records_between(I(18), I(20))), vec![9, 10]);
    assert_eq!(ids(h.records_between(I(19), I(99))), vec![10, 11]);
    for (start, end) in [(16, 20), (21, 22), (22, 99), (18, 18), (20, 18), (0, 0)] {
        assert!(
            h.records_between(I(start), I(end)).is_empty(),
            "{start}..{end}"
        );
    }
    assert_eq!(h.get(I(17)).unwrap().meta().id, 8);
    assert_eq!(h.get(I(20)).unwrap().meta().id, 11);
    for index in [16, 21, 22] {
        assert!(h.get(I(index)).is_none());
    }
}

#[test]
fn state_task_30_history_buffer_101_records() {
    // Existing Go TestHistoryBufferRecordsAndFlushes uses these exact inputs.
    for capacity in [1, 2, 100] {
        let mut h = HistoryBuffer::with_capacity_unit(capacity, capacity).unwrap();
        for id in 0..101 {
            assert_eq!(h.record(region(id)).unwrap(), I(id));
        }
        assert_eq!(h.len(), capacity);
        assert_eq!(h.next_index(), I(101));
        assert_eq!(h.first_index(), I(101 - capacity as u64));
        assert_eq!(h.get(I(100)).unwrap().meta().id, 100);
        assert!(h.get(I(100 - capacity as u64)).is_none());
        assert_eq!(
            ids(h.records_from(h.first_index())),
            ((101 - capacity as u64)..101).collect::<Vec<_>>()
        );
    }
}

#[test]
fn state_task_30_history_buffer_index_exhaustion() {
    let mut h = HistoryBuffer::with_capacity_unit(1, 1).unwrap();
    h.reset_with_index(I(u64::MAX - 1));
    h.record(region(1)).unwrap();
    assert!(matches!(
        h.record(region(2)),
        Err(HistoryError::IndexExhausted)
    ));
    assert_eq!(h.next_index(), I(u64::MAX));
    assert_eq!(h.first_index(), I(u64::MAX - 1));
    assert_eq!(ids(h.records_from(h.first_index())), vec![1]);
    h.reset_with_index(I(0));
    assert_eq!(h.record(region(3)).unwrap(), I(0));
}

#[test]
fn state_task_30_history_buffer_releases_overwritten_and_reset_records() {
    let mut h = HistoryBuffer::with_capacity_unit(1, 1).unwrap();
    let first = region(1);
    let weak = Arc::downgrade(&first);
    h.record(first).unwrap();
    let snapshot = h.records_from(I(0));
    h.record(region(2)).unwrap();
    assert!(weak.upgrade().is_some());
    drop(snapshot);
    assert!(weak.upgrade().is_none());
    let weak = Arc::downgrade(h.get(I(1)).unwrap());
    h.reset_with_index(I(5));
    assert!(weak.upgrade().is_none());
}
