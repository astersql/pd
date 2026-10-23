// Copyright 2026 AsterSQL.
use super::*;
use serde_json::{Value, json};

#[test]
fn config_decode_presence_before_defaults() {
    for (empty, zero) in [
        (
            decode_json_scalars("{}").unwrap(),
            decode_json_scalars(
                r#"{"enable-prevote":false,"max-request-bytes":0,"name":"","lease":0}"#,
            )
            .unwrap(),
        ),
        (
            decode_toml_scalars("").unwrap(),
            decode_toml_scalars(
                "enable-prevote = false\nmax-request-bytes = 0\nname = ''\nlease = 0",
            )
            .unwrap(),
        ),
    ] {
        assert_eq!(empty.values, zero.values);
        for key in ["enable-prevote", "max-request-bytes", "name", "lease"] {
            assert!(!empty.metadata.is_defined(key));
            assert!(zero.metadata.is_defined(key), "explicit zero: {key}");
        }
    }
}

#[test]
fn config_decode_format_specific_names() {
    let decoded = decode_toml_scalars("auto-compaction-retention = '2h'\nEnableGRPCGateway = true\nForceNewCluster = true\nlease = -1").unwrap();
    assert_eq!(decoded.values.auto_compaction_retention, "2h");
    assert!(decoded.values.enable_grpc_gateway);
    assert!(decoded.values.force_new_cluster);
    assert_eq!(decoded.values.leader_lease, -1);
    let encoded = serde_json::to_value(&decoded.values).unwrap();
    assert_eq!(encoded["auto-compaction-retention-v2"], "2h");
    assert!(encoded.get("auto-compaction-retention").is_none());
    assert!(encoded.get("log-file").is_none());
    assert!(encoded.get("WarningMsgs").is_none());
    assert_eq!(encoded["force-new-cluster"], true);
    assert_eq!(encoded["lease"], -1);
}

#[test]
fn config_decode_go_json_bytes() {
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
        let go = &log["config"];
        let decoded = decode_json_scalars(&go.to_string()).unwrap();
        let rust = serde_json::to_value(&decoded.values).unwrap();
        // Compare every supported field with real Go output, retaining omission.
        for (key, value) in rust.as_object().unwrap() {
            assert_eq!(value, &go[key], "{label}: {key}");
            assert!(decoded.metadata.is_defined(key), "{label}: {key}");
            // Each scalar's JSON wire bytes must match, not just its Rust type.
            assert_eq!(serde_json::to_vec(value).unwrap(), {
                let pattern = format!("\"{key}\":");
                let raw = line.split_once(&pattern).unwrap().1;
                let mut stream = serde_json::Deserializer::from_str(raw).into_iter::<Value>();
                stream.next().unwrap().unwrap();
                raw.as_bytes()[..stream.byte_offset()].to_vec()
            });
        }
        assert_eq!(decoded.values.pre_vote, label == "missing");
        assert_eq!(
            decoded.values.max_request_bytes,
            if label == "missing" { 157286400 } else { 0 }
        );
        assert!(rust.get("schedule").is_none());
    }
}

#[test]
fn config_decode_type_errors_and_unknown_fields() {
    assert!(decode_json_scalars(r#"{"lease":"oops"}"#).is_err());
    assert!(decode_json_scalars(r#"{"max-request-bytes":-1}"#).is_err());
    assert!(decode_json_scalars("[]").is_err());
    assert!(decode_json_scalars(r#"{"quota-backend-bytes":null}"#).is_err());
    assert!(decode_toml_scalars("lease = 'oops'").is_err());
    assert!(decode_toml_scalars("name = 'one'\nname = 'two'").is_err());
    let value = decode_json_scalars(r#"{"future":0,"name":null}"#).unwrap();
    assert_eq!(value.values.name, "");
    assert!(value.metadata.is_defined("name"));
    assert!(
        value
            .metadata
            .check_undecoded()
            .unwrap()
            .to_string()
            .contains("future")
    );
    let value = decode_toml_scalars("[future]\nx = 0").unwrap();
    assert!(value.metadata.child(["future"]).is_defined("x"));
    assert_eq!(
        serde_json::to_value(value.values).unwrap()["name"],
        json!("")
    );
}

#[test]
fn config_decode_go_toml_input_to_json() {
    let evidence = include_str!("../../docs/rust/theme3-服务配置契约.md");
    for label in ["missing", "explicit-zero"] {
        let input_marker = format!("<!-- batch84-input-{label} -->\n\n```toml\n");
        let input = evidence
            .split_once(&input_marker)
            .unwrap()
            .1
            .split_once("```")
            .unwrap()
            .0;
        let log_marker = format!("<!-- batch84-{label} -->\n\n```json\n");
        let log: Value = serde_json::from_str(
            evidence
                .split_once(&log_marker)
                .unwrap()
                .1
                .lines()
                .next()
                .unwrap(),
        )
        .unwrap();
        let decoded = decode_toml_scalars(input).unwrap();
        let rust = serde_json::to_value(&decoded.values).unwrap();
        for key in ["name", "data-dir", "client-urls", "peer-urls"] {
            assert_eq!(rust[key], log["config"][key]);
        }
        assert_eq!(
            decoded.metadata.is_defined("max-request-bytes"),
            label == "explicit-zero"
        );
        if label == "explicit-zero" {
            // Go decodes the untagged field name, but Adjust checks the
            // different hyphenated metadata key and consequently sets true.
            assert!(!decoded.values.enable_grpc_gateway);
            assert_eq!(log["config"]["enable-grpc-gateway"], true);
            for key in [
                "enable-prevote",
                "max-request-bytes",
                "quota-backend-bytes",
                "auto-compaction-retention-v2",
                "max-concurrent-tso-proxy-streamings",
                "lease",
            ] {
                assert_eq!(rust[key], log["config"][key], "{key}");
            }
        }
    }
}
