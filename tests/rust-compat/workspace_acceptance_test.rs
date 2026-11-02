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
