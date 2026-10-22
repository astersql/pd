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
