// Copyright 2026 AsterSQL.
use crate::readonly_oracle::validate;
use std::{path::Path, process::Command};

#[test]
fn task_004_behavior() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/health-member.json");
    assert!(
        validate("dual", Path::new(""), None, &fixture).is_err(),
        "missing real oracle must fail before process startup"
    );
}

#[test]
fn task_004_entry_missing_oracle() {
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/run-pd-compat-readonly.sh");
    let output = Command::new("bash")
        .arg(script)
        .env("PD_COMPAT_MODE", "dual")
        .env_remove("PD_COMPAT_GO_BIN")
        .env_remove("PD_COMPAT_RUST_BIN")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("PD_COMPAT_GO_BIN"));
}

#[test]
fn task_004_rejects_invalid_inputs() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/health-member.json");
    let go = Path::new("/usr/bin/true");
    let rust = Path::new("/usr/bin/false");
    assert!(validate("dual", go, Some(rust), &fixture).is_ok());
    assert!(validate("dual", go, Some(go), &fixture).is_err());
    assert!(validate("unknown", go, Some(rust), &fixture).is_err());
    assert!(validate("go", Path::new("relative"), None, &fixture).is_err());
    assert!(validate("rust", go, Some(&fixture), &fixture).is_err());
    assert!(validate("dual", go, Some(rust), Path::new("/missing/input.json")).is_err());
    let invalid =
        std::env::temp_dir().join(format!("pd-readonly-invalid-{}.json", std::process::id()));
    std::fs::write(&invalid, b"[]").unwrap();
    assert!(validate("dual", go, Some(rust), &invalid).is_err());
    std::fs::remove_file(invalid).unwrap();
}
