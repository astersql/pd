// Copyright 2026 AsterSQL.
//! HTTP service admission reuses the canonical label controller and middleware codec.
use crate::{PdServices, request_context};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_ratelimit::{Controller, ControllerDone, ControllerOption};
use astersql_pd_server_config::{MiddlewareDimensionConfig, ServiceMiddlewareConfig};
use astersql_pd_storage_endpoint::StorageEndpoint;
use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub(crate) struct ServiceLimit {
    services: Arc<dyn PdServices>,
    controller: Arc<Controller>,
    config: Arc<Mutex<Option<ServiceMiddlewareConfig>>>,
}
impl ServiceLimit {
    pub(crate) fn new(services: Arc<dyn PdServices>, cancel: &CancellationToken) -> Self {
        Self {
            services,
            controller: Arc::new(Controller::new(cancel)),
            config: Arc::new(Mutex::new(None)),
        }
    }
    fn endpoint(&self) -> Result<StorageEndpoint, String> {
        Ok(StorageEndpoint::new(
            self.services
                .recovery_marker_store()
                .ok_or("server not started")?,
        ))
    }
    fn paths(&self) -> ClusterPaths {
        ClusterPaths::new(self.services.identity().cluster_id)
    }
    async fn load(&self, config: &mut Option<ServiceMiddlewareConfig>) -> Result<(), String> {
        if config.is_none() {
            let cfg = self
                .endpoint()?
                .load_service_middleware_config(&self.paths())
                .await
                .map_err(|e| e.to_string())?
                .unwrap_or_else(ServiceMiddlewareConfig::new);
            self.install(&cfg).await?;
            *config = Some(cfg);
        }
        Ok(())
    }
    async fn install(&self, cfg: &ServiceMiddlewareConfig) -> Result<(), String> {
        if let Some(map) = &cfg.rate_limit_config.limiter_config {
            let entries: std::collections::BTreeMap<String, MiddlewareDimensionConfig> =
                serde_json::from_value(serde_json::to_value(map).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            for (label, dim) in entries {
                if !allowlisted(&label) {
                    self.controller.update(&label, &[option(dim)]).await;
                }
            }
        }
        Ok(())
    }
}
fn option(d: MiddlewareDimensionConfig) -> ControllerOption {
    ControllerOption::Dimensions {
        qps: d.qps,
        burst: d.qps_burst,
        concurrency: d.concurrency_limit,
    }
}
fn allowlisted(label: &str) -> bool {
    matches!(label, "SetRateLimitConfig" | "SetGRPCRateLimitConfig")
}
fn text(status: StatusCode, body: impl Into<String>) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        body.into(),
    )
        .into_response()
}
// Complete on response, handler error, cancellation, or unwinding; Done is idempotent.
struct Admission(ControllerDone);
impl Drop for Admission {
    fn drop(&mut self) {
        self.0.done();
    }
}

pub(crate) async fn middleware(
    State(s): State<ServiceLimit>,
    request: Request,
    next: Next,
) -> Response {
    let label = service_label(request.method().as_str(), request.uri().path());
    if label.is_empty() || allowlisted(label) || s.services.recovery_marker_store().is_none() {
        return next.run(request).await;
    }
    let cancel = request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_default();
    let admit = async {
        let mut config = s.config.lock().await;
        if let Err(e) = s.load(&mut config).await {
            return Err(text(StatusCode::INTERNAL_SERVER_ERROR, e));
        }
        if config.as_ref().unwrap().rate_limit_config.enable_rate_limit {
            match s.controller.allow(label).await {
                Ok(done) => Ok(Some(Admission(done))),
                Err(_) => {
                    let mut response = text(StatusCode::TOO_MANY_REQUESTS, "Too Many Requests\n");
                    response
                        .headers_mut()
                        .insert("x-content-type-options", "nosniff".parse().unwrap());
                    Err(response)
                }
            }
        } else {
            Ok(None)
        }
    };
    let admission = tokio::select! { biased;
        _=cancel.cancelled()=>return text(StatusCode::SERVICE_UNAVAILABLE,"request cancelled"),
        result=admit=>match result {Ok(guard)=>guard,Err(response)=>return response}
    };
    // Each existing handler owns its cancellation response contract. The guard
    // survives until it returns or its future is dropped, without replacing that response.
    let response = next.run(request).await;
    drop(admission);
    response
}

pub(crate) fn router(s: ServiceLimit) -> Router {
    Router::new()
        .route(
            "/pd/api/v1/service-middleware/config",
            get(read).post(update),
        )
        .route(
            "/pd/api/v1/service-middleware/config/rate-limit",
            axum::routing::post(rate),
        )
        .with_state(s)
}
async fn cancellable(work: impl std::future::Future<Output = Response>) -> Response {
    let cancel = request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_default();
    tokio::select! { biased; _=cancel.cancelled()=>text(StatusCode::SERVICE_UNAVAILABLE,"request cancelled"), response=work=>response }
}
async fn read(state: State<ServiceLimit>) -> Response {
    cancellable(read_inner(state)).await
}
async fn update(state: State<ServiceLimit>, body: Body) -> Response {
    cancellable(update_inner(state, body)).await
}
async fn rate(state: State<ServiceLimit>, body: Body) -> Response {
    cancellable(rate_inner(state, body)).await
}
async fn read_inner(State(s): State<ServiceLimit>) -> Response {
    let mut cfg = s.config.lock().await;
    match s.load(&mut cfg).await {
        Ok(()) => crate::http_contract::json_response(StatusCode::OK, cfg.as_ref().unwrap()),
        Err(e) => text(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}
async fn update_inner(State(s): State<ServiceLimit>, body: Body) -> Response {
    let input = match crate::http_contract::read_json_object(body).await {
        Ok(v) => v.unwrap_or_default(),
        Err(e) => return e.into_response(),
    };
    if input.is_empty() {
        return text(
            StatusCode::OK,
            "The input is empty.The service-middleware config is updated.",
        );
    }
    let mut config = s.config.lock().await;
    if let Err(e) = s.load(&mut config).await {
        return text(StatusCode::INTERNAL_SERVER_ERROR, e);
    }
    let old = config.as_ref().unwrap();
    let mut value = serde_json::to_value(old).unwrap();
    for (key, val) in input {
        let parts: Vec<_> = key.split('.').collect();
        let (section, leaf) = match parts.as_slice() {
            [leaf] => match *leaf {
                "enable-rate-limit" | "limiter-config" => ("rate-limit", *leaf),
                "enable-audit" => ("audit", *leaf),
                "enable-grpc-rate-limit" | "grpc-limiter-config" => ("grpc-rate-limit", *leaf),
                _ => {
                    return text(
                        StatusCode::BAD_REQUEST,
                        format!("config item {key} not found"),
                    );
                }
            },
            [section @ ("rate-limit" | "audit" | "grpc-rate-limit"), leaf] => (*section, *leaf),
            _ => {
                return text(
                    StatusCode::BAD_REQUEST,
                    format!("config item {key} not found"),
                );
            }
        };
        if value[section].get(leaf).is_none() {
            return text(
                StatusCode::BAD_REQUEST,
                format!("config item {leaf} not found"),
            );
        }
        value[section][leaf] = val;
    }
    let candidate: ServiceMiddlewareConfig = match serde_json::from_value(value) {
        Ok(v) => v,
        Err(e) => return text(StatusCode::BAD_REQUEST, e.to_string()),
    };
    // Persist before publishing. A failed transaction leaves configuration and admissions intact.
    if let Err(e) = s
        .endpoint()
        .unwrap()
        .save_service_middleware_config(&s.paths(), &candidate)
        .await
    {
        return text(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    // Go's generic config update persists values; only the specialized rate-limit endpoint
    // changes live dimensions. The enable switch takes effect immediately.
    *config = Some(candidate);
    text(StatusCode::OK, "The service-middleware config is updated.")
}
async fn rate_inner(State(s): State<ServiceLimit>, body: Body) -> Response {
    let input = match crate::http_contract::read_json_object(body).await {
        Ok(v) => v.unwrap_or_default(),
        Err(e) => return e.into_response(),
    };
    let string = |key: &str| input.get(key).and_then(Value::as_str).unwrap_or("");
    let label = match string("type") {
        "" if !input.get("type").is_some_and(Value::is_string) => {
            return text(StatusCode::BAD_REQUEST, "The type is empty.");
        }
        "label" => {
            let label = string("label");
            if label.is_empty() {
                return text(StatusCode::BAD_REQUEST, "The label is empty.");
            }
            if !ROUTES.iter().any(|r| r.2 == label) {
                return text(StatusCode::BAD_REQUEST, "There is no label matched.");
            }
            label
        }
        "path" => {
            if string("path").is_empty() {
                return text(StatusCode::BAD_REQUEST, "The path is empty.");
            }
            let label = access_label(string("method"), string("path"));
            if label.is_empty() {
                return text(StatusCode::BAD_REQUEST, "There is no label matched.");
            }
            label
        }
        _ => return text(StatusCode::BAD_REQUEST, "The type is invalid."),
    };
    if allowlisted(label) {
        return text(
            StatusCode::BAD_REQUEST,
            "This service is in allow list whose config can not be changed.",
        );
    }
    let mut config = s.config.lock().await;
    if let Err(e) = s.load(&mut config).await {
        return text(StatusCode::INTERNAL_SERVER_ERROR, e);
    }
    let mut value = serde_json::to_value(config.as_ref().unwrap()).unwrap();
    let mut dim: MiddlewareDimensionConfig = serde_json::from_value(
        value["rate-limit"]["limiter-config"]
            .get(label)
            .cloned()
            .unwrap_or(json!({})),
    )
    .unwrap();
    let mut changed = false;
    if let Some(n) = input.get("concurrency").and_then(Value::as_f64) {
        dim.concurrency_limit = n as u64;
        changed = true;
    }
    if let Some(n) = input.get("qps").and_then(Value::as_f64) {
        dim.qps = n;
        dim.qps_burst = if n > 0.0 { (n as i64).max(1) } else { 0 };
        changed = true;
    }
    if !changed {
        return text(StatusCode::OK, "Rate limiter is not changed.");
    }
    let old_dim = config
        .as_ref()
        .unwrap()
        .rate_limit_config
        .limiter_config
        .as_ref()
        .and_then(|m| m.get(label))
        .unwrap_or_default();
    let status = s.controller.update(label, &[option(dim)]).await;
    let deleted = status & 8 != 0;
    if deleted {
        if let Some(map) = value["rate-limit"]["limiter-config"].as_object_mut() {
            map.remove(label);
        }
    } else {
        value["rate-limit"]["limiter-config"][label] = serde_json::to_value(dim).unwrap();
    }
    let candidate: ServiceMiddlewareConfig = serde_json::from_value(value).unwrap();
    if let Err(e) = s
        .endpoint()
        .unwrap()
        .save_service_middleware_config(&s.paths(), &candidate)
        .await
    {
        if deleted {
            s.controller.update(label, &[option(old_dim)]).await;
        }
        return text(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    *config = Some(candidate);
    text(
        StatusCode::OK,
        if deleted {
            "Rate limiter is deleted."
        } else {
            "Rate limiter is updated."
        },
    )
}

/// Go registration names, including methodless routes and ordered templates.
pub(crate) fn service_label(method: &str, path: &str) -> &'static str {
    let Some(path) = path.strip_prefix("/pd/api/v1") else {
        return "";
    };
    ROUTES
        .iter()
        .find(|(methods, template, _)| {
            (method.is_empty() || methods.is_empty() || methods.split('|').any(|m| m == method))
                && (if let Some(prefix) = template.strip_suffix("*") {
                    path.starts_with(prefix)
                } else {
                    path_match(template, path)
                })
        })
        .map_or("", |r| r.2)
}
fn access_label(method: &str, path: &str) -> &'static str {
    let Some(path) = path.strip_prefix("/pd/api/v1") else {
        return "";
    };
    ROUTES
        .iter()
        .find(|(methods, template, _)| {
            template.trim_end_matches('*') == path
                && (methods.is_empty() || methods.split('|').any(|m| m == method))
        })
        .map_or("", |r| r.2)
}
fn path_match(template: &str, path: &str) -> bool {
    let mut parts = path.split('/');
    for expected in template.split('/') {
        let Some(actual) = parts.next() else {
            return false;
        };
        if expected.starts_with('{') {
            if actual.is_empty() {
                return false;
            }
        } else if expected != actual {
            return false;
        }
    }
    parts.next().is_none()
}
// Derived from server/api/router.go registerFunc/registerPrefix names and methods.
// No Go implementation is executed or embedded in the production crate.
const ROUTES: &[(&str, &str, &str)] = &[
    ("GET", "/scheduler-config*", "GetSchedulerConfig"),
    (
        "POST|DELETE|PUT|PATCH",
        "/scheduler-config*",
        "HandleSchedulerConfig",
    ),
    ("GET", "/operators", "GetOperators"),
    ("POST", "/operators", "CreateOperator"),
    ("DELETE", "/operators", "DeleteOperators"),
    ("GET", "/operators/records", "GetOperatorRecords"),
    ("GET", "/operators/{region_id}", "GetOperatorsByRegion"),
    ("DELETE", "/operators/{region_id}", "DeleteOperatorByRegion"),
    ("POST", "/checker/{name}", "PauseOrResumeChecker"),
    ("GET", "/checker/{name}", "GetCheckerStatus"),
    ("GET", "/schedulers", "GetSchedulers"),
    ("POST", "/schedulers", "CreateScheduler"),
    ("DELETE", "/schedulers/{name}", "DeleteScheduler"),
    ("POST", "/schedulers/{name}", "PauseOrResumeScheduler"),
    (
        "GET",
        "/schedulers/diagnostic/{name}",
        "getDiagnosticResult",
    ),
    ("GET", "/cluster", "GetCluster"),
    ("", "/cluster/status", "GetClusterStatus"),
    ("GET", "/config", "GetConfig"),
    ("POST", "/config", "SetConfig"),
    ("GET", "/config/default", "GetDefaultConfig"),
    ("GET", "/config/schedule", "GetScheduleConfig"),
    ("POST", "/config/schedule", "SetScheduleConfig"),
    ("GET", "/config/pd-server", "GetPDServerConfig"),
    ("GET", "/config/replicate", "GetReplicationConfig"),
    ("POST", "/config/replicate", "SetReplicationConfig"),
    ("GET", "/config/label-property", "GetLabelPropertyConfig"),
    ("POST", "/config/label-property", "SetLabelPropertyConfig"),
    ("GET", "/config/cluster-version", "GetClusterVersion"),
    ("POST", "/config/cluster-version", "SetClusterVersion"),
    (
        "GET",
        "/config/replication-mode",
        "GetReplicationModeConfig",
    ),
    (
        "POST",
        "/config/replication-mode",
        "SetReplicationModeConfig",
    ),
    (
        "GET",
        "/config/region-label/rules",
        "GetAllRegionLabelRules",
    ),
    (
        "GET",
        "/config/region-label/rules/ids",
        "GetRegionLabelRulesByIDs",
    ),
    ("POST", "/config/region-label/rule", "SetRegionLabelRule"),
    (
        "PATCH",
        "/config/region-label/rules",
        "PatchRegionLabelRules",
    ),
    ("GET", "/region/id/{id}/label/{key}", "GetRegionLabelByKey"),
    ("GET", "/region/id/{id}/labels", "GetRegionLabels"),
    ("GET", "/store/{id}", "GetStore"),
    ("DELETE", "/store/{id}", "DeleteStore"),
    ("POST", "/store/{id}/state", "SetStoreState"),
    ("POST", "/store/{id}/label", "SetStoreLabel"),
    ("DELETE", "/store/{id}/label", "DeleteStoreLabel"),
    ("POST", "/store/{id}/weight", "SetStoreWeight"),
    ("POST", "/store/{id}/limit", "SetStoreLimit"),
    ("GET", "/stores", "GetAllStores"),
    ("DELETE", "/stores/remove-tombstone", "RemoveTombStone"),
    ("GET", "/stores/limit", "GetAllStoresLimit"),
    ("POST", "/stores/limit", "SetAllStoresLimit"),
    ("GET", "/stores/progress", "GetStoresProgress"),
    ("GET", "/stores/check", "GetStoresByState"),
    ("GET", "/labels", "GetLabels"),
    ("GET", "/labels/stores", "GetStoresByLabel"),
    ("GET", "/hotspot/regions/write", "GetHotWriteRegions"),
    ("GET", "/hotspot/regions/read", "GetHotReadRegions"),
    ("GET", "/hotspot/regions/history", "GetHistoryHotRegions"),
    ("GET", "/hotspot/stores", "GetHotStores"),
    ("GET", "/hotspot/buckets", "GetHotBuckets"),
    ("GET", "/regions/check/miss-peer", "GetMissPeerRegions"),
    ("GET", "/regions/check/extra-peer", "GetExtraPeerRegions"),
    (
        "GET",
        "/regions/check/pending-peer",
        "GetPendingPeerRegions",
    ),
    ("GET", "/regions/check/down-peer", "GetDownPeerRegions"),
    (
        "GET",
        "/regions/check/learner-peer",
        "GetLearnerPeerRegions",
    ),
    ("GET", "/regions/check/empty-region", "GetEmptyRegions"),
    (
        "GET",
        "/regions/check/offline-peer",
        "GetOfflinePeerRegions",
    ),
    (
        "GET",
        "/regions/check/oversized-region",
        "GetOverSizedRegions",
    ),
    (
        "GET",
        "/regions/check/undersized-region",
        "GetUndersizedRegions",
    ),
    (
        "POST",
        "/regions/accelerate-schedule",
        "AccelerateRegionsScheduleInRange",
    ),
    (
        "POST",
        "/regions/accelerate-schedule/batch",
        "AccelerateRegionsScheduleInRanges",
    ),
    ("POST", "/regions/scatter", "ScatterRegions"),
    ("POST", "/regions/split", "SplitRegions"),
    ("GET", "/regions/replicated", "CheckRegionsReplicated"),
    ("GET", "/members", "GetMembers"),
    ("DELETE", "/members/name/{name}", "DeleteMemberByName"),
    ("DELETE", "/members/id/{id}", "DeleteMemberByID"),
    ("POST", "/members/name/{name}", "SetMemberPropertyByName"),
    ("GET", "/leader", "GetLeader"),
    ("POST", "/leader/resign", "ResignLeader"),
    ("POST", "/leader/transfer/{next_leader}", "TransferLeader"),
    ("GET", "/stats/region", "GetRegionStatus"),
    (
        "DELETE",
        "/admin/storage/region/{id}",
        "DeleteRegionStorage",
    ),
    ("POST", "/admin/persist-file/{file_name}", "SavePersistFile"),
    (
        "GET",
        "/admin/cluster/markers/snapshot-recovering",
        "isSnapshotRecovering",
    ),
    (
        "POST",
        "/admin/cluster/markers/snapshot-recovering",
        "markSnapshotRecovering",
    ),
    (
        "DELETE",
        "/admin/cluster/markers/snapshot-recovering",
        "unmarkSnapshotRecovering",
    ),
    (
        "GET",
        "/admin/cluster/markers/pitr-restore-mode",
        "isPitrRestoreMode",
    ),
    (
        "POST",
        "/admin/cluster/markers/pitr-restore-mode",
        "markPitrRestoreMode",
    ),
    (
        "DELETE",
        "/admin/cluster/markers/pitr-restore-mode",
        "unmarkPitrRestoreMode",
    ),
    ("POST", "/admin/base-alloc-id", "recoverAllocID"),
    (
        "GET",
        "/service-middleware/config",
        "GetServiceMiddlewareConfig",
    ),
    (
        "POST",
        "/service-middleware/config",
        "SetServiceMiddlewareConfig",
    ),
    (
        "POST",
        "/service-middleware/config/rate-limit",
        "SetRateLimitConfig",
    ),
    (
        "POST",
        "/service-middleware/config/grpc-rate-limit",
        "SetGRPCRateLimitConfig",
    ),
    ("POST", "/admin/log", "SetLogLevel"),
    ("", "/replication_mode/status", "GetReplicationModeStatus"),
    ("POST", "/plugin", "loadPlugin"),
    ("DELETE", "/plugin", "unloadPlugin"),
    ("GET", "/health", "GetHealthStatus"),
    ("GET", "/ping", "Ping"),
    ("GET|POST", "/metric/query", "queryMetric"),
    ("GET|POST", "/metric/query_range", "queryMetric"),
    ("", "/debug/pprof/profile", "Profile"),
    ("", "/debug/pprof/trace", "Trace"),
    ("", "/debug/pprof/symbol", "Symbol"),
    ("", "/debug/pprof/heap", "PProfHeap"),
    ("", "/debug/pprof/mutex", "PProfMutex"),
    ("", "/debug/pprof/allocs", "PProfAllocs"),
    ("", "/debug/pprof/block", "PProfBlock"),
    ("", "/debug/pprof/goroutine", "PProfGoroutine"),
    ("", "/debug/pprof/threadcreate", "PProfThreadcreate"),
    ("", "/debug/pprof/zip", "PProfZip"),
    ("GET", "/gc/safepoint", "GetGCSafePoint"),
    ("DELETE", "/gc/safepoint/{service_id}", "DeleteGCSafePoint"),
    ("GET", "/min-resolved-ts", "GetMinResolvedTS"),
    (
        "GET",
        "/min-resolved-ts/{store_id}",
        "GetStoreMinResolvedTS",
    ),
    (
        "POST",
        "/admin/unsafe/remove-failed-stores",
        "RemoveFailedStores",
    ),
    (
        "POST",
        "/admin/unsafe/remove-failed-stores/abort",
        "AbortFailedStoresRemoval",
    ),
    (
        "GET",
        "/admin/unsafe/remove-failed-stores/show",
        "GetFailedStoresRemovalStatus",
    ),
    (
        "POST",
        "/tso/allocator/transfer/{name}",
        "TransferLocalTSOAllocator",
    ),
    ("POST", "/admin/reset-ts", "ResetTS"),
];
