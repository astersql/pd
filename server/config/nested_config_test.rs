// Copyright 2026 AsterSQL.
use crate::*;
use astersql_pd_configutil::ConfigMetaData;
use astersql_pd_schedule_config::ScheduleDefaults;
use std::path::Path;
fn adjust(c: &mut Config, meta: &ConfigMetaData) -> Result<(), String> {
    c.adjust(
        meta,
        ConfigDefaultsContext {
            hostname: "host",
            cwd: Path::new("/tmp"),
        },
        false,
        &mut ScheduleDefaults::default(),
    )
}
#[test]
fn nested_config_defaults() {
    let mut c = Config::default();
    adjust(&mut c, &ConfigMetaData::default()).unwrap();
    assert_eq!(c.schedule.max_snapshot_count, 64);
    assert_eq!(c.replication.max_replicas, 3);
    assert_eq!(c.controller.request_unit.read_base_cost, 0.125);
    assert_eq!(c.security.encryption.data_encryption_method, "plaintext");
    assert_eq!(c.log.level, "info");
    assert_eq!(c.metric.push_job, "pd-host");
}
#[test]
fn nested_config_error_precedence() {
    let mut c = Config::default();
    c.schedule.low_space_ratio = 0.2;
    c.schedule.high_space_ratio = 0.8;
    assert_eq!(
        adjust(&mut c, &ConfigMetaData::default()).unwrap_err(),
        "low-space-ratio should be larger than high-space-ratio"
    );
    assert_eq!(c.security.encryption.data_encryption_method, "");
}

#[test]
fn nested_config_go_captured_values_and_errors() {
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/nested_config_go.json")).unwrap();
    for case in fixtures["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let input = case["toml"].as_str().unwrap();
        if let Some(expected) = case.get("config") {
            let mut decoded = decode_toml_config(input).unwrap_or_else(|e| panic!("{name}: {e}"));
            adjust(&mut decoded.values, &decoded.metadata)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            let actual = serde_json::to_value(&decoded.values).unwrap();
            assert_eq!(&actual, expected, "{name}");
            let check = case["check_messages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>()
                .join("\n");
            if check == "config check successful" {
                assert!(decoded.values.warning_msgs.is_empty(), "{name}");
            } else {
                assert_eq!(decoded.values.warning_msgs.join("\n"), check, "{name}");
            }
        } else {
            let error = match decode_toml_config(input) {
                Err(e) => e,
                Ok(mut decoded) => adjust(&mut decoded.values, &decoded.metadata).unwrap_err(),
            };
            let expected = case["output"].as_str().unwrap().trim();
            if name == "redact" {
                assert!(
                    expected.ends_with(&error),
                    "{name}: {error:?} vs {expected:?}"
                );
            } else {
                assert_eq!(error, expected, "{name}");
            }
        }
    }
}

#[test]
fn nested_config_go_raw_nested_bytes() {
    use serde_json::value::RawValue;
    use std::collections::BTreeMap;
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/nested_config_go.json")).unwrap();
    for case in fixtures["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c.get("config").is_some())
    {
        let log = case["log"].as_str().unwrap();
        let fields: BTreeMap<String, Box<RawValue>> = serde_json::from_str(log).unwrap();
        let config: BTreeMap<String, Box<RawValue>> =
            serde_json::from_str(fields["config"].get()).unwrap();
        let mut decoded = decode_toml_config(case["toml"].as_str().unwrap()).unwrap();
        adjust(&mut decoded.values, &decoded.metadata).unwrap();
        let c = &decoded.values;
        for (key, actual) in [
            ("log", serde_json::to_string(&c.log).unwrap()),
            ("metric", serde_json::to_string(&c.metric).unwrap()),
            ("schedule", serde_json::to_string(&c.schedule).unwrap()),
            (
                "replication",
                serde_json::to_string(&c.replication).unwrap(),
            ),
            ("security", serde_json::to_string(&c.security).unwrap()),
            ("controller", serde_json::to_string(&c.controller).unwrap()),
            ("metering", serde_json::to_string(&c.metering).unwrap()),
        ] {
            assert_eq!(actual, config[key].get(), "{} {key}", case["name"]);
        }
    }
}

#[test]
fn nested_config_order_reload_presence_and_no_side_effects() {
    let input = r#"
name="fixture"
join="http://bad:%"
[controller]
push-metrics-address="preserved"
"#;
    let mut d = decode_toml_config(input).unwrap();
    assert!(
        adjust(&mut d.values, &d.metadata)
            .unwrap_err()
            .starts_with("failed to parse join addr:")
    );
    assert_eq!(d.values.scalars.leader_lease, 0);
    assert_eq!(d.values.metric.push_interval.as_nanos(), 15_000_000_000);
    assert_eq!(d.values.metric.push_job, "");
    assert_eq!(d.values.controller.push_metrics_address, "preserved");
    let mut d=decode_toml_config("[security.encryption]\ndata-encryption-method=\"invalid\"\n[controller]\npush-metrics-address=\"preserved\"").unwrap();
    assert!(
        adjust(&mut d.values, &d.metadata)
            .unwrap_err()
            .contains("unknown method")
    );
    assert_eq!(d.values.log.level, "info");
    assert_eq!(d.values.controller.push_metrics_address, "preserved");
    assert_eq!(d.values.controller.ltb_max_wait_duration.as_nanos(), 0);
    let mut d = decode_toml_config(
        "name=\"fixture\"\n[schedule]\nmax-snapshot-count=0\nenable-joint-consensus=false",
    )
    .unwrap();
    d.values
        .adjust(
            &d.metadata,
            ConfigDefaultsContext {
                hostname: "other",
                cwd: Path::new("/tmp"),
            },
            true,
            &mut ScheduleDefaults::default(),
        )
        .unwrap();
    assert_eq!(d.values.schedule.region_score_formula_version, "");
    assert_eq!(d.values.schedule.max_snapshot_count, 0);
    assert!(!d.values.schedule.enable_joint_consensus);
}
#[test]
fn nested_config_json_toml_types_and_float_boundaries() {
    let d=decode_json_config(r#"{"NAME":"PD","schedule":{"enable-joint-consensus":"false"},"log":{"level":null},"controller":{"ru-version-policy":{"default":2,"overrides":{"42":3}}},"security":{"SSLCABytes":"YWJj"}}"#).unwrap();
    assert_eq!(d.values.scalars.name, "PD");
    assert_eq!(d.values.controller.keyspace_ru_version(42), 3);
    assert_eq!(d.values.security.tls.sslca_bytes, Some(b"abc".to_vec()));
    assert!(decode_json_config(r#"{"schedule":{"enable-joint-consensus":false}}"#).is_err());
    let mut d = decode_toml_config("[schedule.default-store-limit]\nadd-peer=nan").unwrap();
    assert_eq!(
        adjust(&mut d.values, &d.metadata).unwrap_err(),
        "default-store-limit.add-peer should be finite and non-negative"
    );
    let mut d = decode_toml_config("[controller.request-unit]\nread-base-cost=nan").unwrap();
    adjust(&mut d.values, &d.metadata).unwrap();
    assert!(d.values.controller.request_unit.read_base_cost.is_nan());
    assert!(serde_json::to_string(&d.values).is_err());
    let d = decode_toml_config("[replication]\nlocation-labels=[\"zone,a\"]").unwrap();
    assert_eq!(d.values.replication.location_labels.as_slice(), ["zone,a"]);
}
#[test]
fn nested_config_paths_semver_and_warnings() {
    let mut d = decode_toml_config("name=\"fixture\"\nunknown=0").unwrap();
    adjust(&mut d.values, &d.metadata).unwrap();
    assert_eq!(
        d.values.warning_msgs,
        ["Config contains undefined item: unknown"]
    );
    for (file, valid) in [
        ("/tmp/data/logs/pd.log", false),
        ("/tmp/database/pd.log", true),
        ("/tmp/data/..logs/pd.log", true),
        ("/tmp/pd.log", true),
    ] {
        let mut c = Config::default();
        c.scalars.data_dir = "/tmp/data".into();
        c.log.file.filename = file.into();
        assert_eq!(c.validate(Path::new("/tmp")).is_ok(), valid, "{file}");
    }
    let d = decode_json_config(r#"{"cluster-version":"01.2.3-rc.1+build"}"#).unwrap();
    assert_eq!(d.values.cluster_version.to_string(), "1.2.3-rc.1+build");
    assert!(decode_json_config(r#"{"cluster-version":"1.2"}"#).is_err());
    assert_eq!(
        serde_json::to_string(&Config::default().cluster_version).unwrap(),
        r#""0.0.0""#
    );
}
