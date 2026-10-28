// Copyright 2026 AsterSQL.
//! Preconditions shared by the read-only process harness.
use std::{os::unix::fs::PermissionsExt, path::Path};

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
