// Copyright 2026 AsterSQL.
//! Placement routes share the live term's rule manager and storage.
use crate::{PdServices, http_contract::json_response};
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{OriginalUri, State},
    http::{Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
/// Raw request retained until the host checks bootstrap and feature gates.
pub struct Request {
    pub method: Method,
    pub path: String,
    pub body: Vec<u8>,
}
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v1/config/rule", post(handle))
        .route(
            "/pd/api/v1/config/rule/{group}/{id}",
            get(handle).delete(handle),
        )
        .route("/pd/api/v1/config/rules/group/{group}", get(handle))
        .route("/pd/api/v1/config/rules/key/{key}", get(handle))
        .route("/pd/api/v1/config/rules/region/{region}", get(handle))
        .with_state((services, cancel))
}
async fn handle(
    State((services, cancel)): State<(Arc<dyn PdServices>, CancellationToken)>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    body: Body,
) -> Response {
    // Gorilla matches these routes against the decoded URL path. An encoded
    // slash cannot become an extra identifier segment after route selection.
    for segment in uri.path().split('/') {
        if crate::http_contract::query_unescape(&segment.replace('+', "%2B"))
            .is_ok_and(|s| s.contains(&b'/'))
        {
            return (StatusCode::NOT_FOUND, "404 page not found\n").into_response();
        }
    }
    let cancel = crate::request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| cancel.child_token());
    let _guard = cancel.clone().drop_guard();
    let work = async {
        let bytes = match to_bytes(body, usize::MAX).await {
            Ok(v) => v.to_vec(),
            Err(e) => return json_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        };
        match services
            .rule(Request {
                method,
                path: uri.path().into(),
                body: bytes,
            })
            .await
        {
            Ok((status, value)) => {
                let mut response = json_response(StatusCode::from_u16(status).unwrap(), &value);
                if let Some(code) = value.get("code").and_then(serde_json::Value::as_str) {
                    response
                        .headers_mut()
                        .insert("tidb-error-code", code.parse().unwrap());
                }
                response
            }
            Err(e) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        }
    };
    tokio::select! {biased;_=cancel.cancelled()=>(StatusCode::INTERNAL_SERVER_ERROR,"server not started").into_response(),r=work=>r}
}
/// Go-compatible syntax diagnostics shared by the host adapter.
pub fn parse(bytes: &[u8]) -> Result<serde_json::Value, serde_json::Value> {
    use serde_json::{Value, json};
    let value:Value=serde_json::from_slice(bytes).map_err(|e| {
        let offset=if e.is_eof(){bytes.len()}else{bytes.split_inclusive(|b|*b==b'\n').take(e.line().saturating_sub(1)).map(<[u8]>::len).sum::<usize>()+e.column()};
        json!({"code":"input","msg":crate::schedule_admin_balance_leader::syntax_error(bytes,&e),"data":{"Offset":offset}})
    })?;
    fn mismatch(value: &Value, expected: &str, field: &str, start: usize, end: usize) -> Value {
        let kind = crate::schedule_admin_balance_leader::json_type(value);
        let kind = if value.is_number() && (expected == "int" || expected == "uint64") {
            format!("number {value}")
        } else {
            kind.into()
        };
        let offset = if value.is_array() || value.is_object() {
            start + 1
        } else {
            end
        };
        let structure = if field.is_empty() { "" } else { "Rule" };
        let message = if field.is_empty() {
            format!("json: cannot unmarshal {kind} into Go value of type placement.Rule")
        } else {
            format!(
                "json: cannot unmarshal {kind} into Go struct field Rule.{field} of type {expected}"
            )
        };
        json!({"code":"input","msg":message,"data":{"Value":kind,"Type":{},"Offset":offset,"Struct":structure,"Field":field}})
    }
    if value.is_null() {
        return Ok(value);
    }
    let Some(fields) = value.as_object() else {
        let start = bytes
            .iter()
            .position(|b| !b.is_ascii_whitespace())
            .unwrap_or(0);
        let end = bytes
            .iter()
            .rposition(|b| !b.is_ascii_whitespace())
            .map_or(0, |i| i + 1);
        return Err(mismatch(&value, "placement.Rule", "", start, end));
    };
    let mut ordered = fields
        .iter()
        .filter_map(|(k, v)| {
            crate::http_schedule_config_write::field_bounds(bytes, k)
                .map(|(start, end)| (start, end, k, v))
        })
        .collect::<Vec<_>>();
    ordered.sort_by_key(|f| f.0);
    for (start, end, key, v) in ordered {
        if v.is_null() {
            continue;
        }
        let field = key.to_lowercase();
        let expected = match field.as_str() {
            "group_id" | "id" | "start_key" | "end_key" | "isolation_level" => "string",
            "role" => "placement.PeerRoleType",
            "index" | "count" => "int",
            "version" | "create_timestamp" => "uint64",
            "override" | "is_witness" => "bool",
            "location_labels" => "[]string",
            "label_constraints" => "[]placement.LabelConstraint",
            _ => continue,
        };
        let valid = match expected {
            "string" | "placement.PeerRoleType" => v.is_string(),
            "int" => v.as_i64().is_some(),
            "uint64" => v.as_u64().is_some(),
            "bool" => v.is_boolean(),
            _ => v.is_array(),
        };
        if !valid {
            return Err(mismatch(v, expected, &field, start, end));
        }
    }
    Ok(value)
}
