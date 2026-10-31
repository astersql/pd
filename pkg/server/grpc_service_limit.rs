// Copyright 2026 AsterSQL.
//! Admission at the existing PD transport, retaining permits across response streams.
use crate::http_service_limit::ServiceLimit;
use axum::{
    body::Body,
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use futures_util::StreamExt;
use http_body_util::{BodyStream, StreamBody};

// Fixed kvproto PD descriptor labels. Only Go methods calling rateLimitCheck admit.
const LABELS: &[&str] = &[
    "AdvanceGCSafePoint",
    "AdvanceTxnSafePoint",
    "AllocID",
    "AskBatchSplit",
    "AskSplit",
    "BatchScanRegions",
    "Bootstrap",
    "DeleteGCBarrier",
    "DeleteGlobalGCBarrier",
    "GetAllGCSafePointV2",
    "GetAllKeyspacesGCStates",
    "GetAllStores",
    "GetClusterConfig",
    "GetClusterInfo",
    "GetDCLocationInfo",
    "GetExternalTimestamp",
    "GetGCSafePoint",
    "GetGCSafePointV2",
    "GetGCState",
    "GetMembers",
    "GetMinTS",
    "GetOperator",
    "GetPrevRegion",
    "GetRegion",
    "GetRegionByID",
    "GetStore",
    "IsBootstrapped",
    "IsSnapshotRecovering",
    "LoadGlobalConfig",
    "PutClusterConfig",
    "PutStore",
    "QueryRegion",
    "RegionHeartbeat",
    "ReportBatchSplit",
    "ReportBuckets",
    "ReportMinResolvedTS",
    "ReportSplit",
    "ScanRegions",
    "ScatterRegion",
    "SetExternalTimestamp",
    "SetGCBarrier",
    "SetGlobalGCBarrier",
    "SplitAndScatterRegions",
    "SplitRegions",
    "StoreGlobalConfig",
    "StoreHeartbeat",
    "SyncMaxTS",
    "SyncRegions",
    "Tso",
    "UpdateGCSafePoint",
    "UpdateGCSafePointV2",
    "UpdateServiceGCSafePoint",
    "UpdateServiceSafePointV2",
    "WatchGCSafePointV2",
    "WatchGCStates",
    "WatchGlobalConfig",
];
const LIMITED: &[&str] = &[
    "GetMinTS",
    "GetMembers",
    "Tso",
    "Bootstrap",
    "IsBootstrapped",
    "AllocID",
    "IsSnapshotRecovering",
    "GetStore",
    "PutStore",
    "GetAllStores",
    "StoreHeartbeat",
    "ReportBuckets",
    "RegionHeartbeat",
    "GetRegion",
    "GetPrevRegion",
    "GetRegionByID",
    "QueryRegion",
    "ScanRegions",
    "BatchScanRegions",
    "AskSplit",
    "AskBatchSplit",
    "ReportSplit",
    "ReportBatchSplit",
    "GetClusterConfig",
    "PutClusterConfig",
    "ScatterRegion",
    "SyncRegions",
    "GetOperator",
    "SplitRegions",
    "SplitAndScatterRegions",
    "StoreGlobalConfig",
    "LoadGlobalConfig",
    "WatchGlobalConfig",
    "ReportMinResolvedTS",
    "SetExternalTimestamp",
    "GetExternalTimestamp",
];
pub(crate) fn known_label(label: &str) -> bool {
    LABELS.contains(&label)
}
pub(crate) async fn middleware(
    State(state): State<ServiceLimit>,
    request: Request,
    next: Next,
) -> Response {
    let Some(label) = request.uri().path().strip_prefix("/pdpb.PD/") else {
        return next.run(request).await;
    };
    if !LIMITED.contains(&label) {
        return next.run(request).await;
    }
    let cancel = crate::request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_default();
    let admission = tokio::select! { biased;
        _=cancel.cancelled()=>return tonic::Status::unavailable("server not started").into_http::<Body>(),
        result=state.grpc_admit(label)=>match result { Ok(guard)=>guard, Err(e)=>return e.into_http::<Body>() }
    };
    let response = next.run(request).await;
    let (parts, body) = response.into_parts();
    let stream = futures_util::stream::unfold(
        (BodyStream::new(body), admission),
        |(mut stream, guard)| async move {
            let frame = stream.next().await?;
            Some((frame, (stream, guard)))
        },
    );
    Response::from_parts(parts, Body::new(StreamBody::new(stream)))
}
