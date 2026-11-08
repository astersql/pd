// Copyright 2026 AsterSQL.
//! Reuse the reviewed TCP/tonic/MetaStore scenarios instead of copying domain behavior.
use std::{path::Path, process::Command};

/// Each classic PD method has a required differential scenario, including the
/// exact Unimplemented contract for the two obsolete GC watches.
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
        (
            "gc-legacy",
            &[
                "GetGCSafePoint",
                "UpdateGCSafePoint",
                "UpdateServiceGCSafePoint",
            ],
        ),
        (
            "gc-v2",
            &[
                "GetGCSafePointV2",
                "UpdateGCSafePointV2",
                "UpdateServiceSafePointV2",
                "GetAllGCSafePointV2",
            ],
        ),
        ("gc-advance", &["AdvanceTxnSafePoint", "AdvanceGCSafePoint"]),
        ("gc-barrier", &["SetGCBarrier", "DeleteGCBarrier"]),
        (
            "gc-global-barrier",
            &["SetGlobalGCBarrier", "DeleteGlobalGCBarrier"],
        ),
        ("gc-state", &["GetGCState", "GetAllKeyspacesGCStates"]),
    ]
}

pub(crate) fn dual_supervisor(root: &Path) -> Command {
    assert_eq!(
        std::env::var("PD_COMPAT_MODE").as_deref(),
        Ok("dual"),
        "the protocol regression requires both implementations"
    );
    let mut command = Command::new("bash");
    command
        .current_dir(root)
        .arg(root.join("scripts/run-pd-compat-readonly.sh"))
        .arg("--require-pinned-protocol");
    command
}

pub(crate) fn run(root: &Path) {
    let mut command = dual_supervisor(root);
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
