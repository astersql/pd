// Copyright 2026 AsterSQL.
use super::*;

#[test]
fn store_config_merge_go_table_and_boundaries() {
    let c = StoreConfig::default();
    for (size, merge, keys, merge_keys, pass) in [
        (116, 20, 1640000, 200000, true),
        (164, 20, 1640000, 200000, true),
        (146, 50, 1460000, 500000, false),
        (147, 50, 1470000, 500000, true),
        (192, 96, 1920000, 960000, true),
        (144, 48, 1440000, 480000, false),
        (143, 100, 1439999, 1000000, true),
    ] {
        assert_eq!(c.check_region_size(size, merge).is_ok(), pass);
        assert_eq!(c.check_region_keys(keys, merge_keys).is_ok(), pass);
    }
    assert_eq!(
        MergeAgainError::Size.to_string(),
        "[PD:checker:ErrCheckerMergeAgain]region will be merged again, the smallest region of the split regions is less than max-merge-region-size, it will be merged again"
    );
    assert_eq!(
        MergeAgainError::Keys.to_string(),
        "[PD:checker:ErrCheckerMergeAgain]region will be merged again, the smallest region of the split regions is less than max-merge-region-keys"
    );
    assert_eq!(c.check_region_size(144, 48), Err(MergeAgainError::Size));
    assert_eq!(
        c.check_region_keys(1440000, 480000),
        Err(MergeAgainError::Keys)
    );
}
#[test]
fn store_config_units_adjust_and_bucket() {
    let mut c = StoreConfig::default();
    assert_eq!(
        (
            c.get_region_max_size(),
            c.get_region_split_size(),
            c.get_region_bucket_size()
        ),
        (144, 96, 0)
    );
    c.set_region_bucket_enabled(true);
    assert_eq!(c.get_region_bucket_size(), 96);
    c.coprocessor.region_max_size = "1GiB".into();
    c.coprocessor.region_split_size = "100KiB".into();
    c.coprocessor.region_bucket_size = "bad".into();
    assert_eq!(c.get_region_max_size(), 0); // raw values do not auto-adjust
    c.adjust();
    assert_eq!(
        (
            c.get_region_max_size(),
            c.get_region_split_size(),
            c.get_region_bucket_size()
        ),
        (1024, 0, 96)
    );
    assert!(c.check_region_size(2000, 50).is_ok());
    c.coprocessor.region_bucket_size = "1.5MiB".into();
    c.adjust();
    assert_eq!(c.get_region_bucket_size(), 1);
    c.set_region_bucket_enabled(false);
    assert_eq!(c.get_region_bucket_size(), 0);
    c.coprocessor.region_max_keys = -1;
    assert_eq!(c.get_region_max_keys(), u64::MAX);
}
#[test]
fn store_config_clone_equal_readpool_and_engine() {
    let mut c = StoreConfig::default();
    c.adjust();
    let mut copy = c.clone();
    copy.region_max_size_mb = 999;
    assert_eq!(c, copy);
    copy.readpool.unified.max_thread_count = 12;
    assert_ne!(c, copy);
    assert_eq!(c.get_unified_read_pool_max_thread_count(), 0);
    assert_eq!(copy.get_unified_read_pool_max_thread_count(), 12);
    copy = c.clone();
    copy.storage.engine = RAFTSTORE_V2.into();
    assert!(copy.is_raft_kv2());
    assert!(!c.is_raft_kv2());
    assert_ne!(c, copy);
    copy.storage.engine = "RAFT-KV2".into();
    assert!(!copy.is_raft_kv2());
    copy = c.clone();
    copy.coprocessor.region_max_size = "144MiB".into();
    assert_ne!(c, copy);
}
#[test]
fn store_config_json_null_and_cache_exclusion() {
    let c: StoreConfig = serde_json::from_str(r#"{"coprocessor":{"region-max-size":null,"region-max-keys":null},"storage":null,"readpool":null}"#).unwrap();
    assert_eq!(c, StoreConfig::default());
    let mut c = c;
    c.adjust();
    let text = c.to_string();
    assert!(!text.contains("_mb"));
    assert_eq!(serde_json::from_str::<StoreConfig>(&text).unwrap(), c);
}
