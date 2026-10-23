// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_typeutil::Duration;

#[test]
fn config_tso_zero_and_negative_defaults() {
    for nanos in [0, -1, i64::MIN] {
        let mut cfg = ConfigScalars {
            leader_lease: 123,
            tso_proxy_recv_from_client_timeout: Duration::from_nanos(nanos),
            tso_save_interval: Duration::from_nanos(nanos),
            tso_update_physical_interval: Duration::from_nanos(nanos),
            ..Default::default()
        };
        cfg.adjust_tso_defaults();
        assert_eq!(
            cfg.tso_proxy_recv_from_client_timeout,
            Duration::from_nanos(3_600_000_000_000)
        );
        assert_eq!(cfg.tso_save_interval, Duration::from_nanos(5_000_000_000));
        assert_eq!(
            cfg.tso_update_physical_interval,
            Duration::from_nanos(50_000_000)
        );
        assert_eq!(cfg.leader_lease, 123);
    }
}

#[test]
fn config_tso_physical_interval_clamps() {
    // Includes the 500ns/15s cases in Go TestAdjust and both inclusive limits.
    for (input, expected) in [
        (1, 1_000_000),
        (500, 1_000_000),
        (999_999, 1_000_000),
        (1_000_000, 1_000_000),
        (50_000_000, 50_000_000),
        (123_456_789, 123_456_789),
        (10_000_000_000, 10_000_000_000),
        (10_000_000_001, 10_000_000_000),
        (15_000_000_000, 10_000_000_000),
        (i64::MAX, 10_000_000_000),
    ] {
        let mut cfg = ConfigScalars {
            tso_update_physical_interval: Duration::from_nanos(input),
            ..Default::default()
        };
        cfg.adjust_tso_defaults();
        assert_eq!(
            cfg.tso_update_physical_interval,
            Duration::from_nanos(expected),
            "input={input}"
        );
        let once = cfg.clone();
        cfg.adjust_tso_defaults();
        assert_eq!(cfg, once);
    }
}

#[test]
fn config_tso_proxy_count_preserves_negative_unlimited() {
    for (input, expected) in [
        (0, 5000),
        (-1, -1),
        (i64::MIN, i64::MIN),
        (1, 1),
        (i64::MAX, i64::MAX),
    ] {
        let mut cfg = ConfigScalars {
            max_concurrent_tso_proxy_streamings: input,
            ..Default::default()
        };
        cfg.adjust_tso_defaults();
        assert_eq!(cfg.max_concurrent_tso_proxy_streamings, expected);
    }
}

#[test]
fn config_tso_positive_save_and_timeout_preserved() {
    for nanos in [1, 500, 5_000_000_000, i64::MAX] {
        let mut cfg = ConfigScalars {
            tso_proxy_recv_from_client_timeout: Duration::from_nanos(nanos),
            tso_save_interval: Duration::from_nanos(nanos),
            ..Default::default()
        };
        cfg.adjust_tso_defaults();
        assert_eq!(
            cfg.tso_proxy_recv_from_client_timeout,
            Duration::from_nanos(nanos)
        );
        assert_eq!(cfg.tso_save_interval, Duration::from_nanos(nanos));
        assert_eq!(
            cfg.tso_update_physical_interval,
            Duration::from_nanos(50_000_000)
        );
    }
}

#[test]
fn config_tso_decoded_values_normalize() {
    for mut decoded in [
        decode_json_scalars(r#"{"tso-save-interval":"-1ns","tso-proxy-recv-from-client-timeout":"1ns","tso-update-physical-interval":"500ns","max-concurrent-tso-proxy-streamings":-3}"#).unwrap(),
        decode_toml_scalars("tso-save-interval = '-1ns'\ntso-proxy-recv-from-client-timeout = '1ns'\ntso-update-physical-interval = '500ns'\nmax-concurrent-tso-proxy-streamings = -3").unwrap(),
    ] {
        assert!(decoded.metadata.check_undecoded().is_none());
        decoded.values.adjust_tso_defaults();
        assert_eq!(decoded.values.tso_save_interval, Duration::from_nanos(5_000_000_000));
        assert_eq!(decoded.values.tso_proxy_recv_from_client_timeout, Duration::from_nanos(1));
        assert_eq!(decoded.values.tso_update_physical_interval, Duration::from_nanos(1_000_000));
        assert_eq!(decoded.values.max_concurrent_tso_proxy_streamings, -3);
    }
    for key in [
        "tso-save-interval",
        "tso-proxy-recv-from-client-timeout",
        "tso-update-physical-interval",
    ] {
        assert!(decode_json_scalars(&format!("{{\"{key}\":null}}")).is_err());
        assert!(decode_json_scalars(&format!("{{\"{key}\":1}}")).is_err());
        assert!(decode_toml_scalars(&format!("{key} = 'invalid'")).is_err());
    }
}
