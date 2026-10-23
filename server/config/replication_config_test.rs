// Copyright 2026 AsterSQL.
use astersql_pd_configutil::ConfigMetaData;
use astersql_pd_typeutil::Duration;

use super::*;

#[test]
fn replication_config_normalize_and_preserve_mode() {
    for (input, expected) in [
        ("majority", "majority"),
        ("MAJORITY", "majority"),
        ("Dr_Auto_Sync", "dr-auto-sync"),
        ("DR-AUTO-SYNC", "dr-auto-sync"),
        ("dr_auto-sync", "dr-auto-sync"),
        ("", ""),
        (" minority", ""),
        (" majority", ""),
        ("majority ", ""),
        ("dr--auto-sync", ""),
        ("雪", ""),
        ("MAJORİTY", ""),
    ] {
        assert_eq!(normalize_replication_mode(input), expected, "{input:?}");
        let mut config = ReplicationModeConfig {
            replication_mode: input.into(),
            ..Default::default()
        };
        let metadata = ConfigMetaData::from_defined_paths([["replication-mode"]]);
        config.adjust(&metadata);
        assert_eq!(
            config.replication_mode,
            if expected.is_empty() {
                "majority"
            } else {
                input
            }
        );
        assert_eq!(
            config.dr_auto_sync.wait_store_timeout,
            Duration::from_nanos(60_000_000_000)
        );
    }
}

#[test]
fn replication_config_defaults_and_presence() {
    let mut config = ReplicationModeConfig::default();
    config.adjust(&ConfigMetaData::default());
    assert_eq!(config.replication_mode, "majority");
    assert_eq!(
        config.dr_auto_sync.wait_store_timeout,
        Duration::from_nanos(60_000_000_000)
    );
    assert_eq!(
        config.dr_auto_sync.wait_recover_timeout,
        Duration::default()
    );
    assert!(!config.dr_auto_sync.pause_region_split);
    let before = config.clone();
    config.adjust(&ConfigMetaData::default());
    assert_eq!(config, before);
    config.replication_mode = "DR_AUTO_SYNC".into();
    config.dr_auto_sync.wait_store_timeout = Duration::from_nanos(-1);
    config.adjust(&ConfigMetaData::default());
    assert_eq!(config, before); // Missing metadata overwrites nonzero values too.

    for nanos in [i64::MIN, -1, 0, 1, i64::MAX] {
        let mut dr = DRAutoSyncReplicationConfig {
            wait_store_timeout: Duration::from_nanos(nanos),
            wait_recover_timeout: Duration::from_nanos(-3),
            pause_region_split: true,
            ..Default::default()
        };
        let before = dr.clone();
        dr.adjust(&ConfigMetaData::from_defined_paths([[
            "wait-store-timeout",
        ]]));
        assert_eq!(dr, before);
        dr.adjust(&ConfigMetaData::default());
        assert_eq!(dr.wait_store_timeout, Duration::from_nanos(60_000_000_000));
        assert_eq!(dr.wait_recover_timeout, before.wait_recover_timeout);
        assert!(dr.pause_region_split);
    }
}

#[test]
fn replication_config_toml_dr_values() {
    // Same field values as Go TestReplicationMode, rooted at its inner table.
    let (mut config, metadata) = decode_toml_replication_mode(
        r#"
replication-mode = "dr-auto-sync"
[dr-auto-sync]
label-key = "zone"
primary = "zone1"
dr = "zone2"
primary-replicas = 2
dr-replicas = 1
wait-store-timeout = "120s"
wait-recover-timeout = "-2s"
pause-region-split = true
"#,
    )
    .unwrap();
    config.adjust(&metadata);
    assert_eq!(config.replication_mode, "dr-auto-sync");
    assert_eq!(
        config.dr_auto_sync,
        DRAutoSyncReplicationConfig {
            label_key: "zone".into(),
            primary: "zone1".into(),
            dr: "zone2".into(),
            primary_replicas: 2,
            dr_replicas: 1,
            wait_store_timeout: Duration::from_nanos(120_000_000_000),
            wait_recover_timeout: Duration::from_nanos(-2_000_000_000),
            pause_region_split: true,
        }
    );
    for timeout in ["0s", "-1ns"] {
        let input = format!(
            "replication-mode = 'MAJORITY'\n[dr-auto-sync]\nwait-store-timeout = '{timeout}'\nprimary-replicas = -1\ndr-replicas = -2\npause-region-split = false"
        );
        let (mut config, metadata) = decode_toml_replication_mode(&input).unwrap();
        config.adjust(&metadata);
        assert_eq!(config.replication_mode, "MAJORITY");
        assert_eq!(
            config.dr_auto_sync.wait_store_timeout,
            timeout.parse().unwrap()
        );
        assert_eq!(config.dr_auto_sync.primary_replicas, -1);
        assert_eq!(config.dr_auto_sync.dr_replicas, -2);
        assert!(!config.dr_auto_sync.pause_region_split);
    }
    let (mut config, metadata) = decode_toml_replication_mode("[dr-auto-sync]").unwrap();
    config.adjust(&metadata);
    assert_eq!(
        config.dr_auto_sync.wait_store_timeout,
        Duration::from_nanos(60_000_000_000)
    );
}

#[test]
fn replication_config_clone_independent() {
    let (mut original, metadata) = decode_toml_replication_mode("replication-mode = 'DR_AUTO_SYNC'\n[dr-auto-sync]\nlabel-key = 'zone'\nprimary = 'a'\ndr = 'b'").unwrap();
    original.adjust(&metadata);
    let mut copy = original.clone();
    assert_eq!(copy, original);
    copy.replication_mode.clear();
    copy.dr_auto_sync.label_key.clear();
    copy.dr_auto_sync.primary.clear();
    copy.dr_auto_sync.dr.clear();
    copy.dr_auto_sync.wait_store_timeout = Duration::default();
    assert_eq!(original.replication_mode, "DR_AUTO_SYNC");
    assert_eq!(original.dr_auto_sync.label_key, "zone");
    assert_eq!(original.dr_auto_sync.primary, "a");
    assert_eq!(original.dr_auto_sync.dr, "b");
    assert_eq!(
        original.dr_auto_sync.wait_store_timeout,
        Duration::from_nanos(60_000_000_000)
    );
}

#[test]
fn replication_config_existing_go_json_bytes() {
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
        let start = line.find("\"replication-mode\":{").unwrap() + "\"replication-mode\":".len();
        let end = line[start..].find(",\"keyspace\":").unwrap() + start;
        let go_bytes = &line[start..end];
        let mut config = ReplicationModeConfig::default();
        config.adjust(&ConfigMetaData::default());
        assert_eq!(serde_json::to_string(&config).unwrap(), go_bytes);
        assert_eq!(
            serde_json::from_str::<ReplicationModeConfig>(go_bytes).unwrap(),
            config
        );
    }
}

#[test]
fn replication_config_codec_types_and_roundtrip() {
    let (config, _) = decode_toml_replication_mode("[dr-auto-sync]\npause-region-split = true\nprimary-replicas = 9223372036854775807\ndr-replicas = -9223372036854775808").unwrap();
    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("\"pause-region-split\":\"true\""));
    assert_eq!(
        serde_json::from_str::<ReplicationModeConfig>(&json).unwrap(),
        config
    );
    for input in [
        "replication-mode = 1",
        "[dr-auto-sync]\nwait-store-timeout = 1",
        "[dr-auto-sync]\npause-region-split = 'true'",
        "[dr-auto-sync]\nprimary-replicas = 1.5",
    ] {
        assert!(decode_toml_replication_mode(input).is_err(), "{input}");
    }
    assert!(
        serde_json::from_str::<ReplicationModeConfig>(
            r#"{"dr-auto-sync":{"pause-region-split":true}}"#
        )
        .is_err()
    );
}
