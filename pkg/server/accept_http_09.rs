// Copyright 2026 AsterSQL.
//! Keyspace HTTP conversion over the host's canonical domain manager.
use crate::PdServices;
use astersql_pd_keyspace::{
    ConfigMutation, ConfigOp, CreateKeyspaceByIdRequest, CreateKeyspaceRequest,
    GroupAssignmentError, KeyspaceError,
};
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{OriginalUri, State},
    http::{Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
use tokio_util::sync::CancellationToken;
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v2/keyspaces", get(handle).post(handle))
        .route("/pd/api/v2/keyspaces/id", post(handle).get(handle))
        .route("/pd/api/v2/keyspaces/{name}", get(handle))
        .route(
            "/pd/api/v2/keyspaces/{name}/config",
            axum::routing::patch(handle),
        )
        .route(
            "/pd/api/v2/keyspaces/{name}/state",
            axum::routing::put(handle),
        )
        .route("/pd/api/v2/keyspaces/id/{id}", get(handle))
        .route("/pd/api/v2/keyspaces/", get(redirect).post(redirect))
        .route("/pd/api/v2/keyspaces/id/", get(redirect).post(redirect))
        .route("/pd/api/v2/keyspaces/{name}/", get(redirect))
        .route(
            "/pd/api/v2/keyspaces/{name}/config/",
            axum::routing::patch(redirect),
        )
        .route(
            "/pd/api/v2/keyspaces/{name}/state/",
            axum::routing::put(redirect),
        )
        .route("/pd/api/v2/keyspaces/id/{id}/", get(redirect))
        .with_state((services, cancel))
}
pub(crate) fn meta(m: astersql_pd_storage_domain::Keyspace) -> Value {
    let mut v = json!({"id":m.id(),"state":match m.state {0=>"ENABLED".into(),1=>"DISABLED".into(),2=>"ARCHIVED".into(),3=>"TOMBSTONE".into(),n=>n.to_string()}});
    if !m.name.is_empty() {
        v["name"] = json!(m.name);
    }
    if m.created_at != 0 {
        v["created_at"] = json!(m.created_at);
    }
    if m.state_changed_at != 0 {
        v["state_changed_at"] = json!(m.state_changed_at);
    }
    if !m.config.is_empty() {
        v["config"] = json!(m.config);
    }
    v
}
fn domain(e: KeyspaceError) -> (u16, Value) {
    let status = match &e {
        KeyspaceError::ConfigPrecondition(_) | KeyspaceError::Conflict => 409,
        KeyspaceError::Assignment(
            GroupAssignmentError::UnknownGroup | GroupAssignmentError::DisabledGroup,
        ) => 400,
        _ => 500,
    };
    (status, json!(e.to_string()))
}
fn uint(raw: &str, bits: u32, kind: &str) -> Result<u64, (u16, Value)> {
    raw.parse::<u64>()
        .ok()
        .filter(|n| !raw.starts_with(['+', '-']) && (bits == 64 || *n <= u32::MAX as u64))
        .ok_or_else(|| {
            (
                400,
                json!(format!(
                    "strconv.ParseUint: parsing \"{raw}\": {}",
                    if !raw.starts_with(['+', '-'])
                        && raw.bytes().all(|c| c.is_ascii_digit())
                        && !raw.is_empty()
                    {
                        "value out of range"
                    } else {
                        "invalid syntax"
                    }
                )),
            )
        })
        .map_err(|(_, v)| {
            (
                if kind == "id" { 500 } else { 400 },
                if kind == "id" {
                    json!("invalid keyspace id")
                } else {
                    v
                },
            )
        })
}
fn bind_error(message: impl std::fmt::Display) -> (u16, Value) {
    (
        400,
        json!(format!("[PD:gin:ErrBindJSON]{message}: {message}")),
    )
}
fn body(bytes: &[u8], structure: &str) -> Result<Value, (u16, Value)> {
    use serde::Deserialize;
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let value = Value::deserialize(&mut de).map_err(|e| {
        bind_error(if bytes.iter().all(u8::is_ascii_whitespace) {
            "EOF".into()
        } else if e.is_eof() {
            "unexpected EOF".into()
        } else {
            crate::schedule_admin_balance_leader::syntax_error(bytes, &e)
        })
    })?;
    if value.is_null() {
        return Ok(value);
    }
    let fields = value.as_object().ok_or_else(|| {
        bind_error(format!(
            "json: cannot unmarshal {} into Go value of type handlers.{structure}",
            crate::schedule_admin_balance_leader::json_type(&value)
        ))
    })?;
    let mut out = serde_json::Map::new();
    for (key, v) in fields {
        let key = key.to_lowercase();
        let typ = match (structure, key.as_str()) {
            ("CreateKeyspaceByIDParams", "id") => "uint32",
            ("CreateKeyspaceByIDParams" | "CreateKeyspaceParams", "name")
            | ("UpdateStateParam", "state") => "string",
            ("CreateKeyspaceByIDParams" | "CreateKeyspaceParams", "config") => "map[string]string",
            ("UpdateConfigParams", "config" | "preconditions") => "map[string]*string",
            _ => {
                continue;
            }
        };
        if !v.is_null() {
            let valid = match typ {
                "uint32" => v.as_u64().is_some_and(|n| n <= u32::MAX as u64),
                "string" => v.is_string(),
                _ => v.is_object(),
            };
            if !valid {
                return Err(bind_error(format!(
                    "json: cannot unmarshal {} into Go struct field {structure}.{key} of type {typ}",
                    crate::schedule_admin_balance_leader::json_type(v)
                )));
            }
            if let Some(m) = v.as_object() {
                for value in m.values() {
                    if !value.is_null() && !value.is_string() {
                        return Err(bind_error(format!(
                            "json: cannot unmarshal {} into Go struct field {structure}.{key} of type string",
                            crate::schedule_admin_balance_leader::json_type(value)
                        )));
                    }
                }
            }
        }
        out.insert(key, v.clone());
    }
    Ok(Value::Object(out))
}

fn strings(v: &Value, key: &str) -> Result<BTreeMap<String, Option<String>>, (u16, Value)> {
    if v[key].is_null() {
        return Ok(BTreeMap::new());
    }
    let m = v[key]
        .as_object()
        .ok_or_else(|| (400, json!("invalid config")))?;
    m.iter()
        .map(|(k, v)| {
            if v.is_null() {
                Ok((k.clone(), None))
            } else {
                v.as_str()
                    .map(|s| (k.clone(), Some(s.into())))
                    .ok_or_else(|| (400, json!("invalid config value")))
            }
        })
        .collect()
}
async fn handle(
    State((services, cancel)): State<(Arc<dyn PdServices>, CancellationToken)>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    body_data: Body,
) -> Response {
    let cancel = crate::request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| cancel.child_token());
    let _guard = cancel.clone().drop_guard();
    let work = async {
        if !services
            .is_bootstrapped()
            .await
            .map_err(|e| (500, json!(e)))?
        {
            return Ok((
                500,
                json!(
                    "[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first"
                ),
            ));
        }
        let m = services
            .keyspace_manager()
            .await
            .map_err(|e| (500, json!(e)))?;
        let path = uri.path().trim_start_matches("/pd/api/v2/keyspaces");
        let q = query(uri.query().unwrap_or_default());
        let now = crate::grpc_gc_barrier::now()
            .map_err(|e| (500, json!(e.message())))?
            .unix_seconds();
        if method == Method::GET && path.is_empty() {
            let start = uint(
                q.get("page_token")
                    .map(String::as_str)
                    .filter(|s| !s.is_empty())
                    .unwrap_or("0"),
                32,
                "page",
            )? as u32;
            let raw = q
                .get("limit")
                .map(String::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("0");
            let limit = raw.parse::<i64>().map_err(|_| {
                (
                    400,
                    json!(format!(
                        "strconv.ParseInt: parsing \"{raw}\": invalid syntax"
                    )),
                )
            })?;
            let scan = if limit == 0 { 0 } else { limit.wrapping_add(1) };
            let mut values = m
                .load_range(start, if scan <= 0 { 0 } else { scan as usize })
                .await
                .map_err(domain)?;
            let mut next = String::new();
            if scan > 0 && values.len() >= scan as usize {
                next = values.pop().unwrap().id().to_string();
            }
            return Ok((
                200,
                json!({"keyspaces":if values.is_empty(){Value::Null}else{json!(values.into_iter().map(meta).collect::<Vec<_>>())},"next_page_token":next}),
            ));
        }
        if method == Method::GET {
            let loaded = if let Some(raw) = path.strip_prefix("/id/") {
                m.load_by_id(uint(raw, 64, "id")? as u32)
                    .await
                    .map_err(domain)?
            } else {
                let name = crate::http_contract::query_unescape(&path[1..].replace('+', "%2B"))
                    .map_err(|_| (400, json!("invalid URL escape")))?;
                m.load_by_name(&String::from_utf8_lossy(&name))
                    .await
                    .map_err(domain)?
            };
            let mut v = meta(loaded);
            if !path.starts_with("/id/")
                && q.get("force_refresh_group_id").is_some_and(|v| v == "true")
            {
                let groups = services
                    .keyspace_group_manager()
                    .await
                    .map_err(|e| (500, json!(e)))?;
                let id = groups
                    .get_group_by_keyspace_id(v["id"].as_u64().unwrap() as u32)
                    .await
                    .map_err(|e| (500, json!(e.to_string())))?;
                v["config"]["tso_keyspace_group_id"] = json!(id.to_string());
            }
            return Ok((200, v));
        }
        let bytes = to_bytes(body_data, usize::MAX)
            .await
            .map_err(|_| bind_error("unexpected EOF"))?;
        let v = body(
            &bytes,
            if method == Method::POST {
                if path == "/id" {
                    "CreateKeyspaceByIDParams"
                } else {
                    "CreateKeyspaceParams"
                }
            } else if path.ends_with("/state") {
                "UpdateStateParam"
            } else {
                "UpdateConfigParams"
            },
        )?;
        let result = if method == Method::POST {
            let name = v["name"].as_str().unwrap_or_default().to_owned();
            let config: HashMap<_, _> = strings(&v, "config")?
                .into_iter()
                .map(|(k, v)| (k, v.unwrap_or_default()))
                .collect();
            if path == "/id" {
                services
                    .create_keyspace_by_id(
                        CreateKeyspaceByIdRequest {
                            id: if v["id"].is_null() {
                                None
                            } else {
                                Some(
                                    v["id"]
                                        .as_u64()
                                        .filter(|n| *n <= u32::MAX as u64)
                                        .ok_or_else(|| (400, json!("invalid keyspace id")))?
                                        as u32,
                                )
                            },
                            name,
                            config,
                            create_time: now as i64,
                        },
                        cancel.clone(),
                    )
                    .await
            } else {
                services
                    .create_keyspace(
                        CreateKeyspaceRequest {
                            name,
                            config,
                            create_time: now as i64,
                        },
                        cancel.clone(),
                    )
                    .await
            }
        } else {
            let (name, operation) = path[1..].rsplit_once('/').unwrap();
            let name = crate::http_contract::query_unescape(&name.replace('+', "%2B"))
                .map_err(|_| (400, json!("invalid URL escape")))?;
            let name = String::from_utf8_lossy(&name);
            if operation == "config" {
                let patch = strings(&v, "config")?;
                for (key, value) in &patch {
                    if key == "region_bound_type"
                        || (key == "gc_management_type"
                            && value.as_deref() == Some("keyspace_level"))
                    {
                        return Ok((
                            500,
                            json!(format!(
                                "[PD:keyspace:ErrUnsupportedOperationInKeyspace]{}",
                                if key == "region_bound_type" {
                                    "region bound type"
                                } else {
                                    "keyspace level GC"
                                }
                            )),
                        ));
                    }
                }
                let mutations = patch
                    .into_iter()
                    .map(|(key, value)| ConfigMutation {
                        op: if value.is_some() {
                            ConfigOp::Put
                        } else {
                            ConfigOp::Delete
                        },
                        key,
                        value: value.unwrap_or_default(),
                    })
                    .collect::<Vec<_>>();
                m.update_config_with_preconditions(
                    &name,
                    &mutations,
                    &strings(&v, "preconditions")?,
                )
                .await
            } else {
                let state = v["state"].as_str().unwrap_or_default();
                let target = match state.to_uppercase().as_str() {
                    "ENABLED" => 0,
                    "DISABLED" => 1,
                    "ARCHIVED" => 2,
                    "TOMBSTONE" => 3,
                    _ => return Ok((400, json!({}))),
                };
                m.update_state_by_name(&name, target, now as i64).await
            }
        };
        Ok::<_, (u16, Value)>((200, meta(result.map_err(domain)?)))
    };
    let result = tokio::select! {biased;_=cancel.cancelled()=>Err((500,json!("context canceled"))),r=work=>r};
    let (code, value) = result.unwrap_or_else(|e| e);
    (
        StatusCode::from_u16(code).unwrap(),
        [(
            "content-type",
            if code == 400
                && value
                    .as_str()
                    .is_some_and(|s| s.starts_with("[PD:gin:ErrBindJSON]"))
            {
                "text/plain; charset=utf-8"
            } else {
                "application/json; charset=utf-8"
            },
        )],
        serde_json::to_vec_pretty(&value).unwrap(),
    )
        .into_response()
}

fn query(raw: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for part in raw.split('&') {
        let (k, v) = part.split_once('=').unwrap_or((part, ""));
        if let (Ok(k), Ok(v)) = (
            crate::http_contract::query_unescape(k),
            crate::http_contract::query_unescape(v),
        ) {
            out.entry(String::from_utf8_lossy(&k).into_owned())
                .or_insert_with(|| String::from_utf8_lossy(&v).into_owned());
        }
    }
    out
}

async fn redirect(OriginalUri(uri): OriginalUri, method: Method) -> Response {
    let mut location = uri.path().trim_end_matches('/').to_string();
    if let Some(query) = uri.query() {
        location.push('?');
        location.push_str(query);
    }
    let get = method == Method::GET;
    let mut r = (
        if get {
            StatusCode::MOVED_PERMANENTLY
        } else {
            StatusCode::TEMPORARY_REDIRECT
        },
        if get {
            format!("<a href=\"{location}\">Moved Permanently</a>.\n\n")
        } else {
            String::new()
        },
    )
        .into_response();
    r.headers_mut()
        .insert("location", location.parse().unwrap());
    if get {
        r.headers_mut()
            .insert("content-type", "text/html; charset=utf-8".parse().unwrap());
    } else {
        r.headers_mut().remove("content-type");
    }
    r
}
