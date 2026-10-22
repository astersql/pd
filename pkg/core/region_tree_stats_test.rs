// Copyright 2026 AsterSQL.
use crate::*;
use std::sync::Arc;
fn region(size: i64, storage: bool) -> Arc<RegionInfo> {
    let mut r = RegionInfo::new(
        RegionMeta {
            id: 1,
            key_range: KeyRange {
                start_key: b"a".to_vec(),
                end_key: b"b".to_vec(),
            },
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    );
    r.stats.approximate_size = size;
    r.stats.written_bytes = 30;
    r.stats.written_keys = 6;
    r.set_report_interval(0, 3);
    if !storage {
        r.set_source(RegionSource::Heartbeat);
    }
    Arc::new(r)
}
#[test]
fn core_task_41_replace_remove() {
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_41.json")).unwrap();
    let mut t = RegionTree::with_count_ref();
    let a = region(1, true);
    t.update(a.clone());
    assert_eq!(t.total_size(), expected["sizes"][0].as_i64().unwrap());
    assert_eq!(a.get_ref(), 1);
    assert_eq!(a.clone_with([]).get_ref(), 0);
    let b = region(5, false);
    t.update(b.clone());
    assert_eq!(a.get_ref(), 0);
    assert_eq!(b.get_ref(), 1);
    assert_eq!(t.total_size(), 5);
    assert_eq!(t.total_write_rate(), (10., 2.));
    assert_eq!(t.not_from_storage_regions_count(), 1);
    t.remove(&b);
    t.remove(&b);
    assert_eq!(b.get_ref(), 0);
    assert_eq!(t.total_size(), 0);
    assert_eq!(t.total_write_rate(), (0., 0.));
    assert_eq!(t.not_from_storage_regions_count(), 0);
}
#[test]
fn core_task_41_in_place_and_uncounted() {
    for counted in [false, true] {
        let mut t = if counted {
            RegionTree::with_count_ref()
        } else {
            RegionTree::new()
        };
        let a = region(1, true);
        t.update(a.clone());
        let b = region(7, false);
        t.update_stat(&a, &b);
        assert_eq!(t.total_size(), 7);
        assert_eq!(t.not_from_storage_regions_count(), 1);
        assert_eq!(a.get_ref(), 0);
        assert_eq!(b.get_ref(), i32::from(counted));
        t.update_stat(&b, &a);
        assert_eq!(t.total_size(), 1);
        assert_eq!(t.not_from_storage_regions_count(), 0);
        assert_eq!(t.total_write_rate(), (10., 2.));
        t.update_ref(&a, &a);
        assert_eq!(a.get_ref(), i32::from(counted));
    }
}
#[test]
fn core_task_41_missing_removal_and_overlap() {
    let a = region(4, false);
    let mut t = RegionTree::from_regions([a.clone()]);
    let mut absent = (*a).clone();
    absent.meta.id = 9;
    t.remove(&absent);
    assert_eq!(t.total_size(), 4);
    assert_eq!(a.get_ref(), 0);
    t.update(a.clone());
    assert_eq!(t.total_size(), 4);
    assert_eq!(t.not_from_storage_regions_count(), 1);
}

#[test]
fn core_task_41_go_size_sequence_and_multi_overlap() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_41.json")).unwrap();
    let mut t = RegionTree::with_count_ref();
    let a = region(1, true);
    t.update(a.clone());
    assert_eq!(t.total_size(), fixture["sizes"][0].as_i64().unwrap());
    let mut b = (*region(3, false)).clone();
    b.meta.key_range = KeyRange {
        start_key: b"b".to_vec(),
        end_key: b"c".to_vec(),
    };
    let b = Arc::new(b);
    t.update(b.clone());
    assert_eq!(t.total_size(), fixture["sizes"][1].as_i64().unwrap());
    let mut c = (*b).clone();
    c.stats.approximate_size = 5;
    c.stats.written_bytes = 60;
    c.set_report_interval(0, 6);
    c.meta.key_range.end_key = b"e".to_vec();
    let c = Arc::new(c);
    t.update(c.clone());
    assert_eq!(t.total_size(), fixture["sizes"][2].as_i64().unwrap());
    assert_eq!(b.get_ref(), 0);
    t.remove(&a);
    assert_eq!(t.total_size(), fixture["sizes"][3].as_i64().unwrap());
    assert_eq!(t.total_write_rate(), (10., 1.));
    t.update(a.clone());
    let mut merged = (*a).clone();
    merged.meta.key_range.end_key = b"e".to_vec();
    merged.stats.approximate_size = 9;
    merged.set_report_interval(0, 2); // Go rejects intervals below 3 seconds.
    let merged = Arc::new(merged);
    assert_eq!(t.update(merged.clone()).len(), 2);
    assert_eq!(a.get_ref(), 0);
    assert_eq!(c.get_ref(), 0);
    assert_eq!(merged.get_ref(), 1);
    assert_eq!(t.total_size(), 9);
    assert_eq!(t.total_write_rate(), (0., 0.));
    assert_eq!(t.not_from_storage_regions_count(), 0);
}
