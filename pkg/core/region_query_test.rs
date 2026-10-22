// Copyright 2026 AsterSQL.
use crate::*;
#[test]
fn core_task_31_query_merge_reset() {
    let mut q = QueryStats {
        get: u64::MAX,
        scan: 2,
        coprocessor: 3,
        put: 4,
        delete: 5,
        delete_range: 6,
        acquire_pessimistic_lock: 7,
        commit: 8,
        prewrite: 9,
        rollback: 10,
        gc: 11,
    };
    assert_eq!(q.read_query_num(), 4);
    assert_eq!(q.write_query_num(), 49);
    q.merge(&QueryStats {
        get: 1,
        gc: 2,
        ..Default::default()
    });
    assert_eq!(q.get, 0);
    q.reset_read();
    assert_eq!(q.read_query_num(), 0);
    assert_eq!(q.write_query_num(), 49);
    q.reset_write();
    assert_eq!(q.write_query_num(), 0);
    assert_eq!(q.gc, 13);
}
#[test]
fn core_task_31_loads_and_rates() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_31.json")).unwrap();
    let mut r = RegionInfo::new(RegionMeta::default(), None, vec![], vec![]);
    assert_eq!(r.loads(), [0.; 8]);
    r.set_heartbeat_stats(
        RegionHeartbeatStats {
            read_bytes: 8,
            read_keys: 9,
            written_bytes: 10,
            written_keys: 3,
            ..Default::default()
        },
        0,
    );
    r.set_query_stats(Some(QueryStats {
        get: 2,
        put: 4,
        ..Default::default()
    }));
    r.add_query_stats(None);
    r.add_query_stats(Some(QueryStats {
        scan: 3,
        commit: 5,
        ..Default::default()
    }));
    assert_eq!(r.read_query_num(), 5);
    assert_eq!(r.write_query_num(), 9);
    r.set_query_stats(Some(QueryStats {
        get: 2,
        put: 4,
        ..Default::default()
    }));
    r.set_read_cpu_usage(7);
    assert_eq!(r.loads(), [8., 9., 2., 10., 3., 4., 7., 0.]);
    assert_eq!(r.write_loads(), [0., 0., 0., 10., 3., 4., 0., 0.]);
    for case in fixture["rates"].as_array().unwrap() {
        r.set_report_interval(case[0].as_u64().unwrap(), case[1].as_u64().unwrap());
        assert_eq!(
            r.write_rate(),
            (case[2].as_f64().unwrap(), case[3].as_f64().unwrap())
        );
    }
    let copy = r.clone_with([]);
    assert_eq!(copy.loads(), r.loads());
    assert_eq!(copy.write_rate(), r.write_rate());
    r.set_query_stats(None);
    assert_eq!(r.read_query_num(), 0);
    assert_eq!(r.write_query_num(), 0);
    r.add_query_stats(None);
    assert_eq!(r.read_query_num(), 0);
    r.add_query_stats(Some(QueryStats {
        scan: 6,
        ..Default::default()
    }));
    assert_eq!(r.read_query_num(), 6);
}
