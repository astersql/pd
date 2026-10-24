// Copyright 2026 AsterSQL.
use crate::*;

fn item(id: u64, version: u64, keys: &[&str]) -> BucketTreeItem {
    let flows = vec![10; keys.len() - 1];
    convert_to_bucket_tree_item(&BucketReport {
        region_id: id,
        version,
        keys: keys.iter().map(|k| k.as_bytes().to_vec()).collect(),
        period_in_ms: 1000,
        flows: BucketFlows {
            read_bytes: flows.clone(),
            read_keys: flows.clone(),
            read_qps: flows.clone(),
            write_bytes: flows.clone(),
            write_keys: flows.clone(),
            write_qps: flows,
        },
    })
}

#[test]
fn state_task_05_bucket_range_go_put_sequence() {
    let mut index = BucketRangeIndex::new();
    for (id, version, keys, regions, ranges) in [
        (1, 0, vec!["", ""], 1, 1),
        (1, 2, vec!["10", "20", "30"], 1, 3),
        (2, 0, vec!["15", "20"], 1, 5),
        (1, 3, vec!["20", ""], 2, 4),
        (3, 0, vec!["", "20"], 2, 2),
        (4, 0, vec!["", ""], 1, 1),
    ] {
        index.put_item(item(id, version, &keys));
        assert_eq!(index.region_count(), regions);
        assert_eq!(index.len(), ranges);
        assert_eq!(index.region(id).unwrap().version, version);
        assert!(!index.get_buckets_by_key_range(b"10", b"").is_empty());
    }
}

#[test]
fn state_task_05_bucket_range_go_queries_and_adjacency() {
    let mut index = BucketRangeIndex::new();
    for (id, keys) in [(1, ["", "015"]), (2, ["015", "020"]), (3, ["020", ""])] {
        index.put_item(item(id, 1, &keys));
    }
    for (start, end, ids) in [
        ("", "100", vec![1, 2, 3]),
        ("030", "100", vec![3]),
        ("010", "030", vec![1, 2, 3]),
        ("015", "020", vec![2]),
        ("001", "", vec![1, 2, 3]),
        ("020", "020", vec![]),
    ] {
        assert_eq!(
            index
                .get_buckets_by_key_range(start.as_bytes(), end.as_bytes())
                .iter()
                .map(|i| i.region_id)
                .collect::<Vec<_>>(),
            ids
        );
    }
    assert_eq!(index.region_count(), 3);
    let mut finite = BucketRangeIndex::new();
    finite.put_item(item(1, 1, &["010", "020"]));
    finite.put_item(item(2, 1, &["020", "030"]));
    assert_eq!(finite.len(), 2);
    assert_eq!(finite.region_count(), 2);
    assert!(finite.get_buckets_by_key_range(b"030", b"").is_empty());
    assert!(finite.get_buckets_by_key_range(b"", b"010").is_empty());
}

#[test]
fn state_task_05_bucket_range_go_clone_and_owned_stats() {
    let mut original = item(1, 7, &["010", "020", "100"]);
    original.stats[0].hot_degree = 5;
    for (start, end, expected) in [
        ("010", "100", vec![("010", "020"), ("020", "100")]),
        ("000", "010", vec![]),
        ("100", "200", vec![]),
        ("000", "020", vec![("010", "020")]),
        ("015", "095", vec![("015", "020"), ("020", "095")]),
        ("015", "200", vec![("015", "020"), ("020", "100")]),
    ] {
        let mut copy = original.clone_by_range(start.as_bytes(), end.as_bytes());
        assert_eq!(copy.status, BucketStatus::Archive);
        assert_eq!((copy.region_id, copy.version, copy.interval), (1, 7, 1000));
        assert_eq!(
            copy.stats
                .iter()
                .map(|s| (s.start_key.clone(), s.end_key.clone()))
                .collect::<Vec<_>>(),
            expected
                .iter()
                .map(|(a, b)| (a.as_bytes().to_vec(), b.as_bytes().to_vec()))
                .collect::<Vec<_>>()
        );
        if let Some(stat) = copy.stats.first_mut() {
            stat.loads[0] = 99;
            stat.start_key.push(0);
        }
        assert_eq!(original.stats[0].loads[0], 10);
        assert_eq!(original.stats[0].start_key, b"010");
    }
}

#[test]
fn state_task_05_bucket_range_double_debris_and_archive_index() {
    let mut index = BucketRangeIndex::new();
    let mut old = item(1, 8, &["010", "020", "080", "100"]);
    old.stats[1].hot_degree = 9;
    index.put_item(old.clone());
    index.put_item(item(2, 1, &["030", "060"]));
    let parts = index.get_buckets_by_key_range(b"", b"");
    assert_eq!(parts.len(), 3);
    assert_eq!(parts[0], old.clone_by_range(b"010", b"030"));
    assert_eq!(parts[2], old.clone_by_range(b"060", b"100"));
    assert_eq!(parts[0].stats[1].hot_degree, 9);
    assert_eq!(parts[2].stats[0].hot_degree, 9);
    assert!(index.region(1).is_none());
    index.put_item(item(1, 9, &["110", "120"]));
    index.put_item(item(3, 1, &["010", "025"])); // covers archive of region 1
    assert_eq!(index.region(1).unwrap().start_key, b"110");
    assert_eq!(index.region_count(), 3);
    let snapshot = index.get_buckets_by_key_range(b"110", b"120");
    let mut replacement = item(1, 10, &["110", "115", "120"]);
    replacement.stats[0].hot_degree = -7;
    let count = index.len();
    index.put_item(replacement.clone());
    assert_eq!(index.len(), count);
    assert_eq!(index.region_count(), 3);
    assert_eq!(
        index.get_buckets_by_key_range(b"110", b"120"),
        vec![replacement.clone()]
    );
    assert_eq!(index.region(1), Some(&replacement));
    assert_eq!(snapshot[0].version, 9);
    index.put_item(item(4, 1, &["", ""]));
    assert_eq!((index.len(), index.region_count()), (1, 1));
}

#[test]
fn state_task_05_bucket_range_empty_end_factory_contract() {
    let old = item(1, 1, &["", "020", ""]);
    let debris = bucket_debris_factory(b"010", b"030", &old);
    assert_eq!(debris.len(), 2);
    assert_eq!(
        (&debris[0].start_key[..], &debris[0].end_key[..]),
        (&b""[..], &b"010"[..])
    );
    assert_eq!(
        (&debris[1].start_key[..], &debris[1].end_key[..]),
        (&b"030"[..], &b""[..])
    );
    assert_eq!(debris[0].stats.len(), 1);
    assert!(debris[1].stats.is_empty()); // source clone uses ordinary min then strict comparison
    assert!(bucket_debris_factory(b"200", b"300", &item(2, 1, &["010", "100"])).is_empty());
    let mut index = BucketRangeIndex::new();
    index.put_item(old);
    index.put_item(item(2, 1, &["", ""]));
    assert_eq!((index.len(), index.region_count()), (1, 1));
    index.put_item(item(2, 2, &["", ""]));
    assert_eq!((index.len(), index.region_count()), (1, 1));
    assert_eq!(index.get_buckets_by_key_range(b"", b"")[0].version, 2);
}
