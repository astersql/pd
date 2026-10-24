use super::*;

fn report() -> BucketReport {
    BucketReport {
        region_id: 1,
        version: 7,
        keys: vec![b"".to_vec(), b"20".to_vec(), b"".to_vec()],
        period_in_ms: 2000,
        flows: BucketFlows {
            read_bytes: vec![11, 21],
            read_keys: vec![12, 22],
            read_qps: vec![13, 23],
            write_bytes: vec![14, 24],
            write_keys: vec![15, 25],
            write_qps: vec![16, 26],
        },
    }
}

#[test]
fn state_task_04_bucket_values_normalize() {
    let item = convert_to_bucket_tree_item(&report());
    assert_eq!(item.stats.len(), 2);
    assert_eq!((item.region_id, item.version, item.interval), (1, 7, 2000));
    assert_eq!(item.status, BucketStatus::Alive);
    assert!(item.start_key.is_empty() && item.end_key.is_empty());
    assert_eq!(item.stats[0].loads, [5, 6, 6, 7, 7, 8]);
    assert_eq!(item.stats[1].loads, [10, 11, 11, 12, 12, 13]);
    assert_eq!(item.stats[0].end_key, b"20");
    assert_eq!(item.stats[1].start_key, b"20");
    for stat in &item.stats {
        assert_eq!(
            (stat.region_id, stat.hot_degree, stat.interval),
            (1, 0, 2000)
        );
    }
}

#[test]
fn state_task_04_bucket_values_intervals_and_wrap() {
    for (period, interval, rate) in [(0, 10000, 1), (1000, 1000, 11), (3, 3, 3666)] {
        let mut r = report();
        r.period_in_ms = period;
        let item = convert_to_bucket_tree_item(&r);
        assert_eq!(item.interval, interval);
        assert_eq!(item.stats[0].loads[0], rate);
    }
    let mut r = report();
    r.period_in_ms = 1;
    r.flows.read_bytes[0] = u64::MAX;
    assert_eq!(
        convert_to_bucket_tree_item(&r).stats[0].loads[0],
        u64::MAX - 999
    );
}

#[test]
fn state_task_04_bucket_values_deep_clone() {
    let mut r = report();
    let item = convert_to_bucket_tree_item(&r);
    assert_eq!(item.stats.len(), 2);
    let mut copy = item.clone();
    r.keys[1][0] = b'9';
    r.flows.read_bytes[0] = 999;
    copy.stats[0].loads[0] = 999;
    copy.stats[0].end_key[0] = b'8';
    copy.stats[1].start_key[0] = b'7';
    copy.start_key.push(b'x');
    copy.end_key.push(b'z');
    copy.stats.pop();
    assert_eq!(item, convert_to_bucket_tree_item(&report()));
    assert_eq!(item.stats[0].end_key, b"20");
    assert_eq!(item.stats[1].start_key, b"20");
}

#[test]
fn state_task_04_bucket_values_array_boundaries() {
    let mut r = report();
    r.keys.clear();
    assert!(std::panic::catch_unwind(|| convert_to_bucket_tree_item(&r)).is_err());
    r.keys = vec![b"x".to_vec()];
    r.flows = BucketFlows::default();
    let item = convert_to_bucket_tree_item(&r);
    assert!(item.stats.is_empty());
    assert_eq!(item.start_key, b"x");
    assert_eq!(item.end_key, b"x");
    for dim in 0..6 {
        let mut r = report();
        match dim {
            0 => r.flows.read_bytes.clear(),
            1 => r.flows.read_keys.clear(),
            2 => r.flows.read_qps.clear(),
            3 => r.flows.write_bytes.clear(),
            4 => r.flows.write_keys.clear(),
            _ => r.flows.write_qps.clear(),
        }
        assert!(std::panic::catch_unwind(|| convert_to_bucket_tree_item(&r)).is_err());
    }
    let mut r = report();
    r.flows.read_bytes.push(999);
    assert_eq!(
        convert_to_bucket_tree_item(&r),
        convert_to_bucket_tree_item(&report())
    );
}

#[test]
fn state_task_04_bucket_values_go_conversion() {
    let counts = vec![1, 2, 3, 4];
    let report = BucketReport {
        region_id: 1,
        version: 0,
        keys: (b'1'..=b'5').map(|key| vec![key]).collect(),
        period_in_ms: 1000,
        flows: BucketFlows {
            read_bytes: counts.clone(),
            read_keys: counts.clone(),
            read_qps: counts.clone(),
            write_bytes: counts.clone(),
            write_keys: counts.clone(),
            write_qps: counts,
        },
    };
    let item = convert_to_bucket_tree_item(&report);
    assert_eq!(item.start_key, b"1");
    assert_eq!(item.end_key, b"5");
    assert_eq!((item.region_id, item.version, item.stats.len()), (1, 0, 4));
    for (i, stat) in item.stats.iter().enumerate() {
        assert_eq!(stat.loads, [(i + 1) as u64; 6]);
    }
}

#[test]
fn state_task_04_bucket_values_runtime_snapshots() {
    use serde_json::Value;
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../../tests/rust-compat/fixtures/state/task-04.json"
    ))
    .unwrap();
    for fixture in fixtures.as_array().unwrap() {
        let vectors: Vec<Vec<u64>> = serde_json::from_value(fixture["counters"].clone()).unwrap();
        let report = BucketReport {
            region_id: 1,
            version: 0,
            keys: serde_json::from_value(fixture["keys"].clone()).unwrap(),
            period_in_ms: fixture["period"].as_u64().unwrap(),
            flows: BucketFlows {
                read_bytes: vectors[0].clone(),
                read_keys: vectors[1].clone(),
                read_qps: vectors[2].clone(),
                write_bytes: vectors[3].clone(),
                write_keys: vectors[4].clone(),
                write_qps: vectors[5].clone(),
            },
        };
        let item = convert_to_bucket_tree_item(&report);
        assert_eq!(item.interval, fixture["interval"].as_u64().unwrap());
        assert_eq!(item.stats.len(), fixture["loads"].as_array().unwrap().len());
        for (i, stat) in item.stats.iter().enumerate() {
            assert_eq!(
                serde_json::json!(stat.loads),
                fixture["loads"][i],
                "{}",
                fixture["name"]
            );
            assert_eq!(serde_json::json!(stat.start_key), fixture["starts"][i]);
            assert_eq!(serde_json::json!(stat.end_key), fixture["ends"][i]);
        }
    }
}
