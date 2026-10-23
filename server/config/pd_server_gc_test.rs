// Copyright 2026 AsterSQL.

use crate::{PDServerConfig, decode_toml_pd_server};
use astersql_pd_configutil::ConfigMetaData;

const INDEX: &str = "gc-keyspace-index-load-concurrency";
const WARMUP: &str = "gc-state-cache-warmup-concurrency";

fn defaults() -> PDServerConfig {
    let mut config = PDServerConfig::default();
    config.adjust_defaults(&ConfigMetaData::default());
    config
}

#[test]
fn pd_server_gc_defaults_and_valid_combinations() {
    let config = defaults();
    assert_eq!(config.gc_keyspace_index_load_concurrency, 4);
    assert_eq!(config.gc_state_cache_warmup_concurrency, 4);
    assert_eq!(config.validate_gc_concurrency(), Ok(()));
    for index in 1..=16 {
        for warmup in 1..=16 {
            let config = PDServerConfig {
                gc_keyspace_index_load_concurrency: index,
                gc_state_cache_warmup_concurrency: warmup,
                ..defaults()
            };
            let before = config.clone();
            assert_eq!(config.validate_gc_concurrency(), Ok(()));
            assert_eq!(config, before);
        }
    }
}

#[test]
fn pd_server_gc_explicit_boundaries_and_errors() {
    for name in [INDEX, WARMUP] {
        for value in [i64::MIN, -1, 0, 1, 2, 4, 16, 17, 128, i64::MAX] {
            let (mut config, metadata) =
                decode_toml_pd_server(&format!("{name} = {value}\n")).unwrap();
            config.adjust_defaults(&metadata);
            let before = config.clone();
            let result = config.validate_gc_concurrency();
            if (1..=16).contains(&value) {
                assert_eq!(result, Ok(()));
                assert_eq!(serde_json::to_value(&config).unwrap()[name], value);
            } else {
                assert_eq!(
                    result,
                    Err(format!("{name} must be between 1 and 16").as_str())
                );
            }
            assert_eq!(config, before);
        }
    }
}

#[test]
fn pd_server_gc_first_error_and_no_implicit_adjustment() {
    let mut config = PDServerConfig::default();
    assert_eq!(
        config.validate_gc_concurrency(),
        Err("gc-keyspace-index-load-concurrency must be between 1 and 16")
    );
    config.gc_keyspace_index_load_concurrency = 1;
    assert_eq!(
        config.validate_gc_concurrency(),
        Err("gc-state-cache-warmup-concurrency must be between 1 and 16")
    );
    config.gc_state_cache_warmup_concurrency = 16;
    assert_eq!(config.validate_gc_concurrency(), Ok(()));
}

#[test]
fn pd_server_gc_reload_candidate_values() {
    // Value contract used by Go Reload, without migrating PersistOptions/storage.
    let mut current = defaults();
    for input in [
        "{}",
        "{\"gc-keyspace-index-load-concurrency\":1,\"gc-state-cache-warmup-concurrency\":2}",
        "{\"gc-keyspace-index-load-concurrency\":16,\"gc-state-cache-warmup-concurrency\":16}",
    ] {
        let fields: serde_json::Value = serde_json::from_str(input).unwrap();
        let metadata = ConfigMetaData::from_defined_paths(
            fields.as_object().unwrap().keys().map(|key| [key.clone()]),
        );
        let mut candidate: PDServerConfig = serde_json::from_value(fields).unwrap();
        candidate.adjust_defaults(&metadata);
        candidate.validate_gc_concurrency().unwrap();
        let encoded = serde_json::to_value(&candidate).unwrap();
        let original: serde_json::Value = serde_json::from_str(input).unwrap();
        for name in [INDEX, WARMUP] {
            assert_eq!(
                encoded[name],
                original.get(name).cloned().unwrap_or(4.into())
            );
        }
        current = candidate;
    }
    for name in [INDEX, WARMUP] {
        for value in [-1, 0, 17, 128, i64::MAX] {
            let mut fields = serde_json::Map::new();
            fields.insert(name.into(), value.into());
            let metadata = ConfigMetaData::from_defined_paths([[name]]);
            let mut candidate: PDServerConfig = serde_json::from_value(fields.into()).unwrap();
            candidate.adjust_defaults(&metadata);
            let before = current.clone();
            match candidate.validate_gc_concurrency() {
                Ok(()) => current = candidate,
                Err(error) => assert_eq!(error, format!("{name} must be between 1 and 16")),
            }
            assert_eq!(current, before, "invalid reload candidate {name}={value}");
        }
    }
}
