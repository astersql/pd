// Copyright 2026 AsterSQL.
//! Preconditions shared by the read-only process harness.
use std::{os::unix::fs::PermissionsExt, path::Path};

/// Require a real PD module and the exact protocol revision used by the wire tests.
pub fn validate_go_build_info(info: &str) -> Result<(), String> {
    let records: Vec<Vec<&str>> = info
        .lines()
        .map(|line| line.split_whitespace().collect())
        .collect();
    if !records
        .iter()
        .any(|r| r.len() >= 3 && matches!(r[0], "dep" | "mod") && r[1] == "github.com/tikv/pd")
    {
        return Err("oracle is not a Go PD artifact".into());
    }
    if !records.iter().any(|r| {
        r.len() >= 3
            && r[0] == "dep"
            && r[1] == "github.com/pingcap/kvproto"
            && r[2] == "v0.0.0-20261016101550-f8ad079c8e98"
    }) {
        return Err("oracle must use pinned kvproto f8ad079c8e98".into());
    }
    Ok(())
}

/// Reject missing process inputs before starting either implementation.
pub fn validate(mode: &str, go: &Path, rust: Option<&Path>, fixture: &Path) -> Result<(), String> {
    let binaries = match mode {
        "go" => vec![go],
        "rust" => vec![rust.ok_or("PD_COMPAT_RUST_BIN required")?],
        "dual" => vec![go, rust.ok_or("PD_COMPAT_RUST_BIN required")?],
        _ => return Err("mode must be go, rust or dual".into()),
    };
    for binary in &binaries {
        let metadata = binary
            .metadata()
            .map_err(|e| format!("binary {}: {e}", binary.display()))?;
        if !binary.is_absolute()
            || !metadata.is_file()
            || metadata.permissions().mode() & 0o111 == 0
        {
            return Err(format!(
                "absolute executable binary required: {}",
                binary.display()
            ));
        }
        if binary
            .file_name()
            .is_some_and(|name| name == "pd-compat-different-service")
        {
            return Err("different-service is a negative control, not an oracle".into());
        }
    }
    if binaries.len() == 2
        && binaries[0].canonicalize().map_err(|e| e.to_string())?
            == binaries[1].canonicalize().map_err(|e| e.to_string())?
    {
        return Err("dual requires distinct Go and Rust binaries".into());
    }
    let data = std::fs::read(fixture).map_err(|e| format!("fixture {}: {e}", fixture.display()))?;
    let value: serde_json::Value = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    if !value.as_object().is_some_and(|v| !v.is_empty()) {
        return Err("nonempty object fixture required".into());
    }
    Ok(())
}

/// Require the exact Go artifact admitted by the control-plane plan, without rebuilding Go.
pub fn pinned_go_binary() -> Result<std::path::PathBuf, String> {
    use sha2::{Digest, Sha256};
    use std::{path::PathBuf, process::Command};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary =
        PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").ok_or("PD_COMPAT_GO_BIN required")?);
    validate(
        "go",
        &binary,
        None,
        &root.join("tests/rust-compat/fixtures/health-member.json"),
    )?;
    let info = Command::new("go")
        .args(["version", "-m"])
        .arg(&binary)
        .output()
        .map_err(|e| e.to_string())?;
    if !info.status.success() {
        return Err("cannot read Go build metadata".into());
    }
    validate_go_build_info(&String::from_utf8(info.stdout).map_err(|e| e.to_string())?)?;
    let admission = Command::new("gzip")
        .arg("-dc")
        .arg(root.join("docs/rust/control-plane-mcs/task-001-evidence.json.gz"))
        .output()
        .map_err(|e| e.to_string())?;
    if !admission.status.success() {
        return Err("cannot read admission evidence".into());
    }
    let admission: serde_json::Value =
        serde_json::from_slice(&admission.stdout).map_err(|e| e.to_string())?;
    let expected = admission["binaries"]["go_pd"]["sha256"]
        .as_str()
        .ok_or("admission is missing Go artifact digest")?;
    let actual = format!(
        "{:x}",
        Sha256::digest(std::fs::read(&binary).map_err(|e| e.to_string())?)
    );
    if actual != expected {
        return Err("Go artifact digest differs from admission".into());
    }
    Ok(binary)
}
