// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_codec::{TXN_KEYSPACE_MODE_PREFIX, encode_keyspace_boundary};
use astersql_pd_core::{KeyRange, RegionEpoch, RegionInfo, RegionMeta, RegionsInfo};
use serde_json::Value;
use std::{sync::Arc, time::Duration};

fn put(regions: &mut RegionsInfo, id: u64, version: Option<u64>, start: Vec<u8>, end: Vec<u8>) {
    regions.set_region(Arc::new(RegionInfo::new(
        RegionMeta {
            id,
            epoch: version.map(|version| RegionEpoch {
                version,
                conf_ver: 99,
            }),
            key_range: KeyRange {
                start_key: start,
                end_key: end,
            },
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    )));
}
fn record() -> SplitScatterObserver {
    let mut state = SplitScatterObserver::default();
    state.record_batch(100, 6, &[101, 102], Duration::from_secs(1800), true);
    state
}
#[test]
fn task_053_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_053.json"
    ))
    .unwrap();
    for c in fixture["cases"].as_array().unwrap() {
        let state = record();
        let mut regions = RegionsInfo::new();
        for (field, id, start, end) in [
            ("source", 100, b"a".to_vec(), b"m".to_vec()),
            ("result", 101, b"m".to_vec(), b"t".to_vec()),
        ] {
            if !c[field].is_null() {
                put(&mut regions, id, c[field].as_u64(), start, end);
            }
        }
        let observed = observe_split_scatter_result(&state.pending(101).unwrap(), &regions);
        assert_eq!(
            format!("{observed:?}"),
            c["expected"].as_str().unwrap(),
            "{}",
            c["name"]
        );
        assert_eq!(
            state.ready(&regions).iter().any(|p| p.region_id == 101),
            c["expected"] == "Ready"
        );
        assert_eq!(state.len(), 3);
    }
    assert_eq!(split_scatter_region_version(None), 0);
    println!("task_053: 13 frozen version/missing cases");
}
#[test]
fn task_053_partial_and_repeated_batches() {
    let mut state = record();
    let mut regions = RegionsInfo::new();
    put(&mut regions, 100, Some(5), b"a".to_vec(), b"m".to_vec());
    put(&mut regions, 101, Some(6), b"m".to_vec(), b"t".to_vec());
    assert!(state.ready(&regions).is_empty());
    put(&mut regions, 100, Some(6), b"a".to_vec(), b"m".to_vec());
    assert_eq!(
        state
            .ready(&regions)
            .iter()
            .map(|p| p.region_id)
            .collect::<Vec<_>>(),
        vec![100, 101]
    );
    put(&mut regions, 100, Some(7), b"a".to_vec(), b"m".to_vec());
    assert_eq!(
        state
            .ready(&regions)
            .iter()
            .map(|p| p.region_id)
            .collect::<Vec<_>>(),
        vec![101]
    );
    let old = state.pending(101).unwrap();
    state.record_batch(100, 6, &[101, 102], Duration::from_secs(1801), true);
    assert!(!state.is_current(&old));
    assert!(!state.remove_if_current(&old));
    let old = state.pending(101).unwrap();
    state.record_batch(100, 7, &[102, 101], Duration::from_secs(1801), true);
    assert!(!state.is_current(&old));
    assert!(!state.remove_if_current(&old));
    let current = state.pending(101).unwrap();
    assert_eq!(current.group, "split-scatter-100-102");
    assert_eq!(current.split_version, 7);
    assert!(state.remove_if_current(&current));
    assert!(!state.remove_if_current(&current));
    state.record_batch(200, 0, &[201, 201], Duration::ZERO, true);
    assert_eq!(state.pending(201).unwrap().split_version, 1);
    assert_eq!(state.len(), 4);
    state.record_batch(300, 1, &[], Duration::ZERO, true);
    state.record_batch(300, 1, &[301], Duration::ZERO, false);
    assert_eq!(state.len(), 4);
}
#[test]
fn task_053_txn_bounds() {
    for id in [0, 1, 255, 65535, 0xffffff] {
        let mut regions = RegionsInfo::new();
        let left = encode_keyspace_boundary(TXN_KEYSPACE_MODE_PREFIX, id).to_vec();
        let right = encode_keyspace_boundary(TXN_KEYSPACE_MODE_PREFIX, id + 1).to_vec();
        // A containing region alone is insufficient, even when its end is a bound.
        put(&mut regions, 1, Some(1), vec![], right.clone());
        assert!(!has_split_scatter_txn_keyspace_bounds(&regions, id));
        put(&mut regions, 2, Some(1), left.clone(), right.clone());
        assert!(!has_split_scatter_txn_keyspace_bounds(&regions, id));
        put(&mut regions, 3, Some(1), right, vec![]);
        assert!(has_split_scatter_txn_keyspace_bounds(&regions, id));
        let mut raw =
            astersql_pd_codec::make_keyspace_prefix(TXN_KEYSPACE_MODE_PREFIX, id).to_vec();
        raw.extend(astersql_pd_codec::generate_table_key(42));
        let region = RegionInfo::new(
            RegionMeta {
                key_range: KeyRange {
                    start_key: astersql_pd_codec::encode_bytes(&raw),
                    end_key: vec![],
                },
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        );
        let validate = |id| has_split_scatter_txn_keyspace_bounds(&regions, id);
        assert_eq!(
            resolve_split_scatter_range_hint(&region, Some(&validate)).scatter_group,
            format!("split-scatter-keyspace-{id}-table-42")
        );
    }
    println!("task_053: 5 keyspaces x 3 boundary states plus grouping integration");
}

#[test]
fn task_053_identity_fields() {
    let mut state = record();
    let current = state.pending(101).unwrap();
    for field in 0..5 {
        let mut stale = current.clone();
        match field {
            0 => stale.region_id = 999,
            1 => stale.group.push_str("-old"),
            2 => stale.source_region_id += 1,
            3 => stale.split_version += 1,
            _ => stale.expire_at += Duration::from_nanos(1),
        }
        assert!(!state.is_current(&stale));
        assert!(!state.remove_if_current(&stale));
        assert_eq!(state.pending(101), Some(current.clone()));
    }
    state.record_batch(100, 6, &[101, 102], current.expire_at, true);
    assert!(state.is_current(&current));
    assert!(state.remove_if_current(&current));
    assert_eq!(state.len(), 2);
    println!("task_053: 5 independent stale identity fields and identical rerecord");
}
