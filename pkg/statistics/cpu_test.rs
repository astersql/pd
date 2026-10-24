// Copyright 2026 AsterSQL.

use astersql_pd_core::{CpuStats, PeerStat, RecordPair};

use crate::{region_read_cpu_usage, store_read_cpu_usage};

fn record(key: &str, value: u64) -> RecordPair {
    RecordPair {
        key: key.into(),
        value,
    }
}

#[test]
fn state_task_09_cpu_store_go_oracle() {
    let records = [
        record("unified-read-0", 80),
        record("grpc-server-0", 20),
        record("other", 30),
    ];
    assert_eq!(store_read_cpu_usage(&records), 80.0);
}

#[test]
fn state_task_09_cpu_store_prefix_and_multiple_records() {
    let records = [
        record("unified-read", 2),
        record("unified-read-0", 3),
        record("unified-reader", 5),
        record("unified-read_1", 7),
        record("unified-read-0", 11),
        record("unified-rea", 100),
        record("Unified-read-0", 100),
        record("xunified-read", 100),
        record("unified_write", 100),
        RecordPair::default(),
    ];
    assert_eq!(store_read_cpu_usage(&records), 28.0);
    assert_eq!(store_read_cpu_usage(&[]), 0.0);
    assert_eq!(store_read_cpu_usage(&[record("grpc-server-0", 80)]), 0.0);
}

#[test]
fn state_task_09_cpu_store_wraps_before_float_conversion() {
    assert_eq!(
        store_read_cpu_usage(&[
            record("unified-read", u64::MAX),
            record("unified-read-0", 81)
        ]),
        80.0
    );
    // Summing as f64 first would round each operand and lose this result.
    let large = 1_u64 << 53;
    assert_eq!(
        store_read_cpu_usage(&[
            record("unified-read", large + 1),
            record("unified-read-1", 1)
        ]),
        (large + 2) as f64
    );
    assert_eq!(
        store_read_cpu_usage(&[record("unified-read", u64::MAX)]),
        u64::MAX as f64
    );
}

#[test]
fn state_task_09_cpu_peer_go_oracle_and_missing() {
    assert_eq!(region_read_cpu_usage(None), 0.0);
    let mut peer = PeerStat::default();
    assert_eq!(region_read_cpu_usage(Some(&peer)), 0.0);
    peer.cpu_stats = Some(CpuStats {
        unified_read: 80,
        scheduler: 0,
    });
    assert_eq!(region_read_cpu_usage(Some(&peer)), 80.0);
    assert_eq!(peer.cpu_stats.as_ref().unwrap().unified_read, 80);
}

#[test]
fn state_task_09_cpu_peer_ignores_scheduler() {
    let mut peer = PeerStat {
        cpu_stats: Some(CpuStats {
            unified_read: 0,
            scheduler: u64::MAX,
        }),
        ..PeerStat::default()
    };
    assert_eq!(region_read_cpu_usage(Some(&peer)), 0.0);
    peer.cpu_stats.as_mut().unwrap().unified_read = u64::MAX;
    assert_eq!(region_read_cpu_usage(Some(&peer)), u64::MAX as f64);
}
