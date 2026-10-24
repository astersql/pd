// Copyright 2026 AsterSQL.

use super::*;
use serde_json::Value;

fn oracle() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/rust-compat/fixtures/state/task-01.json"
    ))
    .unwrap()
}

fn assert_rates(actual: [f64; DIM_LEN], expected: &Value) {
    assert_eq!(expected.as_array().unwrap().len(), DIM_LEN);
    for (actual, expected) in actual.into_iter().zip(expected.as_array().unwrap()) {
        match expected.as_str() {
            Some("NaN") => assert!(actual.is_nan()),
            Some("+Inf") => assert_eq!(actual, f64::INFINITY),
            Some("-Inf") => assert_eq!(actual, f64::NEG_INFINITY),
            _ => assert_eq!(actual, expected.as_f64().unwrap()),
        }
    }
}

#[test]
fn state_task_01_kind_names_and_defaults() {
    let fixture = oracle();
    let go = &fixture["values"];
    for name in ["byte", "key", "query", "cpu", "", "unknown", "CPU"] {
        assert_eq!(string_to_dim(name) as u64, go[format!("dim_{name}")]);
    }
    for dim in -1..=4 {
        assert_eq!(dim_to_string(dim), go[format!("name_{dim}")]);
    }
    for (dim, name) in [BYTE_PRIORITY, KEY_PRIORITY, QUERY_PRIORITY, CPU_PRIORITY]
        .into_iter()
        .enumerate()
    {
        assert_eq!(string_to_dim(name), dim);
        assert_eq!(dim_to_string(dim as i32), name);
    }
    assert_eq!(string_to_dim(" byte"), BYTE_DIM);
    assert_eq!(string_to_dim("字节"), BYTE_DIM);
    assert_eq!(dim_to_string(i32::MIN), "");
    assert_eq!(REGION_STAT_COUNT, 8);
    assert_eq!(STORE_LOAD_COUNT, 12);
    assert_eq!(RW_TYPE_LEN, 2);
    assert_eq!(ACTION_TYPE_LEN, 3);
    assert_eq!(DEFAULT_AOT_SIZE, 1);
    assert_eq!((DEFAULT_WRITE_MF_SIZE, DEFAULT_READ_MF_SIZE), (5, 5));
    assert_rates(
        RWType::READ.get_load_rates(&MIN_HOT_THRESHOLDS, 2),
        &go["rw_1_rates_2"],
    );
}

#[test]
fn state_task_01_kind_complete_integer_mappings() {
    let fixture = oracle();
    let go = &fixture["values"];
    for value in -1..=REGION_STAT_COUNT as i32 {
        let kind = RegionStatKind::from_i32(value);
        assert_eq!(kind.as_i32(), value);
        assert_eq!(kind.to_string(), go[format!("RegionStatKind_{value}")]);
    }
    for value in -1..=STORE_LOAD_COUNT as i32 {
        let kind = StoreLoadKind::from_i32(value);
        assert_eq!(kind.as_i32(), value);
        assert_eq!(kind.to_string(), go[format!("StoreLoadKind_{value}")]);
    }
    for value in -1..=ACTION_TYPE_LEN as i32 {
        let kind = ActionType::from_i32(value);
        assert_eq!(kind.as_i32(), value);
        assert_eq!(kind.to_string(), go[format!("ActionType_{value}")]);
    }
    // Source-only supplement: this unused method was not linked in the old Go binary.
    assert_eq!(SourceKind::DIRECT.to_string(), "direct");
    assert_eq!(SourceKind::INHERIT.to_string(), "inherit");
    assert_eq!(SourceKind::from_i32(-1).to_string(), "unknown");
    assert_eq!(SourceKind::from_i32(2).to_string(), "unknown");
    assert_eq!(SourceKind::INHERIT.as_i32(), 1);
    assert_eq!(RegionStatKind::default(), RegionStatKind::READ_BYTES);
    assert_eq!(StoreLoadKind::default(), StoreLoadKind::READ_BYTES);
    assert_eq!(RWType::default(), RWType::WRITE);
}

#[test]
fn state_task_01_kind_rw_order_inverse_and_intervals() {
    let fixture = oracle();
    let go = &fixture["values"];
    for value in 0..=2 {
        let rw = RWType::from_i32(value);
        assert_eq!(rw.as_i32(), value);
        assert_eq!(rw.to_string(), go[format!("rw_{value}_String")]);
        assert_eq!(
            rw.inverse().as_i32() as i64,
            go[format!("rw_{value}_Inverse")]
        );
        assert_eq!(
            rw.report_interval(),
            go[format!("rw_{value}_ReportInterval")]
        );
        assert_eq!(
            rw.default_anti_count(),
            go[format!("rw_{value}_DefaultAntiCount")]
        );
        let stats: Vec<_> = rw.region_stats().iter().map(|k| k.as_i32()).collect();
        assert_eq!(
            serde_json::to_value(stats).unwrap(),
            go[format!("rw_{value}_RegionStats")]
        );
    }
    for rw in [RWType::READ, RWType::WRITE] {
        assert_eq!(rw.inverse().inverse(), rw);
    }
    let mut visited = Vec::new();
    foreach_region_stats(|rw, dim, kind| visited.push((rw.as_i32(), dim, kind.as_i32())));
    assert_eq!(
        visited,
        vec![
            (1, 0, 0),
            (1, 1, 1),
            (1, 2, 2),
            (1, 3, 6),
            (0, 0, 3),
            (0, 1, 4),
            (0, 2, 5),
            (0, 3, 7)
        ]
    );
}

#[test]
fn state_task_01_kind_rates_and_zero_interval() {
    let fixture = oracle();
    let go = &fixture["values"];
    assert_eq!(
        MIN_HOT_THRESHOLDS.to_vec(),
        serde_json::from_value::<Vec<f64>>(go["thresholds"].clone()).unwrap()
    );
    for value in 0..=2 {
        for interval in [2, 0] {
            assert_rates(
                RWType::from_i32(value).get_load_rates(&MIN_HOT_THRESHOLDS, interval),
                &go[format!("rw_{value}_rates_{interval}")],
            );
        }
    }
    let mut mixed = MIN_HOT_THRESHOLDS;
    mixed[0] = 0.0;
    mixed[1] = -128.0;
    assert_rates(RWType::READ.get_load_rates(&mixed, 0), &go["zero_mixed"]);
    let deltas = [2., 4., 6., 8., 10., 12., 14., 16.];
    assert_eq!(RWType::READ.get_load_rates(&deltas, 2), [1., 2., 3., 7.]);
    assert_eq!(RWType::WRITE.get_load_rates(&deltas, 2), [4., 5., 6., 8.]);
    assert!(RWType::READ.get_load_rates(&[0.; REGION_STAT_COUNT], 0)[0].is_nan());
    assert_eq!(RWType::from_i32(-1).get_load_rates(&[], 0), [0.; DIM_LEN]);
    let max_interval = u64::MAX;
    let max_loads = [max_interval as f64; REGION_STAT_COUNT];
    assert_eq!(
        RWType::WRITE.get_load_rates(&max_loads, max_interval),
        [1.; DIM_LEN]
    );
}

#[test]
fn state_task_01_kind_set_rates_preserves_other_direction() {
    let fixture = oracle();
    let go = &fixture["values"];
    let mut full = MIN_HOT_THRESHOLDS;
    let loads: [f64; DIM_LEN] = full[4..8].try_into().unwrap();
    RWType::WRITE.set_full_load_rates(&mut full, &loads);
    assert_eq!(
        full.to_vec(),
        serde_json::from_value::<Vec<f64>>(go["full_after_write"].clone()).unwrap()
    );
    let loads: [f64; DIM_LEN] = full[4..8].try_into().unwrap();
    RWType::READ.set_full_load_rates(&mut full, &loads);
    assert_eq!(
        full.to_vec(),
        serde_json::from_value::<Vec<f64>>(go["full_after_read"].clone()).unwrap()
    );
    let mut full = [-1.; REGION_STAT_COUNT];
    RWType::READ.set_full_load_rates(&mut full, &[1., 2., 3., 4.]);
    assert_eq!(full, [1., 2., 3., -1., -1., -1., 4., -1.]);
    RWType::WRITE.set_full_load_rates(&mut full, &[5., 6., 7., 8.]);
    assert_eq!(full, [1., 2., 3., 5., 6., 7., 4., 8.]);
    RWType::from_i32(99).set_full_load_rates(&mut [], &[]);
}

#[test]
fn state_task_01_kind_short_slices_follow_accessed_indexes() {
    // Go indexes only the selected direction; invalid RW never indexes either slice.
    assert!(std::panic::catch_unwind(|| RWType::READ.get_load_rates(&[1.; 6], 1)).is_err());
    assert!(std::panic::catch_unwind(|| RWType::WRITE.get_load_rates(&[1.; 7], 1)).is_err());
    assert!(
        std::panic::catch_unwind(|| RWType::READ.set_full_load_rates(&mut [0.; 8], &[1.; 3]))
            .is_err()
    );
    assert!(
        std::panic::catch_unwind(|| RWType::WRITE.set_full_load_rates(&mut [0.; 7], &[1.; 4]))
            .is_err()
    );
    assert_eq!(RWType::READ.get_load_rates(&[0.; 7], 1), [0.; DIM_LEN]);
}

#[test]
fn state_task_01_kind_single_owned_label() {
    let fixture = oracle();
    let mut label = single_label("store", "42");
    assert_eq!(
        serde_json::to_value(&label).unwrap(),
        fixture["values"]["single_label"]
    );
    label.insert("store".into(), "other".into());
    assert_eq!(single_label("store", "42")["store"], "42");
    assert_eq!(single_label("", "").len(), 1);
    assert_eq!(single_label("标签", "值")["标签"], "值");
}
