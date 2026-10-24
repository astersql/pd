// Copyright 2026 AsterSQL.
use super::{
    CatchUpFrame, FullCatchUpError, FullSnapshot, HistoryBuffer, HistoryError, HistoryIndex as I,
    HistoryReplay,
};
use astersql_pd_core::{BasicCluster, KeyRange, RegionInfo, RegionMeta};
use std::{
    num::NonZeroUsize,
    sync::{Arc, Mutex},
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
fn setup(
    base: usize,
    max: usize,
    start: u64,
    count: u64,
) -> (Mutex<HistoryBuffer>, BasicCluster, FullSnapshot) {
    let mut h = HistoryBuffer::with_config(base, max, 1).unwrap();
    h.reset_with_index(I(start));
    let mut c = BasicCluster::new();
    for id in 1..=count {
        c.regions.put_region(region(id));
    }
    let scan = FullSnapshot::bind(&mut h, &c);
    (Mutex::new(h), c, scan)
}
#[test]
fn state_task_35_full_catchup_keeps_live_records() {
    let (h, mut c, scan) = setup(2, 8, 10, 1);
    let scan = scan.send(size(1000), |_| Ok::<_, ()>(())).unwrap();
    for id in [100, 101] {
        h.lock().unwrap().record(region(id)).unwrap();
    }
    let mut frames = vec![];
    let end = scan
        .catch_up(
            &h,
            size(1000),
            |frame| {
                match frame {
                    CatchUpFrame::Records {
                        start_index,
                        records,
                    } => {
                        frames.push((
                            start_index,
                            records.iter().map(|r| r.meta().id).collect::<Vec<_>>(),
                        ));
                        let live = region(102);
                        c.regions.put_region(live.clone());
                        h.lock().unwrap().record(live).unwrap();
                    }
                    CatchUpFrame::Complete { next_index } => frames.push((next_index.0, vec![])),
                }
                Ok::<_, ()>(())
            },
            || panic!("healthy stream disconnected"),
        )
        .unwrap();
    assert_eq!(frames, vec![(10, vec![100, 101]), (12, vec![])]);
    assert_eq!(end, I(12));
    let mut h = h.lock().unwrap();
    assert_eq!(h.next_index(), I(13));
    let HistoryReplay::Incremental(live) = h.plan_replay(end, I(13), size(1000)).unwrap() else {
        panic!("live record lost")
    };
    assert_eq!(live[0].start_index(), end);
    assert!(Arc::ptr_eq(
        &live[0].records()[0],
        &c.regions.get_region(102).unwrap()
    ));
    assert_eq!(live[0].end_index(), I(13));
}
#[test]
fn state_task_35_full_catchup_overflow_disconnects_and_requires_resync() {
    let (h, mut c, scan) = setup(1, 1, 100, 1);
    let mut full = vec![];
    let scan = scan
        .send(size(1000), |batch| {
            full.extend(batch.records.iter().map(|r| r.meta().id));
            for id in [2, 3] {
                let r = region(id);
                c.regions.put_region(r.clone());
                h.lock().unwrap().record(r).unwrap();
            }
            Ok::<_, ()>(())
        })
        .unwrap();
    let mut connected = true;
    let mut sent = 0;
    let result = scan.catch_up(
        &h,
        size(1000),
        |_| {
            sent += 1;
            Ok::<_, ()>(())
        },
        || connected = false,
    );
    assert!(matches!(
        result,
        Err(FullCatchUpError::ResyncRequired(HistoryError::HistoryGap {
            requested: I(100),
            first: I(101),
            next: I(102)
        }))
    ));
    assert!(!connected);
    assert_eq!(sent, 0);
    assert_eq!(full, vec![1]);
    assert_eq!(c.regions.get_regions().len(), 3);
    for id in 1..=3 {
        assert!(c.regions.get_region(id).is_some());
    }
}
#[test]
fn state_task_35_full_catchup_empty_scan_positions_and_completion() {
    let (h, _, scan) = setup(2, 2048, 42, 0);
    for id in 1..=1001 {
        h.lock().unwrap().record(region(id)).unwrap();
    }
    let mut frames = vec![];
    let end = scan
        .catch_up(
            &h,
            size(usize::MAX),
            |frame| {
                match frame {
                    CatchUpFrame::Records {
                        start_index,
                        records,
                    } => frames.push((start_index, records.len())),
                    CatchUpFrame::Complete { next_index } => frames.push((next_index.0, 0)),
                }
                Ok::<_, ()>(())
            },
            || panic!("disconnected"),
        )
        .unwrap();
    assert_eq!(frames, vec![(0, 1000), (1000, 1), (1043, 0)]);
    assert_eq!(end, I(1043));
    for _ in 0..30 {
        h.lock().unwrap().maybe_shrink().unwrap();
    }
    assert_eq!(h.lock().unwrap().capacity(), 2);
}
#[test]
fn state_task_35_full_catchup_send_failure_closes_and_releases() {
    for fail_at in [0, 1, 2] {
        let (h, _, scan) = setup(2, 8, 10, 1);
        for id in 1..=5 {
            h.lock().unwrap().record(region(id)).unwrap();
        }
        let mut calls = 0;
        let mut closes = 0;
        let result = scan.catch_up(
            &h,
            size(3),
            |_| {
                let i = calls;
                calls += 1;
                if i == fail_at {
                    Err("unavailable")
                } else {
                    Ok(())
                }
            },
            || closes += 1,
        );
        assert!(matches!(result, Err(FullCatchUpError::Send("unavailable"))));
        assert_eq!(calls, fail_at + 1);
        assert_eq!(closes, 1);
        for _ in 0..9 {
            h.lock().unwrap().maybe_shrink().unwrap();
        }
        assert_eq!(h.lock().unwrap().capacity(), 2);
    }
}
#[test]
fn state_task_35_full_catchup_no_changes_still_completes() {
    let (h, _, scan) = setup(2, 8, 0, 0);
    let mut complete = 0;
    let end = scan
        .catch_up(
            &h,
            size(1),
            |frame| {
                assert!(matches!(frame, CatchUpFrame::Complete { next_index: I(0) }));
                complete += 1;
                Ok::<_, ()>(())
            },
            || panic!("disconnected"),
        )
        .unwrap();
    assert_eq!(complete, 1);
    assert_eq!(end, I(0));
    assert_eq!(h.lock().unwrap().capacity(), 2);
}
