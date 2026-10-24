// Copyright 2026 AsterSQL.
use crate::{HistoryBuffer, HistoryError, HistoryIndex as I};
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
fn append(h: &mut HistoryBuffer, ids: std::ops::RangeInclusive<u64>) {
    for id in ids {
        h.record(region(id)).unwrap();
    }
}
fn ids(h: &HistoryBuffer, start: u64) -> Vec<u64> {
    h.retained_records_from(I(start))
        .unwrap()
        .0
        .iter()
        .map(|r| r.meta().id)
        .collect()
}
#[test]
fn state_task_31_history_window_retain_keeps_catch_up_records() {
    let mut h = HistoryBuffer::with_config(2, 8, 1).unwrap();
    h.reset_with_index(I(10));
    let _retain = h.retain_from(I(10));
    append(&mut h, 100..=104);
    assert_eq!(h.capacity(), 8);
    assert_eq!((h.first_index(), h.next_index()), (I(10), I(15)));
    assert_eq!(ids(&h, 10), vec![100, 101, 102, 103, 104]);
}
#[test]
fn state_task_31_history_window_observed_required_window() {
    let mut h = HistoryBuffer::with_config(2, 8, 1).unwrap();
    h.reset_with_index(I(10));
    append(&mut h, 1..=2);
    h.observe_required_window(5).unwrap();
    append(&mut h, 3..=5);
    assert_eq!(h.capacity(), 8);
    assert_eq!(ids(&h, 10), vec![1, 2, 3, 4, 5]);
    assert_eq!(h.next_index(), I(15));
}
#[test]
fn state_task_31_history_window_full_sync_priority() {
    let mut h = HistoryBuffer::with_config(2, 8, 1).unwrap();
    h.reset_with_index(I(10));
    append(&mut h, 1..=2);
    let retain = h.retain_from(I(10));
    h.observe_required_window(4).unwrap();
    append(&mut h, 3..=5);
    for _ in 0..10 {
        h.maybe_shrink().unwrap();
    }
    assert_eq!(h.capacity(), 8);
    assert_eq!(ids(&h, 10), vec![1, 2, 3, 4, 5]);
    drop(retain);
    for _ in 0..3 {
        h.maybe_shrink().unwrap();
    }
    assert_eq!(h.capacity(), 4);
    assert_eq!(ids(&h, 11), vec![2, 3, 4, 5]);
}
#[test]
fn state_task_31_history_window_shrinks_one_step_after_low_window() {
    let mut h = HistoryBuffer::with_config(2, 8, 1).unwrap();
    h.observe_required_window(3).unwrap();
    assert_eq!(h.capacity(), 8);
    h.maybe_shrink().unwrap(); // high observation resets the low streak
    for _ in 0..2 {
        h.observe_required_window(0).unwrap();
        h.maybe_shrink().unwrap();
        assert_eq!(h.capacity(), 8);
    }
    h.maybe_shrink().unwrap();
    assert_eq!(h.capacity(), 4);
    for _ in 0..3 {
        h.maybe_shrink().unwrap();
    }
    assert_eq!(h.capacity(), 2);
    for _ in 0..5 {
        h.maybe_shrink().unwrap();
    }
    assert_eq!(h.capacity(), 2);
}
#[test]
fn state_task_31_history_window_gap_at_maximum_and_invalid_indexes() {
    let mut h = HistoryBuffer::with_config(2, 4, 1).unwrap();
    let _retain = h.retain_from(I(0));
    append(&mut h, 1..=5);
    assert_eq!(h.capacity(), 4);
    assert!(matches!(
        h.retained_records_from(I(0)),
        Err(HistoryError::HistoryGap {
            requested: I(0),
            first: I(1),
            next: I(5)
        })
    ));
    assert_eq!(ids(&h, 1), vec![2, 3, 4, 5]);
    assert!(ids(&h, 5).is_empty());
    assert!(matches!(
        h.retained_records_from(I(6)),
        Err(HistoryError::HistoryGap { .. })
    ));
}
#[test]
fn state_task_31_history_window_duplicate_guards_and_error_cancel() {
    let mut h = HistoryBuffer::with_config(2, 16, 1).unwrap();
    let a = h.retain_from(I(0));
    let b = h.retain_from(I(0));
    drop(a);
    append(&mut h, 1..=5);
    assert_eq!(h.capacity(), 8);
    drop(b);
    let result: Result<(), ()> = (|| {
        let _guard = h.retain_from(I(0));
        Err(())
    })();
    assert!(result.is_err());
    for _ in 0..3 {
        h.maybe_shrink().unwrap();
    }
    assert_eq!(h.capacity(), 4);
    let guard = h.retain_from(h.next_index());
    std::thread::spawn(move || drop(guard)).join().unwrap();
    for _ in 0..3 {
        h.maybe_shrink().unwrap();
    }
    assert_eq!(h.capacity(), 2);
}
#[test]
fn state_task_31_history_window_reset_invalidates_old_guards() {
    let mut h = HistoryBuffer::with_config(2, 8, 1).unwrap();
    let stale = h.retain_from(I(0));
    append(&mut h, 1..=5);
    h.reset_with_index(I(0));
    assert!(h.is_empty());
    assert_eq!(h.capacity(), 2);
    let current = h.retain_from(I(0));
    drop(stale);
    append(&mut h, 1..=5);
    assert_eq!(ids(&h, 0), vec![1, 2, 3, 4, 5]);
    drop(current);
    for _ in 0..3 {
        h.maybe_shrink().unwrap();
    }
    assert_eq!(h.capacity(), 4);
}
#[test]
fn state_task_31_history_window_high_resets_streak_and_overflow_safe() {
    let mut h = HistoryBuffer::with_config(2, 16, 1).unwrap();
    h.observe_required_window(u64::MAX).unwrap();
    assert_eq!(h.capacity(), 16);
    h.maybe_shrink().unwrap();
    for _ in 0..2 {
        h.maybe_shrink().unwrap();
    }
    h.observe_required_window(5).unwrap();
    h.observe_required_window(0).unwrap();
    h.maybe_shrink().unwrap();
    assert_eq!(h.capacity(), 16);
    for _ in 0..2 {
        h.maybe_shrink().unwrap();
        assert_eq!(h.capacity(), 16);
    }
    h.maybe_shrink().unwrap();
    assert_eq!(h.capacity(), 8);
}

#[test]
fn state_task_31_history_window_cancelled_future_releases_guard() {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    let mut h = HistoryBuffer::with_config(2, 8, 1).unwrap();
    h.observe_required_window(3).unwrap();
    h.maybe_shrink().unwrap();
    let guard = h.retain_from(I(0));
    let mut future = Box::pin(async move {
        let _guard = guard;
        std::future::pending::<()>().await;
    });
    assert_eq!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Pending
    );
    for _ in 0..3 {
        h.maybe_shrink().unwrap();
    }
    assert_eq!(h.capacity(), 8);
    drop(future);
    for _ in 0..3 {
        h.maybe_shrink().unwrap();
    }
    assert_eq!(h.capacity(), 4);
}

#[test]
fn state_task_31_history_window_oldest_guard_and_wrap_growth() {
    let mut h = HistoryBuffer::with_config(2, 16, 1).unwrap();
    append(&mut h, 1..=3); // old index 0 has already been evicted
    let oldest = h.retain_from(I(1));
    let later = h.retain_from(I(3));
    append(&mut h, 4..=5);
    assert_eq!(h.capacity(), 4);
    assert_eq!(ids(&h, 1), vec![2, 3, 4, 5]);
    drop(oldest);
    append(&mut h, 6..=7);
    assert_eq!(h.capacity(), 4);
    assert_eq!(ids(&h, 3), vec![4, 5, 6, 7]);
    h.record(region(8)).unwrap();
    assert_eq!(h.capacity(), 8);
    assert_eq!(ids(&h, 3), vec![4, 5, 6, 7, 8]);
    drop(later);
}

#[test]
fn state_task_31_history_window_normalization_and_missing_past() {
    let mut h = HistoryBuffer::with_config(3, 1, 3).unwrap();
    h.observe_required_window(100).unwrap();
    assert_eq!(h.capacity(), 3);
    append(&mut h, 1..=4);
    let _guard = h.retain_from(I(0));
    assert!(matches!(
        h.retained_records_from(I(0)),
        Err(HistoryError::HistoryGap { .. })
    ));
    // Growth cannot reconstruct overwritten history.
    let mut h = HistoryBuffer::with_config(2, 8, 1).unwrap();
    append(&mut h, 1..=3);
    let _guard = h.retain_from(I(0));
    h.record(region(4)).unwrap();
    assert_eq!(h.capacity(), 4);
    assert!(matches!(
        h.retained_records_from(I(0)),
        Err(HistoryError::HistoryGap { first: I(1), .. })
    ));
    assert_eq!(ids(&h, 1), vec![2, 3, 4]);
}

#[test]
fn state_task_31_history_window_failed_append_preserves_retained_snapshot() {
    let mut h = HistoryBuffer::with_config(2, 8, 1).unwrap();
    h.reset_with_index(I(u64::MAX - 2));
    let _guard = h.retain_from(h.next_index());
    append(&mut h, 1..=2);
    assert!(matches!(
        h.record(region(3)),
        Err(HistoryError::IndexExhausted)
    ));
    assert_eq!(h.capacity(), 2);
    assert_eq!(h.next_index(), I(u64::MAX));
    assert_eq!(ids(&h, u64::MAX - 2), vec![1, 2]);
}
