// Copyright 2026 AsterSQL.
use super::oracle_runner::{Input, Inputs, digest, run, validate_receipt};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

fn inputs() -> Inputs {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let go = PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").expect("real Go oracle required"));
    let rust =
        PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").expect("real Rust PD required"));
    let fixture = root.join("fixtures/health-member.json");
    let admission = std::process::Command::new("gzip")
        .args(["-dc"])
        .arg(root.join("../../docs/rust/control-plane-mcs/task-001-evidence.json.gz"))
        .output()
        .unwrap();
    assert!(admission.status.success());
    let admission: Value = serde_json::from_slice(&admission.stdout).unwrap();
    Inputs {
        go: Input {
            path: go,
            sha256: admission["binaries"]["go_pd"]["sha256"]
                .as_str()
                .unwrap()
                .into(),
        },
        rust: Input {
            sha256: digest(&rust).unwrap(),
            path: rust,
        },
        fixture: Input {
            sha256: format!(
                "{:x}",
                Sha256::digest(include_bytes!("fixtures/health-member.json"))
            ),
            path: fixture,
        },
    }
}

#[test]
fn mcs_003_behavior() {
    let inputs = inputs();
    // Bootstrap includes already-bootstrapped/invalid inputs and durable state;
    // cluster restore validates actual restart with the same WAL directory.
    let mut reports = Vec::new();
    for case in ["health-member", "bootstrap", "cluster-restore"] {
        reports.push(run(case, &inputs).unwrap());
    }
    let path = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap())
        .join("mcs-003-live-receipts.json");
    std::fs::write(path, serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
    let actual = &reports[0];
    for (field, replacement) in [
        ("exit", json!(1)),
        ("readonly_unchanged", json!(false)),
        ("inputs_unchanged", json!(false)),
        ("work_directory_cleaned", json!(false)),
        ("commands", json!([])),
        ("go_build_info", json!("not PD")),
        ("after", json!({})),
    ] {
        let mut broken = actual.clone();
        broken[field] = replacement;
        assert!(
            validate_receipt(&broken, &inputs).is_err(),
            "accepted {field}"
        );
    }
    let mut empty = actual.clone();
    empty["before"] = json!({});
    empty["after"] = json!({});
    assert!(validate_receipt(&empty, &inputs).is_err());
    for field in ["exit", "process_group_cleaned"] {
        let mut broken = actual.clone();
        broken["commands"][0][field] = if field == "exit" {
            json!(1)
        } else {
            json!(false)
        };
        assert!(validate_receipt(&broken, &inputs).is_err());
    }
    for summary in [json!([["0", "0", "0"]]), json!([["1", "0", "1"]])] {
        let mut broken = actual.clone();
        broken["commands"][0]["test_summaries"] = summary;
        assert!(validate_receipt(&broken, &inputs).is_err());
    }
}

#[test]
fn mcs_003_rejects_stale_and_missing_inputs() {
    let mut inputs = inputs();
    inputs.validate().unwrap();
    assert!(run("unknown-domain-scenario", &inputs).is_err());
    let original = inputs.go.sha256.clone();
    inputs.go.sha256 = "0".repeat(64);
    assert!(inputs.validate().unwrap_err().contains("checksum changed"));
    inputs.go.sha256 = original;
    let original = inputs.rust.sha256.clone();
    inputs.rust.sha256 = "0".repeat(64);
    assert!(inputs.validate().unwrap_err().contains("checksum changed"));
    inputs.rust.sha256 = original;
    inputs.fixture.sha256 = "0".repeat(64);
    assert!(inputs.validate().unwrap_err().contains("checksum changed"));
    inputs.go.path = PathBuf::from("/missing/pd-mcs-oracle");
    assert!(inputs.validate().is_err());
    // Missing artifacts and unsupported scenarios must fail, never skip/pass.
    assert!(run("bootstrap", &inputs).is_err());
}
