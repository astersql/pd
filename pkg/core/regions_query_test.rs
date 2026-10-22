// Copyright 2026 AsterSQL.
use crate::*;
use std::sync::Arc;
fn registry() -> RegionsInfo {
    let mut r = RegionsInfo::new();
    for (id, start, end) in [(1, b"a", b"b"), (2, b"b", b"c"), (3, b"d", b"e")] {
        let mut region = RegionInfo::new(
            RegionMeta {
                id,
                key_range: KeyRange {
                    start_key: start.to_vec(),
                    end_key: end.to_vec(),
                },
                ..Default::default()
            },
            Some(Peer {
                id: 10,
                store_id: 1,
                ..Default::default()
            }),
            vec![],
            vec![Peer {
                id: 11,
                ..Default::default()
            }],
        );
        region.update_buckets(Some(&RegionBuckets::default()), None);
        r.set_region(Arc::new(region));
    }
    r
}
#[test]
fn core_task_50_mixed_fixture() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_50.json")).unwrap();
    let keys: Vec<_> = f["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap().as_bytes())
        .collect();
    let prev: Vec<_> = f["prev_keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap().as_bytes())
        .collect();
    let r = registry();
    let q = r.query_regions(&keys, &prev, &[1, 3, 4, 4, 0], false);
    assert_eq!(serde_json::to_value(&q.key_ids).unwrap(), f["key_ids"]);
    assert_eq!(
        serde_json::to_value(&q.prev_key_ids).unwrap(),
        f["prev_ids"]
    );
    assert_eq!(q.regions_by_id.len(), 4);
    assert!(q.regions_by_id[&4].is_none());
    for id in [1, 2, 3] {
        let v = q.regions_by_id[&id].as_ref().unwrap();
        assert_eq!(v.region.id, id);
        assert_eq!(v.leader.as_ref().unwrap().id, 10);
        assert_eq!(v.pending_peers[0].id, 11);
        assert!(v.down_peers.is_empty());
        assert!(v.buckets.is_none());
    }
}
#[test]
fn core_task_50_batch_boundaries_duplicates_and_buckets() {
    let r = registry();
    for n in [127, 128, 129, 257] {
        let keys: Vec<&[u8]> = (0..n)
            .map(|i| {
                if i % 2 == 0 {
                    b"b".as_slice()
                } else {
                    b"c".as_slice()
                }
            })
            .collect();
        let ids: Vec<_> = (0..n).map(|i| if i % 2 == 0 { 3 } else { 4 }).collect();
        let q = r.query_regions(&keys, &keys, &ids, true);
        assert_eq!(
            q.key_ids,
            (0..n)
                .map(|i| if i % 2 == 0 { 2 } else { 0 })
                .collect::<Vec<_>>()
        );
        assert_eq!(
            q.prev_key_ids,
            (0..n)
                .map(|i| if i % 2 == 0 { 1 } else { 0 })
                .collect::<Vec<_>>()
        );
        assert_eq!(q.regions_by_id.len(), 4);
        assert!(q.regions_by_id[&4].is_none());
        for id in [1, 2, 3] {
            assert!(q.regions_by_id[&id].as_ref().unwrap().buckets.is_some());
        }
    }
}
#[test]
fn core_task_50_empty_and_no_candidates() {
    for r in [RegionsInfo::new(), registry()] {
        let q = r.query_regions(&[], &[], &[], false);
        assert!(q.key_ids.is_empty() && q.prev_key_ids.is_empty() && q.regions_by_id.is_empty());
        let q = r.query_regions(&[b"z"], &[b"z"], &[0, 99, 99], false);
        assert_eq!(q.key_ids, [0]);
        assert_eq!(q.prev_key_ids, [0]);
        assert_eq!(q.regions_by_id.len(), 1);
        assert!(q.regions_by_id[&99].is_none());
    }
}

#[test]
fn core_task_50_distinct_ids_across_batches() {
    let mut r = RegionsInfo::new();
    for id in 1u64..=257 {
        let start_key = id.to_be_bytes().to_vec();
        let end_key = (id + 1).to_be_bytes().to_vec();
        r.set_region(Arc::new(RegionInfo::new(
            RegionMeta {
                id,
                key_range: KeyRange { start_key, end_key },
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        )));
    }
    let ids: Vec<_> = (1..=258).rev().collect();
    let keys: Vec<_> = ids.iter().map(|id| u64::to_be_bytes(*id)).collect();
    let refs: Vec<_> = keys.iter().map(|key| key.as_slice()).collect();
    let q = r.query_regions(&refs, &refs, &ids, true);
    assert_eq!(
        q.key_ids,
        ids.iter()
            .map(|&id| if id == 258 { 0 } else { id })
            .collect::<Vec<_>>()
    );
    assert_eq!(
        q.prev_key_ids,
        ids.iter()
            .map(|&id| if id == 258 { 0 } else { id - 1 })
            .collect::<Vec<_>>()
    );
    assert_eq!(q.regions_by_id.len(), 258);
    assert!(q.regions_by_id[&258].is_none());
    for id in 1..=257 {
        assert_eq!(q.regions_by_id[&id].as_ref().unwrap().region.id, id);
    }
}
