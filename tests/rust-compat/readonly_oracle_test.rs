// Copyright 2026 AsterSQL.
use crate::readonly_oracle::validate;
use std::{path::Path, process::Command};

#[test]
fn tail102_inputs_reject_missing_old_and_wrong_artifacts() {
    use crate::readonly_oracle::validate_go_build_info;
    assert!(validate_go_build_info("").is_err());
    assert!(validate_go_build_info("\tdep\tgithub.com/tikv/pd\t(devel)\n\tdep\tgithub.com/pingcap/kvproto\tv0.0.0-20260903054228-107095f1d250\n").is_err());
    assert!(
        validate_go_build_info(
            "\tdep\tgithub.com/pingcap/kvproto\tv0.0.0-20261016101550-f8ad079c8e98\n"
        )
        .is_err()
    );
    assert!(validate_go_build_info("\tdep\tgithub.com/tikv/pd\t(devel)\n\tdep\tgithub.com/pingcap/kvproto\tv0.0.0-20261016101550-f8ad079c8e98\n").is_ok());
    let output = Command::new("python3")
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .args([
            "-c",
            r#"
import importlib.util
s = importlib.util.spec_from_file_location('oracle', 'scripts/pd-compat-readonly.py')
m = importlib.util.module_from_spec(s)
s.loader.exec_module(m)
pd = '\tdep\tgithub.com/tikv/pd\t(devel)\n'
pin = '\tdep\tgithub.com/pingcap/kvproto\tv0.0.0-20261016101550-f8ad079c8e98\n'
for info in ['', pin, pd + pin.replace('f8ad079c8e98', '107095f1d250')]:
    try:
        m.validate_go_build_info(info)
    except ValueError:
        pass
    else:
        raise AssertionError('invalid artifact accepted')
m.validate_go_build_info(pd + pin)
"#,
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "requires a real prebuilt Go oracle and a Go build-info inspector"]
fn tail102_inputs_real_build_info() {
    let binary = std::env::var("PD_COMPAT_GO_BIN").expect("real Go oracle required");
    let inspector = std::env::var("PD_COMPAT_GO_INSPECTOR").expect("Go inspector required");
    let info = Command::new(inspector)
        .args(["version", "-m", &binary])
        .output()
        .unwrap();
    assert!(info.status.success());
    crate::readonly_oracle::validate_go_build_info(&String::from_utf8(info.stdout).unwrap())
        .unwrap();
    let output = Command::new(binary).arg("--version").output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Release Version:"));
}

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
