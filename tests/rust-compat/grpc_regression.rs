// Copyright 2026 AsterSQL.
//! Reuse the reviewed TCP/tonic/MetaStore scenarios instead of copying domain behavior.
use std::{path::Path, process::Command};

/// Every implemented classic PD method has one required real differential entrypoint.
/// Owner 78 domain methods remain outside theme 7; its two obsolete watches have an
/// implemented exact Unimplemented contract and therefore remain in this catalog.
pub(crate) fn cases() -> Vec<(&'static str, &'static [&'static str])> {
    vec![
        ("members", &["GetMembers", "GetClusterInfo"]),
        ("classic-tso", &["Tso", "GetMinTS"]),
        ("bootstrap", &["Bootstrap", "IsBootstrapped"]),
        ("alloc-id", &["AllocID"]),
        (
            "store-registration",
            &["GetStore", "PutStore", "GetAllStores"],
        ),
        ("cluster-host", &["StoreHeartbeat", "RegionHeartbeat"]),
        ("bucket-heartbeat", &["ReportBuckets"]),
        ("sync-regions", &["SyncRegions"]),
        (
            "region-lookup",
            &["GetRegion", "GetPrevRegion", "GetRegionByID"],
        ),
        ("region-scan", &["ScanRegions", "BatchScanRegions"]),
        ("region-query", &["QueryRegion"]),
        ("ask-split", &["AskSplit", "AskBatchSplit"]),
        ("report-split", &["ReportSplit", "ReportBatchSplit"]),
        ("cluster-config", &["GetClusterConfig", "PutClusterConfig"]),
        ("scatter", &["ScatterRegion", "GetOperator"]),
        ("split-regions", &["SplitRegions"]),
        ("split-scatter", &["SplitAndScatterRegions"]),
        ("global-config", &["StoreGlobalConfig", "LoadGlobalConfig"]),
        ("global-watch", &["WatchGlobalConfig"]),
        ("report-min-resolved-ts", &["ReportMinResolvedTS"]),
        (
            "external-timestamp",
            &["SetExternalTimestamp", "GetExternalTimestamp"],
        ),
        (
            "deprecated-rpc",
            &[
                "SyncMaxTS",
                "GetDCLocationInfo",
                "WatchGCSafePointV2",
                "WatchGCStates",
            ],
        ),
        ("admin-markers", &["IsSnapshotRecovering"]),
    ]
}

pub(crate) fn run(root: &Path) {
    let target = std::env::var_os("CARGO_TARGET_DIR").expect("claim a shared Cargo slot first");
    let rust = std::env::var_os("PD_COMPAT_RUST_BIN")
        .unwrap_or_else(|| Path::new(&target).join("debug/pd-server").into_os_string());
    let go = std::env::var_os("PD_COMPAT_GO_BIN")
        .unwrap_or_else(|| root.join("bin/pd-server").into_os_string());
    let mut command = Command::new("bash");
    command.arg(root.join("scripts/run-pd-compat-readonly.sh"));
    for (case, _) in cases() {
        if case != "admin-markers" {
            command.args(["--case", case]);
        }
    }
    // These supervisors enforce nonzero effective tests, read-only inputs and
    // cleanup even on failure/cancellation. Never invoke the legacy Go-writing entrypoint.
    for (index, mut command) in [command, {
        let mut c = Command::new("python3");
        c.arg(root.join("scripts/http-admin-markers-oracle.py"));
        c
    }]
    .into_iter()
    .enumerate()
    {
        let output = command
            .current_dir(root)
            .env("PD_COMPAT_MODE", "dual")
            .env("PD_COMPAT_GO_BIN", &go)
            .env("PD_COMPAT_RUST_BIN", &rust)
            .output()
            .expect("read-only regression supervisor");
        println!("{}", String::from_utf8_lossy(&output.stdout));
        assert!(
            output.status.success(),
            "protocol regression failed: {}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        if index == 0 {
            // Different backend causes are an explicit architecture boundary, not
            // an ignored field: pin both actual messages, including the full Go
            // status/cause chain, rather than comparing only UNKNOWN summaries.
            let transcript = String::from_utf8_lossy(&output.stdout);
            let messages: Vec<_> = transcript
                .lines()
                .filter_map(|line| line.strip_prefix("actual discovery backend error: "))
                .collect();
            assert_eq!(
                messages,
                [
                    "metadata backend unavailable; mutation outcome may be unknown",
                    "[PD:etcd:ErrEtcdMemberList]rpc error: code = DeadlineExceeded desc = context deadline exceeded while waiting for connections to become ready: rpc error: code = DeadlineExceeded desc = context deadline exceeded while waiting for connections to become ready",
                ],
                "discovery fault messages changed or a fault scenario did not execute"
            );
            println!("reviewed backend message difference retained: {messages:?}");
        }
    }
}
