// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_configutil::ConfigMetaData;
use astersql_pd_typeutil::{Duration, StringSlice};
use serde_json::{Value, json};

fn all_defined() -> ConfigMetaData {
    let fields = serde_json::to_value(PDServerConfig::default()).unwrap();
    ConfigMetaData::from_defined_paths(fields.as_object().unwrap().keys().map(|s| [s.clone()]))
}

#[test]
fn pd_server_defaults_go_bytes() {
    let evidence = include_str!("../../docs/rust/theme3-服务配置契约.md");
    for label in ["missing", "explicit-zero"] {
        let marker = format!("<!-- batch84-{label} -->\n\n```json\n");
        let line = evidence
            .split_once(&marker)
            .unwrap()
            .1
            .lines()
            .next()
            .unwrap();
        let log: Value = serde_json::from_str(line).unwrap();
        let expected = &log["config"]["pd-server"];
        let mut config = PDServerConfig::default();
        config.adjust_defaults(&ConfigMetaData::default());
        assert_eq!(serde_json::to_value(&config).unwrap(), *expected);
        let raw = line.split_once("\"pd-server\":").unwrap().1;
        let mut stream = serde_json::Deserializer::from_str(raw).into_iter::<Value>();
        stream.next().unwrap().unwrap();
        assert_eq!(
            serde_json::to_vec(&config).unwrap(),
            raw.as_bytes()[..stream.byte_offset()]
        );
        let decoded: PDServerConfig = serde_json::from_value(expected.clone()).unwrap();
        assert_eq!(decoded, config);
        let again = config.clone();
        config.adjust_defaults(&ConfigMetaData::default());
        assert_eq!(config, again);
    }
}

#[test]
fn pd_server_defaults_explicit_zero() {
    let mut config = PDServerConfig::default();
    config.adjust_defaults(&all_defined());
    assert!(!config.use_region_storage);
    assert!(!config.enable_gogc_tuner);
    assert!(!config.block_safe_point_v1);
    assert_eq!(config.key_type, "");
    assert_eq!(config.dashboard_address, "");
    assert_eq!(config.flow_round_by_digit, 0);
    assert_eq!(config.gc_keyspace_index_load_concurrency, 0);
    assert_eq!(config.gc_state_cache_warmup_concurrency, 0);
    assert_eq!(
        config.min_resolved_ts_persistence_interval,
        Duration::default()
    );
    assert_eq!(
        config.max_reset_ts_gap,
        Duration::from_nanos(86_400_000_000_000)
    );
    assert_eq!(config.server_memory_limit_gc_trigger, 0.5);
    assert_eq!(config.gc_tuner_threshold, 0.0);
}

#[test]
fn pd_server_defaults_preserve_and_replace() {
    let mut config = PDServerConfig {
        key_type: "raw".into(),
        dashboard_address: "none".into(),
        runtime_services: StringSlice::from(["a", "b"]),
        metric_storage: "prometheus".into(),
        flow_round_by_digit: 8,
        gc_keyspace_index_load_concurrency: -1,
        gc_state_cache_warmup_concurrency: 16,
        block_safe_point_v1: true,
        max_reset_ts_gap: Duration::from_nanos(1),
        min_resolved_ts_persistence_interval: Duration::from_nanos(2),
        server_memory_limit: 0.4,
        server_memory_limit_gc_trigger: 0.8,
        gc_tuner_threshold: 0.2,
        ..Default::default()
    };
    let before = config.clone();
    config.adjust_defaults(&all_defined());
    assert_eq!(config, before);
    config.adjust_defaults(&ConfigMetaData::default());
    assert_eq!(config.key_type, "table");
    assert_eq!(config.dashboard_address, "auto");
    assert!(config.runtime_services.as_slice().is_empty());
    assert!(config.use_region_storage && config.enable_gogc_tuner);
    assert_eq!(config.metric_storage, "prometheus");
    assert_eq!(config.flow_round_by_digit, 8);
    assert_eq!(config.gc_keyspace_index_load_concurrency, -1);
    assert_eq!(config.gc_state_cache_warmup_concurrency, 16);
    assert!(config.block_safe_point_v1);
    assert_eq!(config.max_reset_ts_gap, before.max_reset_ts_gap);
    assert_eq!(
        config.min_resolved_ts_persistence_interval,
        before.min_resolved_ts_persistence_interval
    );
    assert_eq!(config.server_memory_limit, 0.4);
    assert_eq!(config.server_memory_limit_gc_trigger, 0.8);
    assert_eq!(config.gc_tuner_threshold, 0.2);
}

#[test]
fn pd_server_defaults_bounds() {
    for (value, memory, trigger, tuner) in [
        (f64::NEG_INFINITY, 0.0, 0.5, 0.0),
        (-1.0, 0.0, 0.5, 0.0),
        (1.0, 0.99, 0.99, 0.9),
        (f64::INFINITY, 0.99, 0.99, 0.9),
    ] {
        let mut config = PDServerConfig {
            server_memory_limit: value,
            server_memory_limit_gc_trigger: value,
            gc_tuner_threshold: value,
            max_reset_ts_gap: Duration::from_nanos(-1),
            min_resolved_ts_persistence_interval: Duration::from_nanos(-1),
            ..Default::default()
        };
        config.adjust_defaults(&all_defined());
        assert_eq!(config.server_memory_limit, memory);
        assert_eq!(config.server_memory_limit_gc_trigger, trigger);
        assert_eq!(config.gc_tuner_threshold, tuner);
        assert_eq!(
            config.max_reset_ts_gap,
            Duration::from_nanos(86_400_000_000_000)
        );
        assert_eq!(
            config.min_resolved_ts_persistence_interval,
            Duration::from_nanos(-1)
        );
        config.adjust_defaults(&ConfigMetaData::default());
        assert_eq!(
            config.min_resolved_ts_persistence_interval,
            Duration::from_nanos(1_000_000_000)
        );
    }
    let mut config = PDServerConfig {
        server_memory_limit: f64::NAN,
        server_memory_limit_gc_trigger: f64::NAN,
        gc_tuner_threshold: f64::NAN,
        ..Default::default()
    };
    config.adjust_defaults(&ConfigMetaData::default());
    assert!(config.server_memory_limit.is_nan());
    assert!(config.server_memory_limit_gc_trigger.is_nan());
    assert!(config.gc_tuner_threshold.is_nan());
}

#[test]
fn pd_server_defaults_string_encoding() {
    let config = PDServerConfig {
        runtime_services: StringSlice::from(["x", "y"]),
        use_region_storage: true,
        block_safe_point_v1: true,
        ..Default::default()
    };
    let value = serde_json::to_value(&config).unwrap();
    assert_eq!(value["runtime-services"], "x,y");
    assert_eq!(value["use-region-storage"], "true");
    assert_eq!(value["block-safe-point-v1"], "true");
    assert_eq!(
        serde_json::from_value::<PDServerConfig>(value).unwrap(),
        config
    );
    assert!(serde_json::from_value::<PDServerConfig>(json!({"use-region-storage": true})).is_err());
}

#[test]
fn pd_server_defaults_toml_presence_and_types() {
    let input = r#"
use-region-storage = false
enable-gogc-tuner = false
block-safe-point-v1 = true
runtime-services = ["x,y", "z"]
metric-storage = "custom"
key-type = "txn"
dashboard-address = "none"
flow-round-by-digit = 0
gc-keyspace-index-load-concurrency = 0
gc-state-cache-warmup-concurrency = 0
max-gap-reset-ts = "0s"
min-resolved-ts-persistence-interval = "-1s"
server-memory-limit = 0.0
server-memory-limit-gc-trigger = 0.0
gc-tuner-threshold = 0.0
"#;
    let (mut config, metadata) = decode_toml_pd_server(input).unwrap();
    config.adjust_defaults(&metadata);
    assert!(!config.use_region_storage && !config.enable_gogc_tuner);
    assert!(config.block_safe_point_v1);
    assert_eq!(config.runtime_services.as_slice(), ["x,y", "z"]);
    assert_eq!(config.metric_storage, "custom");
    assert_eq!(config.key_type, "txn");
    assert_eq!(config.dashboard_address, "none");
    assert_eq!(config.flow_round_by_digit, 0);
    assert_eq!(config.gc_keyspace_index_load_concurrency, 0);
    assert_eq!(config.gc_state_cache_warmup_concurrency, 0);
    assert_eq!(
        config.min_resolved_ts_persistence_interval,
        Duration::from_nanos(-1_000_000_000)
    );
    assert_eq!(
        config.max_reset_ts_gap,
        Duration::from_nanos(86_400_000_000_000)
    );
    assert_eq!(config.server_memory_limit_gc_trigger, 0.5);
    for input in [
        "use-region-storage = 'false'",
        "runtime-services = 'a,b'",
        "runtime-services = [1]",
        "max-gap-reset-ts = 0",
    ] {
        assert!(decode_toml_pd_server(input).is_err(), "{input}");
    }
    let (mut empty, meta) = decode_toml_pd_server("").unwrap();
    empty.adjust_defaults(&meta);
    assert!(empty.use_region_storage && empty.enable_gogc_tuner);
    assert_eq!(empty.gc_keyspace_index_load_concurrency, 4);
}

#[test]
fn pd_server_defaults_float_boundaries_and_nonfinite_encoding() {
    for value in [0.0, 0.5, 0.7, 0.9, 0.99] {
        let mut config = PDServerConfig {
            server_memory_limit: value,
            server_memory_limit_gc_trigger: value,
            gc_tuner_threshold: value,
            ..Default::default()
        };
        config.adjust_defaults(&all_defined());
        assert_eq!(config.server_memory_limit, value);
        assert_eq!(config.server_memory_limit_gc_trigger, value.max(0.5));
        assert_eq!(config.gc_tuner_threshold, value.min(0.9));
    }
    let (mut nonfinite, metadata) = decode_toml_pd_server("server-memory-limit = nan\nserver-memory-limit-gc-trigger = inf\ngc-tuner-threshold = -inf").unwrap();
    nonfinite.adjust_defaults(&metadata);
    assert!(nonfinite.server_memory_limit.is_nan());
    assert_eq!(nonfinite.server_memory_limit_gc_trigger, 0.99);
    assert_eq!(nonfinite.gc_tuner_threshold, 0.0);
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for field in [
            "server-memory-limit",
            "server-memory-limit-gc-trigger",
            "gc-tuner-threshold",
        ] {
            let mut config = PDServerConfig::default();
            match field {
                "server-memory-limit" => config.server_memory_limit = value,
                "server-memory-limit-gc-trigger" => config.server_memory_limit_gc_trigger = value,
                _ => config.gc_tuner_threshold = value,
            }
            assert!(serde_json::to_string(&config).is_err());
        }
    }
}
