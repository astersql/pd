// Copyright 2026 AsterSQL.
use super::*;
#[tokio::test(start_paused = true)]
async fn task_061_clock_preserves_pending_ticker_and_reset() {
    let mut timer = Maintenance::new(
        Arc::new(astersql_pd_storage::MemoryStore::default()),
        1,
        1,
        Duration::from_secs(10),
    );
    tokio::time::advance(Duration::from_secs(3)).await;
    assert!(
        !timer.flush_due(Duration::from_secs(1)),
        "config updates do not replay or shorten a pending Go timer"
    );
    tokio::time::advance(Duration::from_secs(7)).await;
    assert!(timer.flush_due(Duration::from_secs(1)));
    tokio::time::advance(Duration::from_millis(999)).await;
    assert!(!timer.flush_due(Duration::from_secs(1)));
    tokio::time::advance(Duration::from_millis(1)).await;
    assert!(timer.flush_due(Duration::from_secs(1)));
}
#[tokio::test(start_paused = true)]
async fn task_061_clock_daily_retention() {
    let zone = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
    let start = zone.with_ymd_and_hms(2026, 10, 30, 3, 59, 59).unwrap();
    let first = first_delete(start);
    assert_eq!(first - start.timestamp_millis(), 1000);
    assert_eq!(
        first_delete(start + chrono::Duration::seconds(2)) - first,
        86_400_000
    );
    let mut timer = Maintenance::new(
        Arc::new(astersql_pd_storage::MemoryStore::default()),
        1,
        1,
        Duration::from_secs(600),
    );
    timer.next_delete = first;
    assert!(!timer.delete_due(first - 1));
    assert!(timer.delete_due(first));
    assert!(!timer.delete_due(first + 86_400_000 - 1));
    assert!(timer.delete_due(first + 86_400_000));
    assert_eq!(timer.next_delete, first + 2 * 86_400_000);
}
