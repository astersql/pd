// Copyright 2026 AsterSQL.

use astersql_pd_configutil::ConfigMetaData;

use crate::*;

#[test]
fn deprecated_config_log_mapping_and_warning_order() {
    for old_file in ["", "legacy.log"] {
        for old_level in ["", "debug"] {
            for new_file in ["", "modern.log"] {
                for new_level in ["", "warn"] {
                    for learner in [false, true] {
                        let config = ConfigScalars {
                            log_file_deprecated: old_file.into(),
                            log_level_deprecated: old_level.into(),
                            ..Default::default()
                        };
                        let metadata = ConfigMetaData::from_defined_paths(
                            learner.then_some(["schedule", "disable-raft-learner"]),
                        );
                        let mut filename = new_file.to_owned();
                        let mut level = new_level.to_owned();
                        let mut warnings = vec!["existing".to_owned()];
                        config.adjust_deprecated_from_file(
                            "config/pd.toml",
                            &metadata,
                            &mut filename,
                            &mut level,
                            &mut warnings,
                        );
                        assert_eq!(
                            filename,
                            if new_file.is_empty() {
                                old_file
                            } else {
                                new_file
                            }
                        );
                        assert_eq!(
                            level,
                            if new_level.is_empty() {
                                old_level
                            } else {
                                new_level
                            }
                        );
                        let mut expected = vec!["existing".to_owned()];
                        if !old_file.is_empty() {
                            expected.push(
                                "log-file in config/pd.toml is deprecated, use [log.file] instead"
                                    .into(),
                            );
                        }
                        if !old_level.is_empty() {
                            expected.push(
                                "log-level in config/pd.toml is deprecated, use [log] instead"
                                    .into(),
                            );
                        }
                        if learner {
                            expected.push(
                                "disable-raft-learner in config/pd.toml is deprecated".into(),
                            );
                        }
                        assert_eq!(warnings, expected);
                        assert_eq!(config.log_file_deprecated, old_file);
                        assert_eq!(config.log_level_deprecated, old_level);
                    }
                }
            }
        }
    }
}

#[test]
fn deprecated_config_learner_presence_including_false() {
    for value in ["true", "false"] {
        let decoded =
            decode_toml_scalars(&format!("[schedule]\ndisable-raft-learner={value}")).unwrap();
        let mut warnings = Vec::new();
        decoded.values.adjust_deprecated_from_file(
            "pd.toml",
            &decoded.metadata,
            &mut String::new(),
            &mut String::new(),
            &mut warnings,
        );
        assert_eq!(warnings, ["disable-raft-learner in pd.toml is deprecated"]);
    }
}

#[test]
fn deprecated_config_trace_presence_always_conflicts() {
    for value in ["true", "false"] {
        for new in ["", "flow-round-by-digit=127"] {
            let (config, meta) =
                decode_toml_pd_server(&format!("trace-region-flow={value}\n{new}")).unwrap();
            let before = config.flow_round_by_digit;
            assert_eq!(
                migrate_configuration_from_file(&meta),
                Err(
                    "config item flow-round-by-digit and trace-region-flow(deprecated) are conflict"
                )
            );
            assert_eq!(config.flow_round_by_digit, before);
        }
    }
    for paths in [
        vec![],
        vec![vec!["flow-round-by-digit"]],
        vec![vec!["pd-server", "trace-region-flow"]],
    ] {
        assert!(
            migrate_configuration_from_file(&ConfigMetaData::from_defined_paths(paths)).is_ok()
        );
    }
}

#[test]
fn deprecated_config_raw_kv_split_is_ignored() {
    let (mut expected, metadata) = decode_toml_keyspace("").unwrap();
    expected.adjust(&metadata).unwrap();
    for value in ["true", "false", "'ignored unknown value'"] {
        let (mut config, metadata) =
            decode_toml_keyspace(&format!("disable-raw-kv-region-split={value}")).unwrap();
        config.adjust(&metadata).unwrap();
        assert_eq!(config, expected);
    }
    for value in ["true", "false", "null", "{}"] {
        let mut config: KeyspaceConfig =
            serde_json::from_str(&format!("{{\"disable-raw-kv-region-split\":{value}}}")).unwrap();
        config.adjust(&ConfigMetaData::default()).unwrap();
        assert_eq!(config, expected);
        assert!(
            !serde_json::to_string(&config)
                .unwrap()
                .contains("disable-raw-kv-region-split")
        );
    }
}

#[test]
fn deprecated_config_existing_go_bytes_after_ignored_legacy_key() {
    let evidence = include_str!("../../docs/rust/theme3-服务配置契约.md");
    for marker in ["batch84-missing", "batch84-explicit-zero"] {
        let marker = format!("<!-- {marker} -->\n\n```json\n");
        let line = evidence
            .split_once(&marker)
            .unwrap()
            .1
            .lines()
            .next()
            .unwrap();
        let start = line.find("\"keyspace\":{").unwrap() + "\"keyspace\":".len();
        let end = line[start..].find(",\"micro-service\":").unwrap() + start;
        let go_bytes = &line[start..end];
        let (mut config, meta) = decode_toml_keyspace("disable-raw-kv-region-split=true").unwrap();
        config.adjust(&meta).unwrap();
        assert_eq!(serde_json::to_string(&config).unwrap(), go_bytes);
        assert!(!line.contains("\"log-file\":"));
        assert!(!line.contains("\"log-level\":"));
    }
}
