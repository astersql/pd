// Copyright 2026 AsterSQL.
use crate::*;
#[test]
fn core_task_30_rounding() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_30.json")).unwrap();
    for c in f["rounding"].as_array().unwrap() {
        let flow = c[0].as_u64().unwrap();
        let stats = RegionHeartbeatStats {
            read_bytes: flow,
            written_bytes: flow,
            ..Default::default()
        }
        .normalize(c[1].as_u64().unwrap());
        assert_eq!(stats.round_bytes_read(), c[2].as_u64().unwrap());
        assert_eq!(stats.round_bytes_written(), c[2].as_u64().unwrap());
    }
}
#[test]
fn core_task_30_sizes_and_clone() {
    let mut r = RegionInfo::new(RegionMeta::default(), None, vec![], vec![]);
    r.set_heartbeat_stats(
        RegionHeartbeatStats {
            approximate_size: 1,
            approximate_kv_size: 100 * 1048576,
            approximate_ia_kv_size: 40 * 1048576,
            approximate_columnar_kv_size: 3 * 1048576 + 1,
            approximate_keys: u64::MAX,
            ..Default::default()
        },
        10,
    );
    let s = r.stats();
    assert_eq!(
        (
            s.approximate_size,
            s.approximate_kv_size,
            s.approximate_ia_kv_size,
            s.approximate_columnar_kv_size,
            s.approximate_keys
        ),
        (1, 100, 40, 3, -1)
    );
    assert_eq!(r.clone_with([]).stats(), s);
    assert_eq!(
        r.clone_with([RegionOption::FlowRoundByDigit(3)])
            .stats()
            .flow_round_divisor,
        1000
    );
    assert_eq!(r.stats().flow_round_divisor, 10);
    assert_eq!(
        RegionHeartbeatStats::default()
            .normalize(0)
            .approximate_size,
        0
    );
    assert_eq!(
        RegionHeartbeatStats {
            approximate_size: 2097151,
            ..Default::default()
        }
        .normalize(0)
        .approximate_size,
        1
    );
}
#[test]
fn core_task_30_invalid_flow_pairs() {
    let limit = 1u64 << 50;
    for (rb, rk, wb, wk, expected) in [
        (limit, 1, 2, 3, (0, 0, 2, 3)),
        (1, limit, 2, 3, (0, 0, 2, 3)),
        (2, 3, limit, 1, (2, 3, 0, 0)),
        (2, 3, 1, u64::MAX, (2, 3, 0, 0)),
        (limit - 1, limit - 1, 0, 0, (limit - 1, limit - 1, 0, 0)),
    ] {
        let s = RegionHeartbeatStats {
            read_bytes: rb,
            read_keys: rk,
            written_bytes: wb,
            written_keys: wk,
            ..Default::default()
        }
        .normalize(0);
        assert_eq!(
            (s.read_bytes, s.read_keys, s.written_bytes, s.written_keys),
            expected
        );
    }
    assert_eq!(flow_round_divisor_by_digit(-1), 0);
    assert_eq!(flow_round_divisor_by_digit(3), 1000);
    assert_eq!(flow_round_divisor_by_digit(i64::MIN), 0);
    assert_eq!(
        RegionHeartbeatStats {
            read_bytes: 258623,
            ..Default::default()
        }
        .normalize(flow_round_divisor_by_digit(64))
        .round_bytes_read(),
        0
    );
}

#[test]
fn core_task_68_store_peer_statistics() {
    // region.go:GetStorePeerApproximate{Size,Keys}; store ID zero bypasses witness.
    let mut r = RegionInfo::new(
        RegionMeta {
            peers: vec![
                Peer {
                    id: 1,
                    store_id: 1,
                    is_witness: true,
                    ..Default::default()
                },
                Peer {
                    id: 2,
                    store_id: 2,
                    ..Default::default()
                },
                Peer {
                    id: 3,
                    store_id: 0,
                    is_witness: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    );
    r.set_heartbeat_stats(
        RegionHeartbeatStats {
            approximate_size: 7 * 1048576,
            approximate_keys: 19,
            ..Default::default()
        },
        0,
    );
    for (store, size, keys) in [(1, 0, 0), (2, 7, 19), (9, 7, 19), (0, 7, 19)] {
        assert_eq!(r.store_peer_approximate_size(store), size);
        assert_eq!(r.store_peer_approximate_keys(store), keys);
    }
    let copy = r.clone_with([RegionOption::RemoveStorePeer(1)]);
    assert_eq!(copy.store_peer_approximate_size(1), 7);
    assert_eq!(copy.store_peer_approximate_keys(1), 19);
    assert_eq!(r.store_peer_approximate_size(1), 0);
    assert_eq!(r.store_peer_approximate_keys(1), 0);
}

#[test]
fn core_task_68_raw_stat_options_preserve_signed_values_and_source() {
    // region_option.go:Set{WrittenBytes,WrittenKeys,ReadBytes,ReadKeys,
    // ApproximateSize,ApproximateKvSize,ApproximateIAKvSize,ApproximateKeys}
    // assign directly; unlike RegionFromHeartbeat they do not normalize.
    let mut r = RegionInfo::new(RegionMeta::default(), None, vec![], vec![]);
    let stats = r.stats_mut();
    stats.written_bytes = u64::MAX;
    stats.written_keys = 7;
    stats.read_bytes = 9;
    stats.read_keys = 11;
    stats.approximate_size = -3;
    stats.approximate_kv_size = -4;
    stats.approximate_ia_kv_size = -5;
    stats.approximate_keys = -6;
    assert_eq!(r.stats().written_bytes, u64::MAX);
    assert_eq!(r.stats().written_keys, 7);
    assert_eq!(r.stats().read_bytes, 9);
    assert_eq!(r.stats().read_keys, 11);
    assert_eq!(r.store_peer_approximate_size(0), -3);
    assert_eq!(r.stats().approximate_kv_size, -4);
    assert_eq!(r.stats().approximate_ia_kv_size, -5);
    assert_eq!(r.store_peer_approximate_keys(0), -6);
    assert_eq!(r.source(), RegionSource::Storage);
    let mut copy = r.clone_with([]);
    copy.stats_mut().approximate_size = 4;
    assert_eq!(r.stats().approximate_size, -3);
}
