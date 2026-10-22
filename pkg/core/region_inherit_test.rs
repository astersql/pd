// Copyright 2026 AsterSQL.
use crate::*;
use serde::Deserialize;
use std::sync::Arc;
#[derive(Deserialize)]
struct Case {
    name: String,
    origin: Option<[i64; 2]>,
    current: [i64; 2],
    expected: [i64; 2],
    source: String,
}
fn region() -> RegionInfo {
    RegionInfo::new(
        RegionMeta {
            id: 100,
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    )
}
#[test]
fn core_task_33_size_keys_fixture() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("rust-fixtures/core_task_33.json")).unwrap();
    assert_eq!(cases.len(), 9);
    for c in cases {
        assert!(!c.source.is_empty());
        let origin = c.origin.map(|s| {
            let mut r = region();
            r.stats.approximate_size = s[0];
            r.stats.approximate_keys = s[1];
            r
        });
        let mut r = region();
        r.stats.approximate_size = c.current[0];
        r.stats.approximate_keys = c.current[1];
        r.inherit(origin.as_ref(), false);
        assert_eq!(
            [r.stats.approximate_size, r.stats.approximate_keys],
            c.expected,
            "{}",
            c.name
        );
    }
}
#[test]
fn core_task_33_sources_and_flow_remain_local() {
    // Source-derived: region.go RegionSource, LoadedFromStorage/Sync and
    // RegionFromHeartbeat; Inherit writes neither source nor flow counters.
    // These branches are Rust coverage, not an executed Go differential.
    let mut origin = region();
    origin.stats.read_bytes = 999;
    for source in [
        RegionSource::Storage,
        RegionSource::Sync,
        RegionSource::Heartbeat,
    ] {
        let mut r = region();
        r.set_source(source);
        r.stats.read_bytes = 17;
        r.stats.written_keys = 23;
        r.stats.flow_round_divisor = 10;
        r.inherit(Some(&origin), true);
        assert_eq!(r.source(), source);
        assert_eq!(r.loaded_from_storage(), source == RegionSource::Storage);
        assert_eq!(r.loaded_from_sync(), source == RegionSource::Sync);
        assert_eq!(
            (
                r.stats.read_bytes,
                r.stats.written_keys,
                r.stats.flow_round_divisor
            ),
            (17, 23, 10)
        );
    }
    let mut r = region();
    assert!(r.loaded_from_storage());
    r.set_heartbeat_stats(Default::default(), 0);
    assert_eq!(r.source(), RegionSource::Heartbeat);
}
#[test]
fn core_task_33_bucket_conditions_and_sharing() {
    // TestInherit asserts report version 1 is inherited with and without meta.
    // TestGetBucketMeta asserts meta version 1 wins over newer report buckets.
    // Disabled/absent-origin/existing-report combinations derive directly from
    // Inherit's conjunction. Arc identity preserves Go pointer sharing; these
    // extra combinations are not claimed as executed Go differential cases.
    let bucket = Arc::new(RegionBuckets {
        region_id: 100,
        version: 1,
        keys: vec![b"a".to_vec(), b"b".to_vec()],
    });
    let mut origin = region();
    origin.report_buckets = Some(bucket.clone());
    for enabled in [false, true] {
        for exists in [false, true] {
            for own in [false, true] {
                for meta in [false, true] {
                    let mut r = region();
                    let local = Arc::new(RegionBuckets {
                        version: 3,
                        ..(*bucket).clone()
                    });
                    if own {
                        r.report_buckets = Some(local.clone());
                    }
                    if meta {
                        r.bucket_meta = Some(Arc::new(RegionBucketMeta {
                            version: 2,
                            keys: vec![b"z".to_vec()],
                        }));
                    }
                    r.inherit(if exists { Some(&origin) } else { None }, enabled);
                    let expected = if own {
                        Some(&local)
                    } else if enabled && exists {
                        Some(&bucket)
                    } else {
                        None
                    };
                    match (r.report_buckets(), expected) {
                        (Some(a), Some(b)) => assert!(Arc::ptr_eq(a, b)),
                        (None, None) => {}
                        _ => panic!("bucket presence"),
                    }
                    if meta {
                        assert_eq!(r.buckets().unwrap().version, 2);
                        assert_eq!(r.buckets().unwrap().region_id, 100);
                    } else {
                        assert_eq!(r.buckets().map(|b| b.version), expected.map(|b| b.version));
                    }
                    let clone = r.clone_with([]);
                    assert_eq!(clone.buckets(), r.buckets());
                    assert_eq!(clone.source(), RegionSource::Storage);
                }
            }
        }
    }
    let mut r = region();
    r.inherit(Some(&region()), true);
    assert!(r.report_buckets().is_none());
}
