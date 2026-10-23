// Copyright 2026 AsterSQL.

use crate::{ConfigMetaData, adjust_bool, adjust_int64, adjust_string};

#[test]
fn config_metadata_explicit_zero_values_and_missing() {
    let metadata = ConfigMetaData::from_defined_paths([["enabled"], ["count"], ["name"]]);
    let (mut enabled, mut count, mut name) = (false, 0, String::new());
    if !metadata.is_defined("enabled") {
        adjust_bool(&mut enabled, true);
    }
    if !metadata.is_defined("count") {
        adjust_int64(&mut count, 7);
    }
    if !metadata.is_defined("name") {
        adjust_string(&mut name, "fallback");
    }
    assert!(!enabled);
    assert_eq!(count, 0);
    assert_eq!(name, "");
    for key in ["enabled", "count", "name"] {
        assert!(metadata.is_defined(key), "explicit field {key}");
    }
    assert!(!metadata.is_defined("missing"));
    let mut missing = 0;
    if !metadata.is_defined("missing") {
        adjust_int64(&mut missing, 7);
    }
    assert_eq!(missing, 7);
}

#[test]
fn config_metadata_nested_prefixes_and_missing_paths() {
    let metadata = ConfigMetaData::from_defined_paths([
        vec!["server", "limits", "count"],
        vec!["server", "enabled"],
        vec!["empty-table"],
    ]);
    assert!(metadata.is_defined("server"));
    assert!(metadata.is_defined("empty-table"));
    assert!(!metadata.is_defined("count"));
    assert!(metadata.child(["server"]).is_defined("enabled"));
    assert!(metadata.child(["server", "limits"]).is_defined("count"));
    assert!(
        metadata
            .child(["server"])
            .child(["limits"])
            .is_defined("count")
    );
    assert!(!metadata.child(["missing"]).is_defined("count"));
    assert!(!metadata.child(["empty-table"]).is_defined("count"));
    assert!(!metadata.child(["server", "enabled"]).is_defined("count"));
}

#[test]
fn config_metadata_child_does_not_mutate_parent_or_siblings() {
    let metadata = ConfigMetaData::from_defined_paths([
        ["a", "x", "value"],
        ["a", "y", "other"],
        ["b", "x", "other"],
    ]);
    let a = metadata.child(["a"]);
    let x = a.child(["x"]);
    let y = a.child(["y"]);
    assert!(x.is_defined("value"));
    assert!(!x.is_defined("other"));
    assert!(y.is_defined("other"));
    assert!(!y.is_defined("value"));
    assert!(a.is_defined("x"));
    assert!(a.is_defined("y"));
    assert!(!a.is_defined("value"));
    assert!(metadata.is_defined("a"));
    assert!(metadata.is_defined("b"));
    assert!(!metadata.is_defined("x"));
    assert!(a.child(Vec::<String>::new()).is_defined("x"));
    drop(metadata);
    drop(a);
    assert!(x.is_defined("value"));
}

#[test]
fn config_metadata_literal_segments_case_and_empty_keys() {
    let metadata = ConfigMetaData::from_defined_paths([
        vec!["a.b"],
        vec!["a", "b", "c"],
        vec![""],
        vec!["nested", ""],
        vec!["中文", "值"],
        vec!["Case"],
    ]);
    assert!(metadata.is_defined("a.b"));
    assert!(metadata.child(["a"]).is_defined("b"));
    assert!(!metadata.child(["a.b"]).is_defined("c"));
    assert!(metadata.child(["a", "b"]).is_defined("c"));
    assert!(metadata.is_defined(""));
    assert!(metadata.child(["nested"]).is_defined(""));
    assert!(metadata.child(["中文"]).is_defined("值"));
    assert!(metadata.is_defined("Case"));
    assert!(!metadata.is_defined("case"));
}

#[test]
fn config_metadata_absent_and_empty_metadata() {
    for metadata in [
        ConfigMetaData::default(),
        ConfigMetaData::from_defined_paths(Vec::<Vec<String>>::new()),
        ConfigMetaData::from_defined_paths([Vec::<String>::new()]),
    ] {
        assert!(!metadata.is_defined(""));
        assert!(!metadata.is_defined("missing"));
        assert!(!metadata.child(["a", "b"]).is_defined("c"));
        assert!(!metadata.child(Vec::<String>::new()).is_defined(""));
    }
}
