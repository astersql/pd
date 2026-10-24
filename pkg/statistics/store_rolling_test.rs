// Copyright 2026 AsterSQL.
use super::RollingStoreStats;
use astersql_pd_core::{QueryStats, RecordPair, StoreStats, TimeInterval};
use astersql_pd_statistics_utils::StoreLoadKind as K;

fn pair(key: &str, value: u64) -> RecordPair {
    RecordPair {
        key: key.into(),
        value,
    }
}
fn report(secs: u64, scale: u64) -> StoreStats {
    StoreStats {
        interval: Some(TimeInterval {
            start_timestamp: 0,
            end_timestamp: secs,
        }),
        bytes_read: 10 * secs * scale,
        keys_read: 20 * secs * scale,
        bytes_written: 30 * secs * scale,
        keys_written: 40 * secs * scale,
        query_stats: Some(QueryStats {
            get: 25 * secs * scale,
            scan: 25 * secs * scale,
            put: 30 * secs * scale,
            commit: 30 * secs * scale,
            ..Default::default()
        }),
        cpu_usages: vec![
            pair("unified-read-0", 70 * scale),
            pair("other", 10 * scale),
        ],
        read_io_rates: vec![pair("a", 40 * scale), pair("b", 50 * scale)],
        write_io_rates: vec![pair("a", 100 * scale)],
        ..Default::default()
    }
}
fn check(r: &RollingStoreStats, loads: [f64; 12]) {
    for (i, value) in loads.into_iter().enumerate() {
        assert_eq!(r.get_load(K::from_i32(i as i32)), value, "kind {i}");
    }
}
#[test]
fn state_task_10_store_rolling_go_cpu_time_median() {
    let mut r = RollingStoreStats::new();
    for secs in [5, 10, 20, 10, 5] {
        r.observe(&StoreStats {
            interval: Some(TimeInterval {
                start_timestamp: 0,
                end_timestamp: secs,
            }),
            cpu_usages: vec![pair("unified-read-pool-0", 100)],
            ..Default::default()
        });
    }
    assert_eq!(r.get_load(K::READ_CPU), 100.);
    assert_eq!(r.get_instant_load(K::READ_CPU), 100.);
}
#[test]
fn state_task_10_store_rolling_all_kinds_and_unknown() {
    let mut r = RollingStoreStats::default();
    check(&r, [0.; 12]);
    for i in -1..14 {
        assert_eq!(r.get_instant_load(K::from_i32(i)), 0.);
    }
    for secs in [5, 10, 20, 10, 5] {
        r.observe(&report(secs, 1));
    }
    r.observe_regions_stats(110., 120.);
    let loads = [
        10., 20., 30., 40., 50., 60., 80., 90., 100., 110., 120., 70.,
    ];
    check(&r, loads);
    for (i, value) in loads.into_iter().enumerate() {
        assert_eq!(r.get_instant_load(K::from_i32(i as i32)), value);
    }
    for i in [-1, 12, i32::MAX] {
        assert_eq!(r.get_load(K::from_i32(i)), 0.);
    }
}
#[test]
fn state_task_10_store_rolling_partial_intervals_and_median_eviction() {
    let mut r = RollingStoreStats::new();
    r.observe(&report(5, 1));
    assert_eq!(r.get_load(K::READ_BYTES), 0.);
    assert_eq!(r.get_instant_load(K::READ_BYTES), 10.);
    r.observe(&report(5, 3));
    assert_eq!(r.get_load(K::READ_BYTES), 20.);
    assert_eq!(r.get_instant_load(K::READ_BYTES), 30.);
    r.observe(&report(20, 5));
    assert_eq!(r.get_load(K::READ_BYTES), 35.);
    assert_eq!(r.get_load(K::READ_CPU), 245.);
    assert_eq!(r.get_instant_load(K::READ_CPU), 350.);
    for _ in 0..5 {
        r.observe(&report(10, 9));
    }
    assert_eq!(r.get_load(K::READ_BYTES), 90.);
    assert_eq!(r.get_load(K::READ_CPU), 630.);
}
#[test]
fn state_task_10_store_rolling_set_preserves_time_history() {
    let mut r = RollingStoreStats::new();
    r.set(&report(10, 2));
    assert_eq!(r.get_load(K::READ_BYTES), 20.);
    assert_eq!(r.get_instant_load(K::READ_BYTES), 0.);
    assert_eq!(r.get_load(K::READ_CPU), 140.);
    assert_eq!(r.get_instant_load(K::READ_CPU), 0.);
    assert_eq!(r.get_read_cpu_recent_max(), 140.);
    r.observe(&report(5, 1));
    assert_eq!(r.get_load(K::READ_BYTES), 20.); // average still not full
    assert_eq!(r.get_instant_load(K::READ_BYTES), 10.);
    r.set(&report(20, 3));
    assert_eq!(r.get_load(K::READ_BYTES), 30.);
    assert_eq!(r.get_instant_load(K::READ_BYTES), 10.);
    r.observe(&report(5, 5));
    assert_eq!(r.get_load(K::READ_BYTES), 30.); // retained 5s + new 5s, rate 30
    assert_eq!(r.get_instant_load(K::READ_BYTES), 50.);
    assert_eq!(r.get_read_cpu_recent_max(), 350.);
}
#[test]
fn state_task_10_store_rolling_zero_interval_and_collect_overflow() {
    let mut r = RollingStoreStats::new();
    r.observe(&report(10, 1));
    r.set_regions_stats(110., 120.);
    let mut zero = report(0, 9);
    zero.bytes_read = 999;
    r.set(&zero); // zero Set must not change *any* dimension
    check(
        &r,
        [
            10., 20., 30., 40., 50., 60., 80., 90., 100., 110., 120., 70.,
        ],
    );
    zero.interval = None;
    zero.cpu_usages = vec![pair("unified-read-a", u64::MAX), pair("unified-read-b", 81)];
    zero.read_io_rates = vec![pair("a", u64::MAX), pair("b", 101)];
    r.observe(&zero);
    assert_eq!(r.get_instant_load(K::READ_BYTES), 10.);
    assert_eq!(r.get_instant_load(K::READ_CPU), 70.);
    assert_eq!(r.get_instant_load(K::CPU_USAGE), 80.);
    assert_eq!(r.get_instant_load(K::DISK_READ_RATE), 100.);
    assert_eq!(r.get_read_cpu_recent_max(), 80.);
    assert_eq!(r.get_load(K::DISK_READ_RATE), 95.);
}
#[test]
fn state_task_10_store_rolling_distinct_windows_and_recent_max() {
    let mut r = RollingStoreStats::new();
    r.observe(&report(10, 10));
    r.observe_regions_stats(1000., 2000.);
    for _ in 0..2 {
        r.observe(&report(10, 1));
    }
    assert_eq!(r.get_load(K::DISK_READ_RATE), 90.);
    assert_eq!(r.get_load(K::CPU_USAGE), 80.);
    for _ in 0..15 {
        r.observe(&report(10, 1));
    }
    assert_eq!(r.get_read_cpu_recent_max(), 700.); // sample 1 still in 18-report window
    r.observe(&report(10, 1));
    assert_eq!(r.get_read_cpu_recent_max(), 70.);
    for _ in 0..4 {
        r.observe_regions_stats(10., 20.);
    }
    assert_eq!(r.get_load(K::REGIONS_WRITE_BYTES), 10.);
    r.set_regions_stats(300., 400.);
    r.set(&report(10, 2));
    assert_eq!(r.get_load(K::REGIONS_WRITE_BYTES), 300.);
    assert_eq!(r.get_instant_load(K::REGIONS_WRITE_KEYS), 400.);
    r.observe_regions_stats(100., 200.);
    assert_eq!(r.get_load(K::REGIONS_WRITE_BYTES), 200.);
    assert_eq!(r.get_load(K::REGIONS_WRITE_KEYS), 300.);
    // Prove each median capacity with a rising sequence, including wraparound.
    let mut r = RollingStoreStats::new();
    for n in 1..=10 {
        r.observe(&report(10, n));
        r.observe_regions_stats(n as f64, 2. * n as f64);
    }
    assert_eq!(r.get_load(K::CPU_USAGE), 640.); // median 6..10 = 8, window 5
    assert_eq!(r.get_load(K::DISK_READ_RATE), 810.); // median 8..10 = 9, window 3
    assert_eq!(r.get_load(K::REGIONS_WRITE_BYTES), 6.); // median 2..10, window 9
}
#[test]
fn state_task_10_store_rolling_unsigned_set_and_duration_wrap() {
    let mut r = RollingStoreStats::new();
    let stats = StoreStats {
        interval: Some(TimeInterval {
            start_timestamp: 1,
            end_timestamp: 0,
        }),
        bytes_read: u64::MAX,
        cpu_usages: vec![pair("unified-read", 100)],
        ..Default::default()
    };
    r.set(&stats);
    assert_eq!(r.get_load(K::READ_BYTES), 1.); // unsigned seconds subtraction
    r.observe(&stats); // Go Duration is -1 second after nanosecond wrapping
    assert_eq!(r.get_instant_load(K::READ_BYTES), -(u64::MAX as f64));
    assert_eq!(r.get_instant_load(K::READ_CPU), 100.);
    assert_eq!(r.get_load(K::READ_BYTES), 1.); // negative interval not full
}
