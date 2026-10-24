// Copyright 2026 AsterSQL.
use super::{FullSnapshot, HistoryBuffer, HistoryIndex as I, SnapshotOffset};
use astersql_pd_core::{BasicCluster, KeyRange, RegionInfo, RegionMeta};
use std::{
    num::NonZeroUsize,
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

fn size(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}
fn region(id: u64) -> Arc<RegionInfo> {
    Arc::new(RegionInfo::new(
        RegionMeta {
            id,
            key_range: KeyRange {
                start_key: id.to_be_bytes().to_vec(),
                end_key: (id + 1).to_be_bytes().to_vec(),
            },
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    ))
}
fn history() -> HistoryBuffer {
    let mut h = HistoryBuffer::with_config(2, 8, 1).unwrap();
    h.reset_with_index(I(10));
    h
}
fn cluster(count: u64) -> BasicCluster {
    let mut c = BasicCluster::new();
    for id in 1..=count {
        c.regions.put_region(region(id));
    }
    c
}
#[test]
fn state_task_34_full_snapshot_buffers_live_records() {
    let mut h = history();
    let mut c = cluster(1);
    let original = c.regions.get_region(1).unwrap();
    let scan = FullSnapshot::bind(&mut h, &c);
    c.regions.put_region(region(2));
    let scan = scan
        .send(size(1000), |batch| {
            assert_eq!(batch.offset, SnapshotOffset(0));
            assert_eq!(batch.records.len(), 1);
            assert!(Arc::ptr_eq(&original, &batch.records[0]));
            for id in 100..105 {
                h.record(region(id)).unwrap();
            }
            Ok::<_, ()>(())
        })
        .unwrap();
    assert_eq!(scan.sync_start_index(), I(10));
    for _ in 0..6 {
        h.maybe_shrink().unwrap();
    }
    assert_eq!(h.capacity(), 8);
    let (records, end) = h.retained_records_from(scan.sync_start_index()).unwrap();
    assert_eq!(end, I(15));
    assert_eq!(
        records.iter().map(|r| r.meta().id).collect::<Vec<_>>(),
        (100..105).collect::<Vec<_>>()
    );
    drop(scan);
    for _ in 0..6 {
        h.maybe_shrink().unwrap();
    }
    assert_eq!(h.capacity(), 2);
}
#[test]
fn state_task_34_full_snapshot_offsets_and_empty() {
    let mut h = history();
    let c = cluster(1001);
    let scan = FullSnapshot::bind(&mut h, &c);
    let batches: Vec<_> = scan.batches(size(usize::MAX)).collect();
    assert_eq!(
        batches
            .iter()
            .map(|b| (b.offset.0, b.records.len()))
            .collect::<Vec<_>>(),
        vec![(0, 1000), (1000, 1)]
    );
    let mut ids: Vec<_> = batches
        .iter()
        .flat_map(|b| b.records)
        .map(|r| r.meta().id)
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, (1..=1001).collect::<Vec<_>>());
    assert_eq!(h.next_index(), I(10));
    assert_eq!(scan.sync_start_index(), I(10));
    assert_eq!(
        scan.batches(size(400))
            .map(|b| (b.offset.0, b.records.len()))
            .collect::<Vec<_>>(),
        vec![(0, 400), (400, 400), (800, 201)]
    );
    let empty = FullSnapshot::bind(&mut h, &cluster(0));
    assert_eq!(empty.batches(size(1)).count(), 0);
    let empty = empty
        .send(size(1), |_| -> Result<(), ()> {
            panic!("empty scan sends no frame")
        })
        .unwrap();
    assert_eq!(empty.sync_start_index(), I(10));
    assert_eq!(h.capacity(), 2);
}
#[test]
fn state_task_34_full_snapshot_bind_waits_for_append_boundary() {
    let history = Arc::new(Mutex::new(history()));
    let mut append = history.lock().unwrap();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (bound_tx, bound_rx) = mpsc::channel();
    let shared = history.clone();
    let thread = std::thread::spawn(move || {
        ready_tx.send(()).unwrap();
        let scan = FullSnapshot::bind(&mut shared.lock().unwrap(), &cluster(1));
        bound_tx.send(scan).unwrap();
    });
    ready_rx.recv().unwrap();
    assert!(bound_rx.recv_timeout(Duration::from_millis(20)).is_err());
    for id in 1..=3 {
        append.record(region(id)).unwrap();
    }
    drop(append);
    let scan = bound_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    thread.join().unwrap();
    assert_eq!(scan.sync_start_index(), I(13));
}
#[test]
fn state_task_34_full_snapshot_failure_releases_retain() {
    let mut h = history();
    let scan = FullSnapshot::bind(&mut h, &cluster(1001));
    let mut calls = 0;
    let result = scan.send(size(1000), |batch| {
        calls += 1;
        if calls == 1 {
            assert_eq!(batch.records.len(), 1000);
            for id in 100..105 {
                h.record(region(id)).unwrap();
            }
            Ok(())
        } else {
            Err("unavailable")
        }
    });
    assert!(matches!(result, Err("unavailable")));
    assert_eq!(calls, 2);
    for _ in 0..6 {
        h.maybe_shrink().unwrap();
    }
    assert_eq!(h.capacity(), 2);
}
#[test]
fn state_task_34_full_snapshot_retain_is_bounded_and_unwind_safe() {
    let mut h = HistoryBuffer::with_config(1, 1, 1).unwrap();
    h.reset_with_index(I(100));
    let scan = FullSnapshot::bind(&mut h, &cluster(1));
    h.record(region(2)).unwrap();
    h.record(region(3)).unwrap();
    assert!(h.retained_records_from(scan.sync_start_index()).is_err());
    let mut h = history();
    let scan = FullSnapshot::bind(&mut h, &cluster(1));
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = scan.send(size(1), |_| -> Result<(), ()> {
                for id in 100..105 {
                    h.record(region(id)).unwrap();
                }
                panic!("sender panicked")
            });
        }))
        .is_err()
    );
    for _ in 0..6 {
        h.maybe_shrink().unwrap();
    }
    assert_eq!(h.capacity(), 2);
}
