// Copyright 2026 AsterSQL.
use crate::*;

fn item(id: u64, keys: &[&[u8]], flow: u64) -> BucketTreeItem {
    let counters = vec![flow; keys.len() - 1];
    convert_to_bucket_tree_item(&BucketReport {
        region_id: id,
        version: 1,
        keys: keys.iter().map(|key| key.to_vec()).collect(),
        period_in_ms: 1000,
        flows: BucketFlows {
            read_bytes: counters.clone(),
            read_keys: counters.clone(),
            read_qps: counters.clone(),
            write_bytes: counters.clone(),
            write_keys: counters.clone(),
            write_qps: counters,
        },
    })
}

// Compose the existing synchronous domain stages in the Go report order.
fn report(index: &mut BucketRangeIndex, mut new: BucketTreeItem) {
    let origins = match index.region(new.region_id) {
        Some(old) if old.start_key == new.start_key && old.end_key == new.end_key => {
            vec![old.clone()]
        }
        _ => index.get_buckets_by_key_range(&new.start_key, &new.end_key),
    };
    new.inherit(&origins);
    new.calculate_hot_degree();
    index.put_item(new);
}

#[test]
fn state_task_07_bucket_degree_go_calculate() {
    let mut value = item(1, &[b"010", b"100"], 0);
    value.calculate_hot_degree();
    assert_eq!(value.stats[0].hot_degree, -1);
    value.stats[0].loads = [8193, 129, 0, 0, 0, 0];
    value.calculate_hot_degree();
    assert_eq!(value.stats[0].hot_degree, 0);
    value.stats[0].loads = [0, 0, 0, 1025, 33, 0];
    value.calculate_hot_degree();
    assert_eq!(value.stats[0].hot_degree, 1);
}

#[test]
fn state_task_07_bucket_degree_thresholds_and_bounds() {
    for (dim, threshold) in [(0, 8192), (1, 128), (3, 1024), (4, 32)] {
        let mut value = item(1, &[b"a", b"b"], 0);
        value.stats[0].loads[dim] = threshold;
        let before = value.clone();
        value.calculate_hot_degree();
        assert_eq!(value.stats[0].hot_degree, -1, "equal threshold dim {dim}");
        let mut expected = before;
        expected.stats[0].hot_degree = -1;
        assert_eq!(value, expected, "only degree changes");
        value.stats[0].loads[dim] += 1;
        value.calculate_hot_degree();
        assert_eq!(value.stats[0].hot_degree, 0, "single hot dim {dim}");
        value.stats[0].hot_degree = MAX_HOT_DEGREE;
        value.calculate_hot_degree();
        assert_eq!(value.stats[0].hot_degree, 20);
        value.stats[0].loads = [0, 0, u64::MAX, 0, 0, u64::MAX];
        value.stats[0].hot_degree = MIN_HOT_DEGREE;
        value.calculate_hot_degree();
        assert_eq!(value.stats[0].hot_degree, -20, "QPS alone is cold");
        value.stats[0].hot_degree = i32::MIN;
        value.calculate_hot_degree();
        assert_eq!(value.stats[0].hot_degree, i32::MIN);
        value.stats[0].loads[dim] = threshold + 1;
        value.stats[0].hot_degree = i32::MAX;
        value.calculate_hot_degree();
        assert_eq!(value.stats[0].hot_degree, i32::MAX);
    }
    BucketTreeItem::default().calculate_hot_degree();
}

#[test]
fn state_task_07_bucket_degree_go_repeated_and_transition() {
    let mut index = BucketRangeIndex::new();
    for (id, keys, flow, direction) in [
        (1, [b"10".as_slice(), b"20".as_slice()], 0, -1),
        (2, [b"20".as_slice(), b"30".as_slice()], u64::MAX, 1),
    ] {
        for step in 1..=25 {
            report(&mut index, item(id, &keys, flow));
            let stats = index.get_hot_bucket_stats(-20, &[]);
            assert_eq!(stats[&id][0].hot_degree, direction * step.min(20));
        }
    }
    let frozen = index.get_hot_bucket_stats(-20, &[]);
    for step in 1..=45 {
        report(&mut index, item(1, &[b"10", b"20"], u64::MAX));
        report(&mut index, item(2, &[b"20", b"30"], 0));
        let stats = index.get_hot_bucket_stats(-20, &[]);
        assert_eq!(stats[&1][0].hot_degree, (-20 + step).min(20));
        assert_eq!(stats[&2][0].hot_degree, (20 - step).max(-20));
    }
    assert_eq!(frozen[&1][0].hot_degree, -20);
    assert_eq!(frozen[&2][0].hot_degree, 20);
}

#[test]
fn state_task_07_bucket_degree_go_collect() {
    let mut index = BucketRangeIndex::new();
    assert!(index.get_hot_bucket_stats(-100, &[]).is_empty());
    for id in 0..10 {
        let start = (id * 10).to_string();
        let end = ((id + 1) * 10).to_string();
        index.put_item(item(id, &[start.as_bytes(), end.as_bytes()], 0));
    }
    assert_eq!(index.get_hot_bucket_stats(-100, &[]).len(), 10);
    assert!(index.get_hot_bucket_stats(1, &[]).is_empty());
}

#[test]
fn state_task_07_bucket_degree_query_snapshot_and_archive() {
    let mut index = BucketRangeIndex::new();
    let mut value = item(1, &[b"10", b"20", b"30", b"40"], 7);
    for (stat, degree) in value.stats.iter_mut().zip([-20, 0, 20]) {
        stat.hot_degree = degree;
    }
    index.put_item(value.clone());
    let expected = value.stats[1..].to_vec();
    let mut snapshot = index.get_hot_bucket_stats(0, &[1, 999, 1]);
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[&1], expected);
    assert!(index.get_hot_bucket_stats(21, &[]).is_empty());
    assert!(index.get_hot_bucket_stats(-100, &[999]).is_empty());
    snapshot.get_mut(&1).unwrap()[0].start_key.push(0xff);
    snapshot.get_mut(&1).unwrap()[0].end_key.clear();
    snapshot.get_mut(&1).unwrap()[0].loads[0] = u64::MAX;
    snapshot.get_mut(&1).unwrap()[0].hot_degree = -100;
    assert_eq!(index.region(1), Some(&value));
    assert_eq!(index.get_hot_bucket_stats(0, &[1])[&1], expected);
    // A new middle region removes the old current report, but retains archives.
    report(&mut index, item(2, &[b"20", b"30"], 0));
    assert_eq!(index.len(), 3);
    assert_eq!(index.region_count(), 1);
    assert!(index.get_hot_bucket_stats(-100, &[1]).is_empty());
    let current = index.get_hot_bucket_stats(-100, &[]);
    assert_eq!(current.len(), 1);
    assert_eq!(current[&2][0].hot_degree, -1);
}
