// Copyright 2026 AsterSQL.
use super::{DFSStats, StoreInfo, StoreStats};

#[test]
fn core_task_14_raw_stats_fixture() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_14.json")).unwrap();
    let mut store = StoreInfo::new(None);
    assert_eq!(store.store_stats(), StoreStats::default());
    for case in fixture["cases"].as_array().unwrap() {
        let stats: StoreStatsInput = serde_json::from_value(case["stats"].clone()).unwrap();
        let raw = StoreStats {
            capacity: stats.capacity,
            available: stats.available,
            used_size: stats.used_size,
            is_busy: stats.is_busy,
            sending_snap_count: stats.sending_snap_count,
            receiving_snap_count: stats.receiving_snap_count,
            bytes_written: stats.bytes_written,
            bytes_read: stats.bytes_read,
            keys_written: stats.keys_written,
            keys_read: stats.keys_read,
        };
        store.update_raw_stats(raw.clone());
        assert_eq!(store.store_stats(), raw, "{}", case["name"]);
        assert_eq!(store.capacity(), raw.capacity);
        assert_eq!(store.available(), raw.available);
        assert_eq!(store.used_size(), raw.used_size);
        assert_eq!(store.is_busy(), raw.is_busy);
        assert_eq!(store.sending_snap_count(), raw.sending_snap_count);
        assert_eq!(store.receiving_snap_count(), raw.receiving_snap_count);
        assert_eq!(store.bytes_written(), raw.bytes_written);
        assert_eq!(store.bytes_read(), raw.bytes_read);
        assert_eq!(store.keys_written(), raw.keys_written);
        assert_eq!(store.keys_read(), raw.keys_read);
    }
}

#[derive(serde::Deserialize)]
struct StoreStatsInput {
    capacity: u64,
    available: u64,
    used_size: u64,
    is_busy: bool,
    sending_snap_count: u32,
    receiving_snap_count: u32,
    bytes_written: u64,
    bytes_read: u64,
    keys_written: u64,
    keys_read: u64,
}

#[test]
fn core_task_14_dfs_owned_counters() {
    let mut stats = DFSStats::default();
    assert_eq!(stats.written_bytes, 0);
    assert_eq!(stats.write_requests, 0);
    stats.written_bytes = u64::MAX;
    stats.write_requests = 7;
    let copy = stats.clone();
    stats.write_requests = 0;
    assert_eq!(stats.write_requests, 0);
    assert_eq!(copy.written_bytes, u64::MAX);
    assert_eq!(copy.write_requests, 7);
}
