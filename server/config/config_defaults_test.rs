// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_configutil::ConfigMetaData;
use astersql_pd_typeutil::{ByteSize, Duration};
use std::path::Path;

fn adjust(config: &mut ConfigScalars, meta: &ConfigMetaData) {
    config.adjust_base_defaults(
        meta,
        ConfigDefaultsContext {
            hostname: "host-a",
            cwd: Path::new("/work/pd"),
        },
    );
}

#[test]
fn config_defaults_missing_values() {
    let mut c = ConfigScalars::default();
    adjust(&mut c, &ConfigMetaData::default());
    assert_eq!(c.name, "pd-host-a");
    assert_eq!(c.data_dir, "/work/pd/default.pd-host-a");
    assert_eq!(c.client_urls, "http://127.0.0.1:2379");
    assert_eq!(c.peer_urls, "http://127.0.0.1:2380");
    assert_eq!(c.advertise_client_urls, c.client_urls);
    assert_eq!(c.advertise_peer_urls, c.peer_urls);
    assert_eq!(c.initial_cluster, "pd-host-a=http://127.0.0.1:2380");
    assert_eq!(c.initial_cluster_state, "new");
    assert_eq!(c.initial_cluster_token, "pd-cluster");
    assert_eq!(c.leader_lease, 5);
    assert_eq!(c.quota_backend_bytes.as_bytes(), 8 << 30);
    assert_eq!(c.max_request_bytes, 150 << 20);
    assert_eq!(c.auto_compaction_mode, "periodic");
    assert_eq!(c.auto_compaction_retention, "1h");
    assert!(c.pre_vote && c.enable_grpc_gateway);
    assert_eq!(c.labels, Some(Default::default()));
    assert_eq!(c.tick_interval.as_nanos(), 500_000_000);
    assert_eq!(c.election_interval.as_nanos(), 3_000_000_000);
    assert_eq!(c.heartbeat_stream_bind_interval.as_nanos(), 60_000_000_000);
    assert_eq!(c.leader_priority_check_interval.as_nanos(), 60_000_000_000);
    let before = c.clone();
    adjust(&mut c, &ConfigMetaData::default());
    assert_eq!(c, before);
}

#[test]
fn config_defaults_presence_zero_and_false() {
    let mut decoded = decode_json_scalars(r#"{"quota-backend-bytes":"0B","max-request-bytes":0,"enable-prevote":false,"enable-grpc-gateway":false,"lease":0}"#).unwrap();
    adjust(&mut decoded.values, &decoded.metadata);
    assert_eq!(decoded.values.quota_backend_bytes.as_bytes(), 0);
    assert_eq!(decoded.values.max_request_bytes, 0);
    assert!(!decoded.values.pre_vote);
    assert!(!decoded.values.enable_grpc_gateway);
    assert_eq!(decoded.values.leader_lease, 5);
    // Go metadata keys are literal, not canonicalized by the decoder.
    let mut decoded =
        decode_toml_scalars("EnableGRPCGateway = false\nEnable-Prevote = false").unwrap();
    adjust(&mut decoded.values, &decoded.metadata);
    assert!(decoded.values.enable_grpc_gateway && decoded.values.pre_vote);
}

#[test]
fn config_defaults_explicit_values_preserved() {
    let mut c = ConfigScalars {
        name: "custom".into(),
        data_dir: "../data/./db/..".into(),
        client_urls: "http://client:1".into(),
        peer_urls: "http://peer:2".into(),
        advertise_client_urls: "https://advertised:3".into(),
        advertise_peer_urls: "http://one:4,,http://two:5".into(),
        leader_lease: -1,
        quota_backend_bytes: ByteSize::from_bytes(42),
        max_request_bytes: 17,
        auto_compaction_mode: "revision".into(),
        auto_compaction_retention: "0".into(),
        initial_cluster_state: "existing".into(),
        initial_cluster_token: "token".into(),
        labels: Some([("zone".into(), "east".into())].into()),
        force_new_cluster: true,
        ..Default::default()
    };
    let meta = ConfigMetaData::from_defined_paths([["quota-backend-bytes"], ["max-request-bytes"]]);
    adjust(&mut c, &meta);
    assert_eq!(c.name, "custom");
    assert_eq!(c.data_dir, "/work/data");
    assert_eq!(c.client_urls, "http://client:1");
    assert_eq!(c.peer_urls, "http://peer:2");
    assert_eq!(c.advertise_client_urls, "https://advertised:3");
    assert_eq!(
        c.initial_cluster,
        "custom=http://one:4,custom=,custom=http://two:5"
    );
    assert_eq!(c.initial_cluster_state, "existing");
    assert_eq!(c.initial_cluster_token, "token");
    assert_eq!(c.leader_lease, -1);
    assert_eq!(c.quota_backend_bytes.as_bytes(), 42);
    assert_eq!(c.max_request_bytes, 17);
    assert_eq!(c.auto_compaction_mode, "revision");
    assert_eq!(c.auto_compaction_retention, "0");
    assert_eq!(c.labels.unwrap()["zone"], "east");
    assert!(c.force_new_cluster);
}

#[test]
fn config_defaults_absent_metadata_overwrites_sizes() {
    let mut c = ConfigScalars {
        quota_backend_bytes: ByteSize::from_bytes(42),
        max_request_bytes: 17,
        initial_cluster: "provided=http://peer:1".into(),
        data_dir: "/data/../db".into(),
        ..Default::default()
    };
    adjust(&mut c, &ConfigMetaData::default());
    assert_eq!(c.quota_backend_bytes.as_bytes(), 8 << 30);
    assert_eq!(c.max_request_bytes, 150 << 20);
    assert_eq!(c.initial_cluster, "provided=http://peer:1");
    assert_eq!(c.data_dir, "/db");
}

#[test]
fn config_defaults_duration_boundaries() {
    for nanos in [-1, 0, 1, i64::MAX] {
        let mut c = ConfigScalars {
            tick_interval: Duration::from_nanos(nanos),
            election_interval: Duration::from_nanos(nanos),
            heartbeat_stream_bind_interval: Duration::from_nanos(nanos),
            leader_priority_check_interval: Duration::from_nanos(nanos),
            ..Default::default()
        };
        adjust(&mut c, &ConfigMetaData::default());
        for (value, default) in [
            (c.tick_interval, 500_000_000),
            (c.election_interval, 3_000_000_000),
            (c.heartbeat_stream_bind_interval, 60_000_000_000),
            (c.leader_priority_check_interval, 60_000_000_000),
        ] {
            assert_eq!(value.as_nanos(), if nanos <= 0 { default } else { nanos });
        }
    }
}

#[test]
fn config_defaults_inherits_addresses_and_preserves_other_owners() {
    let mut c = ConfigScalars {
        name: "node".into(),
        peer_urls: " https://one:1,http://two:2".into(),
        client_urls: "https://client:3".into(),
        join: "http://joining:4".into(),
        max_concurrent_tso_proxy_streamings: -7,
        log_level_deprecated: "debug".into(),
        ..Default::default()
    };
    adjust(&mut c, &ConfigMetaData::default());
    assert_eq!(c.advertise_client_urls, "https://client:3");
    assert_eq!(c.advertise_peer_urls, c.peer_urls);
    assert_eq!(c.initial_cluster, "node= https://one:1,node=http://two:2");
    assert_eq!(c.join, "http://joining:4");
    assert_eq!(c.max_concurrent_tso_proxy_streamings, -7);
    assert_eq!(c.log_level_deprecated, "debug");
}

#[test]
fn config_defaults_decoded_durations_and_labels() {
    let mut decoded = decode_toml_scalars("tick-interval = '-1s'\nelection-interval = '2s'\nHeartbeatStreamBindInterval = '3s'\nLeaderPriorityCheckInterval = '4s'\n[labels]\nzone = 'east'").unwrap();
    assert!(decoded.metadata.check_undecoded().is_none());
    adjust(&mut decoded.values, &decoded.metadata);
    assert_eq!(decoded.values.tick_interval.as_nanos(), 500_000_000);
    assert_eq!(decoded.values.election_interval.as_nanos(), 2_000_000_000);
    assert_eq!(
        decoded.values.heartbeat_stream_bind_interval.as_nanos(),
        3_000_000_000
    );
    assert_eq!(
        decoded.values.leader_priority_check_interval.as_nanos(),
        4_000_000_000
    );
    assert_eq!(decoded.values.labels.as_ref().unwrap()["zone"], "east");
    // Hidden Go JSON fields cannot be supplied by JSON configuration.
    let decoded =
        decode_json_scalars(r#"{"HeartbeatStreamBindInterval":"3s","labels":null}"#).unwrap();
    assert_eq!(decoded.values.heartbeat_stream_bind_interval.as_nanos(), 0);
    assert_eq!(decoded.values.labels, None);
    assert!(decode_json_scalars(r#"{"tick-interval":null}"#).is_err());
    assert!(decode_json_scalars(r#"{"election-interval":null}"#).is_err());
}
