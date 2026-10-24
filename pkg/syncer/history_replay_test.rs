// Copyright 2026 AsterSQL.
use super::{HistoryBuffer, HistoryIndex as I, HistoryReplay, SyncFrame};
use astersql_pd_core::{RegionInfo, RegionMeta};
use std::{num::NonZeroUsize, sync::Arc};

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
fn history(start: u64, count: u64, capacity: usize) -> HistoryBuffer {
    let mut h = HistoryBuffer::with_capacity_unit(capacity, 1).unwrap();
    h.reset_with_index(I(start));
    for id in 0..count {
        h.record(region(id)).unwrap();
    }
    h
}
fn frames(h: &mut HistoryBuffer, start: u64, end: u64, batch: usize) -> Vec<SyncFrame> {
    match h
        .plan_replay(I(start), I(end), NonZeroUsize::new(batch).unwrap())
        .unwrap()
    {
        HistoryReplay::Incremental(frames) => frames,
        other => panic!("expected incremental replay, got {other:?}"),
    }
}
fn full(h: &mut HistoryBuffer, start: u64, end: u64) {
    assert!(
        matches!(h.plan_replay(I(start), I(end), NonZeroUsize::new(1000).unwrap()).unwrap(),
        HistoryReplay::FullSync { sync_start_index } if sync_start_index == I(end))
    );
}

#[test]
fn state_task_33_history_replay_split_batches() {
    let mut h = history(10, 1001, 2048);
    let f = frames(&mut h, 10, 1011, 1000);
    assert_eq!(f.len(), 2);
    assert_eq!(
        (f[0].start_index(), f[0].end_index(), f[0].records().len()),
        (I(10), I(1010), 1000)
    );
    assert_eq!(
        (f[1].start_index(), f[1].end_index(), f[1].records().len()),
        (I(1010), I(1011), 1)
    );
    assert_eq!(
        f.iter()
            .flat_map(|f| f.records())
            .map(|r| r.meta().id)
            .collect::<Vec<_>>(),
        (0..1001).collect::<Vec<_>>()
    );
}

#[test]
fn state_task_33_history_replay_stops_at_sync_start_index() {
    let mut h = history(10, 0, 4);
    let first = region(1);
    h.record(first.clone()).unwrap();
    let end = h.next_index();
    h.record(region(2)).unwrap();
    let f = frames(&mut h, 10, end.0, 1000);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].start_index(), I(10));
    assert_eq!(f[0].records().len(), 1);
    assert!(Arc::ptr_eq(&first, &f[0].records()[0]));
    assert_eq!(h.next_index(), I(12));
}

#[test]
fn state_task_33_history_replay_falls_back_when_history_missing() {
    let mut h = history(100, 0, 2);
    full(&mut h, 1, 100);
    let mut h = history(10, 5, 2);
    full(&mut h, 12, 15); // overwritten start
    full(&mut h, 14, 16); // partial suffix is insufficient
    full(&mut h, 0, 15);
    full(&mut h, 16, 15);
}

#[test]
fn state_task_33_history_replay_equal_and_keepalive_do_not_skip() {
    let mut h = history(10, 3, 4);
    let f = frames(&mut h, 11, 11, 1);
    assert_eq!(f.len(), 1);
    assert_eq!(
        (f[0].start_index(), f[0].end_index(), f[0].records().len()),
        (I(11), I(11), 0)
    );
    let keepalive = SyncFrame::keepalive(I(11));
    assert_eq!(keepalive.end_index(), I(11));
    assert!(keepalive.records().is_empty());
    assert_eq!(h.next_index(), I(13));
    full(&mut h, 0, 0); // initial zero is always full sync
}

#[test]
fn state_task_33_history_replay_custom_batch_and_snapshot_survive_overwrite() {
    let mut h = history(10, 5, 8);
    let f = frames(&mut h, 11, 15, 3);
    assert_eq!(
        f.iter()
            .map(|f| (f.start_index().0, f.records().len()))
            .collect::<Vec<_>>(),
        vec![(11, 3), (14, 1)]
    );
    h.reset_with_index(I(80));
    assert_eq!(
        f.iter()
            .flat_map(|f| f.records())
            .map(|r| r.meta().id)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
    let mut h = history(u64::MAX - 2, 2, 4);
    assert_eq!(
        frames(&mut h, u64::MAX - 2, u64::MAX, 1)[1].end_index(),
        I(u64::MAX)
    );
}

#[test]
fn state_task_33_history_replay_caps_batch_and_observes_window() {
    let mut h = history(10, 1001, 2048);
    assert_eq!(
        frames(&mut h, 10, 1011, usize::MAX)[0].records().len(),
        1000
    );
    let mut h = HistoryBuffer::with_config(2, 8, 1).unwrap();
    h.reset_with_index(I(10));
    h.record(region(1)).unwrap();
    h.record(region(2)).unwrap();
    frames(&mut h, 10, 12, 1);
    assert_eq!(h.capacity(), 4);
    full(&mut h, 0, 12);
    assert_eq!(h.capacity(), 4);
}
