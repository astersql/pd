// Copyright 2026 AsterSQL.

use crate::PDServerValidationError;
use crate::{PDServerConfig, decode_toml_pd_server};
use astersql_pd_configutil::ConfigMetaData;

fn valid() -> PDServerConfig {
    let mut config = PDServerConfig::default();
    config.adjust_defaults(&ConfigMetaData::default());
    config
}

fn error(config: &PDServerConfig) -> String {
    config.validate().unwrap_err().to_string()
}

#[test]
fn pd_server_validate_dashboard() {
    // config_test.go::TestPDServerConfig and util_test.go::TestValidateURLWithScheme.
    let mut config = valid();
    for address in [
        "auto",
        "none",
        "http://127.0.0.1:2379",
        "https://foo",
        "custom://pd",
    ] {
        config.dashboard_address = address.into();
        assert!(config.validate().is_ok(), "{address}");
    }
    for address in [
        "",
        "foo",
        "127.0.0.1:2379",
        "/foo",
        "AUTO",
        "http://",
        "http://pd/%zz",
    ] {
        config.dashboard_address = address.into();
        assert_eq!(
            error(&config),
            crate::validate_url_with_scheme(address)
                .unwrap_err()
                .to_string()
        );
    }
}

#[test]
fn pd_server_validate_key_and_flow() {
    let mut config = valid();
    for key in ["table", "raw", "txn"] {
        config.key_type = key.into();
        assert!(config.validate().is_ok());
    }
    for key in ["", "TABLE", " table", "unknown", "表"] {
        config.key_type = key.into();
        assert_eq!(error(&config), format!("key-type {key} is invalid"));
    }
    config.key_type = "table".into();
    for value in [0, 3, i64::MAX] {
        config.flow_round_by_digit = value;
        assert!(config.validate().is_ok());
    }
    for value in [-1, i64::MIN] {
        config.flow_round_by_digit = value;
        let err = config.validate().unwrap_err();
        assert_eq!(
            err.to_string(),
            "[PD:server:ErrConfiguration]flow round by digit cannot be negative number"
        );
        assert_eq!(
            match &err {
                PDServerValidationError::FlowRound(error) => error.code(),
                _ => panic!("expected typed flow error: {err:?}"),
            },
            "PD:server:ErrConfiguration"
        );
    }
}

#[test]
fn pd_server_validate_float_boundaries() {
    // Direct Validate comparisons in config.go; NaN is accepted by Go comparisons.
    for (index, low, high, message) in [
        (
            0,
            0.0_f64,
            0.99_f64,
            "server-memory-limit should between 0 and 0.99",
        ),
        (
            1,
            0.5,
            0.99,
            "server-memory-limit-gc-trigger should between 0.5 and 0.99",
        ),
        (2, 0.0, 0.9, "gc-tuner-threshold should between 0 and 0.9"),
    ] {
        for value in [
            low,
            high,
            (low + high) / 2.0,
            f64::NAN,
            low.next_up(),
            high.next_down(),
            -0.0,
        ] {
            if value < low {
                continue;
            }
            let mut config = valid();
            *float_field(&mut config, index) = value;
            assert!(config.validate().is_ok(), "{index}: {value}");
            assert_eq!(float_field(&mut config, index).to_bits(), value.to_bits());
        }
        for value in [
            low.next_down(),
            high.next_up(),
            f64::NEG_INFINITY,
            f64::INFINITY,
        ] {
            let mut config = valid();
            *float_field(&mut config, index) = value;
            assert_eq!(error(&config), message, "{index}: {value}");
            assert_eq!(float_field(&mut config, index).to_bits(), value.to_bits());
        }
    }
}

fn float_field(config: &mut PDServerConfig, index: usize) -> &mut f64 {
    match index {
        0 => &mut config.server_memory_limit,
        1 => &mut config.server_memory_limit_gc_trigger,
        _ => &mut config.gc_tuner_threshold,
    }
}

#[test]
fn pd_server_validate_error_order() {
    let mut config = valid();
    config.gc_keyspace_index_load_concurrency = 0;
    config.gc_state_cache_warmup_concurrency = 0;
    config.dashboard_address = "http://".into();
    config.key_type.clear();
    config.flow_round_by_digit = -1;
    config.server_memory_limit = -1.0;
    config.server_memory_limit_gc_trigger = -1.0;
    config.gc_tuner_threshold = -1.0;
    assert_eq!(
        error(&config),
        "gc-keyspace-index-load-concurrency must be between 1 and 16"
    );
    config.gc_keyspace_index_load_concurrency = 1;
    assert_eq!(
        error(&config),
        "gc-state-cache-warmup-concurrency must be between 1 and 16"
    );
    config.gc_state_cache_warmup_concurrency = 16;
    assert_eq!(error(&config), "http:// has no scheme");
    config.dashboard_address = "none".into();
    assert_eq!(error(&config), "key-type  is invalid");
    config.key_type = "raw".into();
    assert_eq!(
        error(&config),
        "[PD:server:ErrConfiguration]flow round by digit cannot be negative number"
    );
    config.flow_round_by_digit = 0;
    assert_eq!(
        error(&config),
        "server-memory-limit should between 0 and 0.99"
    );
    config.server_memory_limit = 0.0;
    assert_eq!(
        error(&config),
        "server-memory-limit-gc-trigger should between 0.5 and 0.99"
    );
    config.server_memory_limit_gc_trigger = 0.5;
    assert_eq!(
        error(&config),
        "gc-tuner-threshold should between 0 and 0.9"
    );
    config.gc_tuner_threshold = 0.0;
    assert!(config.validate().is_ok());
}

#[test]
fn pd_server_validate_decode_adjust_and_unchecked_fields() {
    let (mut config, meta) = decode_toml_pd_server("server-memory-limit = 2.0\nmax-gap-reset-ts = '-1s'\nmin-resolved-ts-persistence-interval = '-1s'\nmetric-storage = 'anything'\nruntime-services = ['unknown']").unwrap();
    config.adjust_defaults(&meta);
    assert_eq!(config.server_memory_limit, 0.99);
    let before = config.clone();
    assert!(config.validate().is_ok());
    assert_eq!(config, before);
    let (mut config, meta) = decode_toml_pd_server("key-type = ''").unwrap();
    config.adjust_defaults(&meta);
    assert_eq!(error(&config), "key-type  is invalid");
}
