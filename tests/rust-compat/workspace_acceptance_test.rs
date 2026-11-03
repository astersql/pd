// Copyright 2026 AsterSQL.
use super::workspace_acceptance::maintenance;
use std::path::PathBuf;

#[test]
#[ignore = "requires distinct real Go/Rust PD binaries and pinned etcd"]
fn task_102_maintenance_live_differential() {
    assert_eq!(std::env::var("PD_COMPAT_MODE").as_deref(), Ok("dual"));
    let go = PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/task-102-maintenance-go.json");
    super::readonly_oracle::validate("dual", &go, Some(&rust), &fixture).unwrap();
    assert_eq!(maintenance(&go, false), maintenance(&rust, true));
}

// This validates the metric property independently. Business replay failures are
// reported explicitly and remain failures of the existing full matrix runner.
#[test]
#[ignore = "requires same-round real observation reports"]
fn task_102_revalidate_collected_metric_contracts() {
    let paths: Vec<PathBuf> = serde_json::from_slice(
        &std::fs::read(std::env::var_os("PD_TASK_102_METRIC_REPORTS").unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(paths.len(), 52);
    let reports: Vec<serde_json::Value> = paths
        .iter()
        .map(|p| serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap())
        .collect();
    let mut cases = std::collections::BTreeSet::new();
    let mut failed = Vec::new();
    for report in &reports {
        assert!(cases.insert(report["case"].as_str().unwrap()));
        if report["exit"] != 0 {
            failed.push(report["case"].clone());
        }
    }
    println!("metric property only; original business replay failures: {failed:?}");
    crate::http_regression::validate_live_metrics(&crate::http_regression::catalog(), &reports)
        .unwrap();
}

#[test]
fn task_102_source_evidence_negative_controls() {
    use super::workspace_acceptance::audit_behavior_evidence as check;
    let hash = "a".repeat(64);
    let stale = "b".repeat(64);
    let rows = [("source.rs", hash.as_str())];
    assert!(check(&rows, 1, &rows, true, false).is_ok());
    assert_eq!(check(&[], 1, &rows, true, false), Err("missing source row"));
    assert_eq!(check(&[], 0, &[], true, false), Err("missing source row"));
    assert_eq!(
        check(&[rows[0], rows[0]], 2, &rows, true, false),
        Err("duplicate source row")
    );
    assert_eq!(
        check(&rows, 1, &[("source.rs", stale.as_str())], true, false),
        Err("stale or missing runtime input")
    );
    assert_eq!(
        check(&rows, 1, &rows, true, true),
        Err("missing business evidence")
    );
    assert_eq!(
        check(&rows, 1, &rows, false, false),
        Err("missing business evidence")
    );
}

#[test]
fn task_102_source_audit_record_gate() {
    use super::workspace_acceptance::audit_behavior_evidence;
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/rust/server-api/task-102-resume-evidence.json.gz");
    let output = std::process::Command::new("gzip")
        .arg("-dc")
        .arg(path)
        .output()
        .unwrap();
    assert!(output.status.success());
    let record: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let audit = &record["tail102_source_audit_2026_11_03"];
    for (name, expected) in [("source", 163), ("wire", 20), ("surface", 26)] {
        let changes = audit["manifest_drift"][name]["changed"].as_array().unwrap();
        assert_eq!(changes.len(), expected);
        for change in changes {
            let row = [(
                change["path"].as_str().unwrap(),
                change["current_sha256"].as_str().unwrap(),
            )];
            let snapshots: Vec<_> = change["recorded_input_snapshots"]
                .as_array()
                .unwrap()
                .iter()
                .map(|snapshot| (row[0].0, snapshot["sha256"].as_str().unwrap()))
                .collect();
            // Input snapshots never establish full business admission on their own.
            assert!(audit_behavior_evidence(&row, 1, &snapshots, false, false).is_err());
            assert!(audit_behavior_evidence(&row, 1, &snapshots, true, true).is_err());
            assert_eq!(
                audit_behavior_evidence(&row, 1, &snapshots, true, false).is_ok(),
                !change["current_input_snapshots"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
        }
    }
}
