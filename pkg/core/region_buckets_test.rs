// Copyright 2026 AsterSQL.
use crate::{RegionBuckets, RegionInfo, RegionMeta};
use std::sync::{Arc, Barrier, Mutex};

fn region() -> RegionInfo {
    RegionInfo::new(RegionMeta::default(), None, vec![], vec![])
}
fn buckets(version: u64) -> Arc<RegionBuckets> {
    Arc::new(RegionBuckets {
        region_id: 100,
        version,
        keys: vec![b"a".to_vec()],
        ..Default::default()
    })
}
#[test]
fn core_task_34_versions() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_34.json")).unwrap();
    for row in fixture["versions"].as_array().unwrap() {
        let mut r = region();
        let old = buckets(row["old"].as_u64().unwrap());
        assert!(r.compare_and_set_report_buckets(Some(old.clone())));
        let candidate = buckets(row["candidate"].as_u64().unwrap());
        assert!(r.compare_and_set_report_buckets(Some(candidate.clone())));
        assert_eq!(
            r.buckets().unwrap().version,
            row["expected"].as_u64().unwrap()
        );
        assert!(Arc::ptr_eq(
            r.report_buckets().unwrap(),
            if candidate.version > old.version {
                &candidate
            } else {
                &old
            }
        ));
    }
}
#[test]
fn core_task_34_identity_and_clear() {
    let mut r = region();
    let original = buckets(2);
    assert!(r.update_buckets(Some(&original), None));
    let old = r.report_buckets().unwrap().clone();
    assert!(!Arc::ptr_eq(&old, &original));
    assert!(!r.update_buckets(Some(&buckets(3)), Some(&original)));
    assert!(r.update_buckets(Some(&buckets(1)), Some(&old)));
    assert_eq!(r.buckets().unwrap().version, 1);
    assert!(r.update_buckets(None, Some(&original)));
    assert!(r.buckets().is_none());
}
#[test]
fn core_task_34_absent_and_zero() {
    let mut r = region();
    assert!(r.compare_and_set_report_buckets(None));
    assert!(r.compare_and_set_report_buckets(Some(buckets(0))));
    assert!(r.compare_and_set_report_buckets(None));
    assert!(r.report_buckets().is_some()); // nil has version zero, equal is ignored
    assert!(r.update_buckets(None, None));
    assert!(r.buckets().is_none());
}
#[test]
fn core_task_34_meta_priority_and_clone() {
    let mut r = region();
    r.bucket_meta = Some(Arc::new(crate::RegionBucketMeta {
        version: 1,
        keys: vec![],
    }));
    let copy = r.clone();
    assert!(r.compare_and_set_report_buckets(Some(buckets(u64::MAX))));
    assert_eq!(r.buckets().unwrap().version, 1);
    assert_eq!(r.buckets().unwrap().region_id, r.meta().id);
    assert!(copy.report_buckets().is_none());
    assert_eq!(copy.buckets().unwrap().version, 1);
}
#[test]
fn core_task_34_competing_snapshots() {
    let mut r = region();
    assert!(r.compare_and_set_report_buckets(Some(buckets(1))));
    let old = r.report_buckets().unwrap().clone();
    let region = Arc::new(Mutex::new(r));
    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = [2, 3]
        .into_iter()
        .map(|version| {
            let (r, old, barrier) = (region.clone(), old.clone(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                r.lock()
                    .unwrap()
                    .update_buckets(Some(&buckets(version)), Some(&old))
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.into_iter().filter(|ok| *ok).count(), 1);
    assert!([2, 3].contains(&region.lock().unwrap().buckets().unwrap().version));
    let handles: Vec<_> = [2, 3, 4, 0, u64::MAX]
        .into_iter()
        .map(|version| {
            let r = region.clone();
            std::thread::spawn(move || {
                r.lock()
                    .unwrap()
                    .compare_and_set_report_buckets(Some(buckets(version)))
            })
        })
        .collect();
    for h in handles {
        assert!(h.join().unwrap());
    }
    assert_eq!(region.lock().unwrap().buckets().unwrap().version, u64::MAX);
}
