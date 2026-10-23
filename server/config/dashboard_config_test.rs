// Copyright 2026 AsterSQL.
use astersql_pd_configutil::ConfigMetaData;

use super::*;

#[test]
fn dashboard_config_presence_and_host_defaults() {
    for present in [false, true] {
        for value in [false, true] {
            for default in [false, true] {
                let metadata = if present {
                    ConfigMetaData::from_defined_paths([["enable-telemetry"]])
                } else {
                    ConfigMetaData::default()
                };
                let mut config = DashboardConfig {
                    enable_telemetry: value,
                    ..Default::default()
                };
                config.adjust_with_telemetry_default(&metadata, default);
                assert_eq!(
                    config.enable_telemetry,
                    if present { value } else { default }
                );
                let before = config.clone();
                config.adjust_with_telemetry_default(&metadata, default);
                assert_eq!(config, before);
                config.adjust(&metadata);
                assert_eq!(config.enable_telemetry, present && value);
            }
        }
    }
}

#[test]
fn dashboard_config_custom_values_and_toml_roundtrip() {
    let input = r#"
tidb-cacert-path = "/nonexistent/ca.pem"
tidb-cert-path = "/nonexistent/client.pem"
tidb-key-path = "/nonexistent/client-key.pem"
public-path-prefix = "https://example.test/仪表盘/"
temp-dir = "/nonexistent/tmp"
internal-proxy = true
enable-telemetry = false
enable-experimental = true
disable-custom-prom-addr = true
"#;
    let (mut config, metadata) = decode_toml_dashboard(input).unwrap();
    let expected = DashboardConfig {
        tidb_ca_path: "/nonexistent/ca.pem".into(),
        tidb_cert_path: "/nonexistent/client.pem".into(),
        tidb_key_path: "/nonexistent/client-key.pem".into(),
        public_path_prefix: "https://example.test/仪表盘/".into(),
        temp_dir: "/nonexistent/tmp".into(),
        internal_proxy: true,
        enable_telemetry: false,
        enable_experimental: true,
        disable_custom_prom_addr: true,
    };
    config.adjust_with_telemetry_default(&metadata, true);
    assert_eq!(config, expected);
    let encoded = toml::to_string(&config).unwrap();
    assert_eq!(decode_toml_dashboard(&encoded).unwrap().0, expected);
    let json = serde_json::to_string(&config).unwrap();
    assert_eq!(
        serde_json::from_str::<DashboardConfig>(&json).unwrap(),
        expected
    );
    let mut copy = config.clone();
    copy.tidb_ca_path.clear();
    assert_eq!(config, expected);
}

#[test]
fn dashboard_config_nested_metadata_and_missing_toml() {
    let (mut config, metadata) = decode_toml_dashboard("").unwrap();
    config.adjust_with_telemetry_default(&metadata, true);
    assert!(config.enable_telemetry);
    let metadata = ConfigMetaData::from_defined_paths([["dashboard", "enable-telemetry"]]);
    let mut config = DashboardConfig::default();
    config.adjust_with_telemetry_default(&metadata.child(["dashboard"]), true);
    assert!(!config.enable_telemetry);
    config.adjust_with_telemetry_default(&metadata.child(["other"]), true);
    assert!(config.enable_telemetry);
}

#[test]
fn dashboard_config_existing_go_json_bytes() {
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
        let start = line.find("\"dashboard\":{").unwrap() + "\"dashboard\":".len();
        let end = line[start..].find(",\"replication-mode\":").unwrap() + start;
        let go_bytes = &line[start..end];
        let mut config = DashboardConfig::default();
        config.adjust(&ConfigMetaData::default());
        assert_eq!(serde_json::to_string(&config).unwrap(), go_bytes);
        assert_eq!(
            serde_json::from_str::<DashboardConfig>(go_bytes).unwrap(),
            config
        );
    }
}

#[test]
fn dashboard_config_scalar_types_and_null() {
    for key in [
        "tidb-cacert-path",
        "tidb-cert-path",
        "tidb-key-path",
        "public-path-prefix",
        "temp-dir",
        "internal-proxy",
        "enable-telemetry",
        "enable-experimental",
        "disable-custom-prom-addr",
    ] {
        let boolean = [
            "internal-proxy",
            "enable-telemetry",
            "enable-experimental",
            "disable-custom-prom-addr",
        ]
        .contains(&key);
        for value in if boolean {
            vec!["\"true\"", "1", "[]", "{}"]
        } else {
            vec!["true", "1", "[]", "{}"]
        } {
            assert!(
                serde_json::from_str::<DashboardConfig>(&format!("{{\"{key}\":{value}}}")).is_err()
            );
        }
        for value in if boolean {
            vec!["'true'", "1", "[]", "{}"]
        } else {
            vec!["true", "1", "[]", "{}"]
        } {
            assert!(decode_toml_dashboard(&format!("{key} = {value}")).is_err());
        }
        assert_eq!(
            serde_json::from_str::<DashboardConfig>(&format!("{{\"{key}\":null}}")).unwrap(),
            DashboardConfig::default()
        );
    }
    assert_eq!(
        serde_json::from_str::<DashboardConfig>("{}").unwrap(),
        DashboardConfig::default()
    );
}
