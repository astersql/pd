// Copyright 2026 AsterSQL.
//! Host-owned HTTP access audit boundary. Raw paths, queries, headers and bodies
//! never cross this boundary: route labels contain no caller-controlled values.
use crate::http_service_limit::{ServiceLimit, service_label};
use axum::{extract::Request, middleware::Next, response::Response};
use std::{sync::Arc, time::Duration};

/// Backends selected by Go's route registration, independent of sink implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    pub local_log: bool,
    pub prometheus: bool,
}
/// Sanitized access result. No credentials, payloads or caller-supplied labels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub service: &'static str,
    pub policy: Policy,
    /// None means the handler future was dropped before producing a response.
    pub status: Option<u16>,
    pub elapsed: Duration,
}
/// The host implements a bounded, nonblocking sink; failures cannot change replies.
pub trait Sink: Send + Sync {
    fn record(&self, record: Record) -> Result<(), String>;
}
/// Monotonic host clock, injectable for deterministic duration evidence.
pub trait Clock: Send + Sync {
    fn now(&self) -> Duration;
}
struct Monotonic(std::time::Instant);
impl Clock for Monotonic {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
}
/// Sink and clock owned by the embedding host, with no global runtime or workers.
#[derive(Clone)]
pub struct Audit {
    sink: Arc<dyn Sink>,
    clock: Arc<dyn Clock>,
}
impl Audit {
    pub fn new(sink: Arc<dyn Sink>) -> Self {
        Self::with_clock(sink, Arc::new(Monotonic(std::time::Instant::now())))
    }
    pub fn with_clock(sink: Arc<dyn Sink>, clock: Arc<dyn Clock>) -> Self {
        Self { sink, clock }
    }
}
#[derive(Clone)]
pub(crate) struct State {
    pub limits: ServiceLimit,
    pub audit: Option<Audit>,
}
struct Completion {
    audit: Audit,
    record: Record,
    start: Duration,
}
impl Drop for Completion {
    fn drop(&mut self) {
        self.record.elapsed = self.audit.clock.now().saturating_sub(self.start);
        // A telemetry backend outage must not turn a successful mutation into a retry.
        let _ = self.audit.sink.record(self.record.clone());
    }
}
pub(crate) async fn middleware(
    axum::extract::State(state): axum::extract::State<State>,
    request: Request,
    next: Next,
) -> Response {
    let label = service_label(request.method().as_str(), request.uri().path());
    let Some((audit, policy)) = state.audit.zip(policy(label)) else {
        return next.run(request).await;
    };
    if !state.limits.audit_enabled().await {
        return next.run(request).await;
    }
    let mut completion = Completion {
        start: audit.clock.now(),
        audit,
        record: Record {
            service: label,
            policy,
            status: None,
            elapsed: Duration::ZERO,
        },
    };
    let response = next.run(request).await;
    completion.record.status = Some(response.status().as_u16());
    response
}
/// Only explicit Go audit registrations are eligible. Unknown routes and RPCs bypass.
pub fn policy(label: &str) -> Option<Policy> {
    POLICIES
        .iter()
        .find(|(name, _)| *name == label)
        .map(|(_, p)| *p)
}
// server/api/router.go: setAuditBackend; names use the existing route resolver.
const BOTH: Policy = Policy {
    local_log: true,
    prometheus: true,
};
const LOCAL: Policy = Policy {
    local_log: true,
    prometheus: false,
};
const METRICS: Policy = Policy {
    local_log: false,
    prometheus: true,
};
const POLICIES: &[(&str, Policy)] = &[
    ("GetOperators", METRICS),
    ("CreateOperator", BOTH),
    ("DeleteOperators", BOTH),
    ("GetOperatorRecords", METRICS),
    ("GetOperatorsByRegion", METRICS),
    ("DeleteOperatorByRegion", BOTH),
    ("PauseOrResumeChecker", BOTH),
    ("GetCheckerStatus", METRICS),
    ("GetSchedulers", METRICS),
    ("CreateScheduler", BOTH),
    ("DeleteScheduler", BOTH),
    ("PauseOrResumeScheduler", BOTH),
    ("getDiagnosticResult", METRICS),
    ("HandleSchedulerConfig", BOTH),
    ("GetSchedulerConfig", METRICS),
    ("GetCluster", METRICS),
    ("GetClusterStatus", METRICS),
    ("GetConfig", METRICS),
    ("SetConfig", BOTH),
    ("GetDefaultConfig", METRICS),
    ("GetScheduleConfig", METRICS),
    ("SetScheduleConfig", BOTH),
    ("GetPDServerConfig", METRICS),
    ("GetReplicationConfig", METRICS),
    ("SetReplicationConfig", BOTH),
    ("GetLabelPropertyConfig", METRICS),
    ("SetLabelPropertyConfig", BOTH),
    ("GetClusterVersion", METRICS),
    ("SetClusterVersion", BOTH),
    ("GetReplicationModeConfig", METRICS),
    ("SetReplicationModeConfig", BOTH),
    ("GetAllRules", METRICS),
    ("SetAllRules", BOTH),
    ("BatchRules", BOTH),
    ("GetRuleByGroup", METRICS),
    ("GetRulesByRegion", METRICS),
    ("CheckRegionPlacementRule", METRICS),
    ("GetRulesByKey", METRICS),
    ("GetRuleByGroupAndID", METRICS),
    ("SetRule", BOTH),
    ("DeleteRuleByGroup", BOTH),
    ("GetGroupConfig", METRICS),
    ("SetGroupConfig", BOTH),
    ("DeleteGroupConfig", BOTH),
    ("GetAllGroupConfigs", METRICS),
    ("GetPlacementRules", METRICS),
    ("SetPlacementRules", BOTH),
    ("GetPlacementRuleByGroup", METRICS),
    ("SetPlacementRuleByGroup", BOTH),
    ("DeletePlacementRuleByGroup", BOTH),
    ("GetAllRegionLabelRules", METRICS),
    ("GetRegionLabelRulesByIDs", METRICS),
    ("GetRegionLabelRuleByID", METRICS),
    ("DeleteRegionLabelRule", BOTH),
    ("SetRegionLabelRule", BOTH),
    ("PatchRegionLabelRules", BOTH),
    ("GetRegionLabelByKey", METRICS),
    ("GetRegionLabels", METRICS),
    ("GetStore", METRICS),
    ("DeleteStore", BOTH),
    ("SetStoreState", BOTH),
    ("SetStoreLabel", BOTH),
    ("DeleteStoreLabel", BOTH),
    ("SetStoreWeight", BOTH),
    ("SetStoreLimit", BOTH),
    ("GetAllStores", METRICS),
    ("RemoveTombStone", BOTH),
    ("GetAllStoresLimit", METRICS),
    ("SetAllStoresLimit", BOTH),
    ("GetStoresProgress", METRICS),
    ("GetStoresByState", METRICS),
    ("GetLabels", METRICS),
    ("GetStoresByLabel", METRICS),
    ("GetHotWriteRegions", METRICS),
    ("GetHotReadRegions", METRICS),
    ("GetHistoryHotRegions", METRICS),
    ("GetHotStores", METRICS),
    ("GetHotBuckets", METRICS),
    ("GetRegionByID", METRICS),
    ("GetRegion", METRICS),
    ("GetRegions", BOTH),
    ("ScanRegions", METRICS),
    ("GetRegionCount", METRICS),
    ("GetStoreRegions", METRICS),
    ("GetKeyspaceRegions", METRICS),
    ("GetTopWriteFlowRegions", METRICS),
    ("GetTopWriteQueryRegions", METRICS),
    ("GetTopReadFlowRegions", METRICS),
    ("GetTopReadQueryRegions", METRICS),
    ("GetTopConfVerRegions", METRICS),
    ("GetTopVersionRegions", METRICS),
    ("GetTopSizeRegions", METRICS),
    ("GetTopKeysRegions", METRICS),
    ("GetTopCPURegions", METRICS),
    ("GetMissPeerRegions", METRICS),
    ("GetExtraPeerRegions", METRICS),
    ("GetPendingPeerRegions", METRICS),
    ("GetDownPeerRegions", METRICS),
    ("GetLearnerPeerRegions", METRICS),
    ("GetEmptyRegions", METRICS),
    ("GetOfflinePeerRegions", METRICS),
    ("GetOverSizedRegions", METRICS),
    ("GetUndersizedRegions", METRICS),
    ("GetSizeHistogram", METRICS),
    ("GetKeysHistogram", METRICS),
    ("GetRegionSiblings", METRICS),
    ("AccelerateRegionsScheduleInRange", BOTH),
    ("AccelerateRegionsScheduleInRanges", BOTH),
    ("ScatterRegions", BOTH),
    ("SplitRegions", BOTH),
    ("GetRangeHoles", METRICS),
    ("CheckRegionsReplicated", METRICS),
    ("GetVersion", METRICS),
    ("GetPDStatus", METRICS),
    ("GetMembers", METRICS),
    ("DeleteMemberByName", BOTH),
    ("DeleteMemberByID", BOTH),
    ("SetMemberPropertyByName", BOTH),
    ("GetLeader", METRICS),
    ("ResignLeader", BOTH),
    ("TransferLeader", BOTH),
    ("GetRegionStatus", METRICS),
    ("DeleteRegionCache", BOTH),
    ("DeleteRegionStorage", BOTH),
    ("DeleteAllRegionCache", BOTH),
    ("SavePersistFile", BOTH),
    ("isSnapshotRecovering", BOTH),
    ("markSnapshotRecovering", BOTH),
    ("unmarkSnapshotRecovering", BOTH),
    ("isPitrRestoreMode", BOTH),
    ("markPitrRestoreMode", BOTH),
    ("unmarkPitrRestoreMode", BOTH),
    ("recoverAllocID", BOTH),
    ("GetServiceMiddlewareConfig", METRICS),
    ("SetServiceMiddlewareConfig", BOTH),
    ("SetRateLimitConfig", BOTH),
    ("SetGRPCRateLimitConfig", BOTH),
    ("SetLogLevel", BOTH),
    ("GetReplicationModeStatus", METRICS),
    ("loadPlugin", METRICS),
    ("unloadPlugin", METRICS),
    ("GetHealthStatus", METRICS),
    ("Ping", METRICS),
    ("queryMetric", METRICS),
    ("Profile", LOCAL),
    ("Trace", LOCAL),
    ("Symbol", LOCAL),
    ("PProfHeap", LOCAL),
    ("PProfMutex", LOCAL),
    ("PProfAllocs", LOCAL),
    ("PProfBlock", LOCAL),
    ("PProfGoroutine", LOCAL),
    ("PProfThreadcreate", LOCAL),
    ("PProfZip", LOCAL),
    ("GetGCSafePoint", METRICS),
    ("DeleteGCSafePoint", BOTH),
    ("GetMinResolvedTS", METRICS),
    ("GetStoreMinResolvedTS", METRICS),
    ("RemoveFailedStores", BOTH),
    ("AbortFailedStoresRemoval", BOTH),
    ("GetFailedStoresRemovalStatus", METRICS),
    ("TransferLocalTSOAllocator", BOTH),
    ("ResetTS", BOTH),
    ("FailPoint", LOCAL),
];
