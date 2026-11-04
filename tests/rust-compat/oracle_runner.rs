// Copyright 2026 AsterSQL.
//! Pinned input and execution receipts over the existing real-process supervisor.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{path::PathBuf, process::Command};

pub(crate) struct Input {
    pub path: PathBuf,
    pub sha256: String,
}

pub(crate) fn digest(path: &std::path::Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub(crate) struct Inputs {
    pub go: Input,
    pub rust: Input,
    pub fixture: Input,
}

impl Inputs {
    pub fn validate(&self) -> Result<(), String> {
        crate::readonly_oracle::validate(
            "dual",
            &self.go.path,
            Some(&self.rust.path),
            &self.fixture.path,
        )?;
        for input in [&self.fixture, &self.go, &self.rust] {
            if digest(&input.path)? != input.sha256 {
                return Err(format!("input checksum changed: {}", input.path.display()));
            }
        }
        Ok(())
    }
}

pub(crate) fn validate_receipt(report: &Value, inputs: &Inputs) -> Result<(), String> {
    if report["exit"] != 0
        || report["readonly_unchanged"] != true
        || report["before"] != report["after"]
        || report["inputs_unchanged"] != true
        || report["work_directory_cleaned"] != true
        || !report["before"]["files"]
            .as_object()
            .is_some_and(|files| !files.is_empty())
    {
        return Err("failed execution, input identity or cleanup guard".into());
    }
    let commands = report["commands"].as_array().ok_or("missing commands")?;
    if commands.len() != 1
        || commands[0]["exit"] != 0
        || commands[0]["process_group_cleaned"] != true
        || commands[0]["test_summaries"] != serde_json::json!([["1", "0", "0"]])
    {
        return Err("expected one effective real scenario with cleanup".into());
    }
    crate::readonly_oracle::validate_go_build_info(
        report["go_build_info"]
            .as_str()
            .ok_or("missing build info")?,
    )?;
    for input in [&inputs.go, &inputs.rust, &inputs.fixture] {
        let path = input.path.canonicalize().map_err(|e| e.to_string())?;
        if report["inputs"][path.to_string_lossy().as_ref()] != input.sha256 {
            return Err(format!("receipt input mismatch: {}", input.path.display()));
        }
    }
    Ok(())
}

/// Run an existing scenario without reimplementing its domain or normalization.
pub(crate) fn run(case: &str, inputs: &Inputs) -> Result<Value, String> {
    inputs.validate()?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("python3")
        .current_dir(&root)
        .arg(root.join("scripts/pd-compat-readonly.py"))
        .args([
            "--mode",
            "dual",
            "--require-pinned-protocol",
            "--case",
            case,
        ])
        .env("PD_COMPAT_GO_BIN", &inputs.go.path)
        .env("PD_COMPAT_RUST_BIN", &inputs.rust.path)
        .output()
        .map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("{stdout}");
    if !output.status.success() {
        return Err(format!(
            "oracle replay {}: {}\n{stdout}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let path = stdout
        .lines()
        .find_map(|line| line.strip_prefix("read-only evidence: "))
        .ok_or("missing real execution receipt")?;
    let report: Value =
        serde_json::from_slice(&std::fs::read(path).map_err(|e| format!("receipt: {e}"))?)
            .map_err(|e| e.to_string())?;
    validate_receipt(&report, inputs)?;
    inputs.validate()?;
    Ok(report)
}
