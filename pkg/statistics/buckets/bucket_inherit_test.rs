// Copyright 2026 AsterSQL.
use crate::*;

fn item(id: u64, keys: &[&str], degrees: &[i32]) -> BucketTreeItem {
    assert_eq!(keys.len(), degrees.len() + 1);
    BucketTreeItem {
        region_id: id,
        start_key: keys[0].as_bytes().to_vec(),
        end_key: keys.last().unwrap().as_bytes().to_vec(),
        stats: keys
            .windows(2)
            .zip(degrees)
            .map(|(k, &d)| BucketStat {
                region_id: id,
                start_key: k[0].as_bytes().to_vec(),
                end_key: k[1].as_bytes().to_vec(),
                hot_degree: d,
                interval: 1000,
                loads: [id; 6],
            })
            .collect(),
        interval: 1000,
        version: 7,
        status: BucketStatus::Alive,
    }
}
fn degrees(b: &BucketTreeItem) -> Vec<i32> {
    b.stats.iter().map(|s| s.hot_degree).collect()
}
fn check(mut new: BucketTreeItem, old: &[BucketTreeItem], expected: &[i32]) {
    let before = new.clone();
    let origins = old.to_vec();
    new.inherit(old);
    assert_eq!(degrees(&new), expected);
    for (s, before) in new.stats.iter_mut().zip(&before.stats) {
        s.hot_degree = before.hot_degree;
    }
    assert_eq!(new, before, "inherit changes only hot_degree");
    assert_eq!(old, origins);
}

#[test]
fn state_task_06_bucket_inherit_go_matrix() {
    let old = [item(1, &["", "20", "50", ""], &[3, 2, 10])];
    for (keys, expected) in [
        (vec!["", "20", "30", "40", "50"], vec![3, 2, 2, 2]),
        (vec!["20", "45", "50"], vec![2, 2]),
        (vec!["00", "05"], vec![3]),
        (vec!["80", ""], vec![10]),
        (vec!["", ""], vec![10]),
    ] {
        check(item(1, &keys, &vec![0; expected.len()]), &old, &expected);
    }
}

#[test]
fn state_task_06_bucket_inherit_cold_hot_merge_and_existing_degree() {
    for (old_degrees, expected) in [
        (vec![-2, -8, -3], -8),
        (vec![-2, 0, -3], -3),
        (vec![-9, 2, 7], 7),
        (vec![0, 0, 0], 0),
        (vec![5, -9, 2], 5),
    ] {
        let old = [item(1, &["10", "20", "30", "40"], &old_degrees)];
        check(item(2, &["15", "35"], &[0]), &old, &[expected]);
    }
    let old = [item(1, &["10", "20"], &[-9]), item(2, &["20", "30"], &[5])];
    for (initial, expected) in [(-10, 5), (0, 5), (9, 9)] {
        check(item(3, &["10", "30"], &[initial]), &old, &[expected]);
    }
    check(item(3, &["10", "20"], &[3]), &old, &[3]);
    check(item(3, &["10", "20"], &[-10]), &old, &[-10]);
}

#[test]
fn state_task_06_bucket_inherit_disjoint_partial_and_empty() {
    let old = [item(1, &["20", "30"], &[7]), item(2, &["40", "50"], &[-5])];
    check(item(3, &["00", "10"], &[0]), &old, &[0]);
    check(item(3, &["50", "60"], &[0]), &old, &[0]);
    check(
        item(
            3,
            &["10", "20", "25", "30", "40", "45", "50", "60"],
            &[0; 7],
        ),
        &old,
        &[0, 7, 7, 0, -5, -5, 0],
    );
    check(item(3, &["10", "20"], &[2]), &[], &[2]);
    check(item(3, &["10"], &[]), &old, &[]);
    check(item(3, &["10", "20"], &[0]), &[item(1, &["10"], &[])], &[0]);
}

#[test]
fn state_task_06_bucket_inherit_archive_split_merge() {
    let mut index = BucketRangeIndex::new();
    let original = item(1, &["10", "20", "40", "60"], &[3, -4, 8]);
    index.put_item(original.clone());
    let mut split = item(2, &["25", "30", "35"], &[0, 0]);
    split.inherit(&index.get_buckets_by_key_range(&split.start_key, &split.end_key));
    assert_eq!(degrees(&split), [-4, -4]);
    index.put_item(split);
    assert_eq!(index.len(), 3);
    assert!(index.region(1).is_none());
    let mut merged = item(3, &["10", "15", "45", "60"], &[0; 3]);
    let sources = index.get_buckets_by_key_range(&merged.start_key, &merged.end_key);
    assert_eq!(
        sources
            .iter()
            .filter(|s| s.status == BucketStatus::Archive)
            .count(),
        2
    );
    merged.inherit(&sources);
    assert_eq!(degrees(&merged), [3, 8, 8]);
    index.put_item(merged.clone());
    assert_eq!((index.len(), index.region_count()), (1, 1));
    assert_eq!(index.region(3), Some(&merged));
    assert_eq!(degrees(&original), [3, -4, 8]);
}

#[test]
fn state_task_06_bucket_inherit_source_empty_end_comparisons() {
    // Keep the source early-return byte comparison, even for an unbounded new end.
    check(
        item(2, &["20", ""], &[0]),
        &[item(1, &["10", ""], &[9])],
        &[0],
    );
    // A finite new end compares greater than an empty old end: advance the old cursor.
    check(
        item(2, &["", "10", "20", "30"], &[0; 3]),
        &[item(1, &["", ""], &[9])],
        &[9, 0, 0],
    );
    // One source ending at infinity is encountered only after finite predecessors.
    check(
        item(2, &["", ""], &[0]),
        &[item(1, &["", "20", ""], &[-2, 9])],
        &[9],
    );
}
