// Copyright 2026 AsterSQL.
//! Scheduler lifecycle requests are serialized with the host's scheduling ticks.
use crate::{
    PdServices,
    http_contract::{json_response, query_unescape, read_json_object},
    request_context,
};
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{OriginalUri, State},
    http::{Method, StatusCode},
    response::{IntoResponse, Response},
    routing::any,
};
use serde_json::Value;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[derive(Clone, Debug)]
pub enum Command {
    List {
        status: String,
        timestamps: bool,
    },
    Create {
        name: String,
        args: Vec<String>,
        input: Value,
    },
    Delete(String),
    Pause {
        name: String,
        delay: i64,
    },
}
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v1/schedulers", any(handle))
        .route("/pd/api/v1/schedulers/{name}", any(handle))
        .with_state((services, cancel))
}
async fn handle(
    State((services, cancel)): State<(Arc<dyn PdServices>, CancellationToken)>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    body: Body,
) -> Response {
    let suffix = uri
        .path()
        .strip_prefix("/pd/api/v1/schedulers")
        .unwrap_or_default();
    if method == Method::OPTIONS {
        return StatusCode::OK.into_response();
    }
    if !(method == Method::POST
        || method == Method::GET && suffix.is_empty()
        || method == Method::DELETE && !suffix.is_empty())
    {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let cancel = request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| cancel.child_token());
    let work = async {
        let name = String::from_utf8_lossy(
            &query_unescape(&suffix.trim_start_matches('/').replace('+', "%2B"))
                .unwrap_or_default(),
        )
        .into_owned();
        let command = if method == Method::GET {
            let pairs: Vec<_> = uri
                .query()
                .unwrap_or_default()
                .split('&')
                .filter(|p| !p.contains(';'))
                .filter_map(|p| {
                    let (k, v) = p.split_once('=').unwrap_or((p, ""));
                    Some((query_unescape(k).ok()?, query_unescape(v).ok()?))
                })
                .collect();
            Command::List {
                status: pairs
                    .iter()
                    .find(|(k, _)| k == b"status")
                    .map(|(_, v)| String::from_utf8_lossy(v).into_owned())
                    .unwrap_or_default(),
                timestamps: pairs.iter().any(|(k, _)| k == b"timestamp"),
            }
        } else if method == Method::DELETE {
            Command::Delete(name)
        } else if suffix.is_empty() {
            let input = match read_json_object(body).await {
                Ok(v) => v.map(Value::Object).unwrap_or(Value::Null),
                Err(e) => return e.into_response(),
            };
            match creation(input) {
                Ok(c) => c,
                Err((s, e)) => return json_response(StatusCode::from_u16(s).unwrap(), &e),
            }
        } else {
            // This endpoint decodes map[string]int64, unlike creation's float64 map.
            let bytes = match to_bytes(body, usize::MAX).await {
                Ok(b) => b,
                Err(_) => return crate::http_contract::HttpError::Internal.into_response(),
            };
            let input: Value = match serde_json::from_slice(&bytes) {
                Ok(v) => v,
                Err(e) => {
                    return crate::http_contract::HttpError::Input(
                        crate::schedule_admin_balance_leader::syntax_error(&bytes, &e),
                    )
                    .into_response();
                }
            };
            if !input.is_null() && !input.is_object() {
                return crate::http_contract::HttpError::Input(format!(
                    "json: cannot unmarshal {} into Go value of type map[string]int64",
                    crate::schedule_admin_balance_leader::json_type(&input)
                ))
                .into_response();
            }
            if let Some(fields) = input.as_object() {
                for value in fields.values() {
                    if !value.is_null() && value.as_i64().is_none() {
                        let kind = crate::schedule_admin_balance_leader::json_type(value);
                        let kind = if value.is_number() {
                            format!("number {value}")
                        } else {
                            kind.into()
                        };
                        return crate::http_contract::HttpError::Input(format!(
                            "json: cannot unmarshal {kind} into Go value of type int64"
                        ))
                        .into_response();
                    }
                }
            }
            let Some(delay) = input.get("delay") else {
                return json_response(StatusCode::BAD_REQUEST, &"missing pause time");
            };
            Command::Pause {
                name,
                delay: delay.as_i64().unwrap_or(0),
            }
        };
        match services.is_bootstrapped().await {
            Ok(true) => {}
            Ok(false) => {
                return json_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &"[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first",
                );
            }
            Err(e) => return json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        }
        match services.scheduler(command).await {
            Ok((s, v)) => json_response(
                StatusCode::from_u16(s).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                &v,
            ),
            Err(e) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        }
    };
    tokio::select! {biased;_=cancel.cancelled()=>json_response(StatusCode::INTERNAL_SERVER_ERROR,&"server not started"),response=work=>response}
}
fn creation(input: Value) -> Result<Command, (u16, String)> {
    let fail = |e: &str| (400, e.to_owned());
    let name = input["name"]
        .as_str()
        .ok_or_else(|| fail("missing scheduler name"))?
        .to_owned();
    let mut registry = astersql_pd_schedule_schedulers::SchedulerRegistry::default();
    registry
        .register_builtins()
        .map_err(|e| (500, e.to_string()))?;
    let typ = registry
        .find_type_by_name(&name)
        .filter(|t| t.as_str() == name || name == "scatter-range")
        .ok_or_else(|| fail("unknown scheduler"))?;
    let mut args = vec![];
    match typ.as_str() {
        "evict-leader-scheduler" => {
            let one = input.get("store_id");
            let many = input.get("store_ids");
            if one.is_some() && many.is_some() {
                return Err(fail("only one of store_id and store_ids can be set"));
            }
            let values = if let Some(m) = many {
                m.as_array()
                    .filter(|v| !v.is_empty())
                    .cloned()
                    .ok_or_else(|| fail("please input a right store id"))?
            } else {
                vec![one.cloned().ok_or_else(|| fail("missing store id"))?]
            };
            let mut ids = vec![];
            for v in values {
                let f = v
                    .as_f64()
                    .filter(|f| *f >= 0. && f.fract() == 0. && *f < 18446744073709551616.)
                    .ok_or_else(|| fail("please input a right store id"))?;
                let id = (f as u64).to_string();
                if !ids.contains(&id) {
                    ids.push(id)
                }
            }
            args.push(ids.join(","));
        }
        "grant-leader-scheduler" => {
            let v = input
                .get("store_id")
                .ok_or_else(|| fail("missing store id"))?;
            let f = v
                .as_f64()
                .ok_or_else(|| fail("please input a right store id"))?;
            args.push(go_uint(f).to_string());
        }
        "shuffle-hot-region-scheduler" => {
            args.push(go_uint(input["limit"].as_f64().unwrap_or(1.)).to_string())
        }
        "grant-hot-region-scheduler" => {}
        "scatter-range-scheduler" => {
            for key in ["start_key", "end_key", "range_name"] {
                let v = option(&input, key).map_err(|e| (500, e))?;
                args.push(if key == "range_name" {
                    v
                } else {
                    String::from_utf8_lossy(&query_unescape(&v).map_err(|e| {
                        (
                            500,
                            match e {
                                crate::http_contract::HttpError::Input(e) => e,
                                _ => "internal server error".into(),
                            },
                        )
                    })?)
                    .into_owned()
                })
            }
        }
        "balance-range-scheduler" => {
            for key in ["rule", "engine", "timeout", "alias"] {
                args.push(if key == "timeout" && input.get(key).is_none() {
                    "30m".into()
                } else {
                    option(&input, key).map_err(|e| (400, e))?
                })
            }
            let ranges = astersql_pd_keyutil::decode_http_key_ranges(input.as_object().unwrap())
                .map_err(|e| (400, e.to_string()))?;
            args.extend(ranges.into_iter().map(|v| {
                v.into_iter()
                    .map(|b| {
                        if b >= 128 {
                            format!("%{b:02X}")
                        } else {
                            char::from(b).to_string()
                        }
                    })
                    .collect::<String>()
            }));
        }
        _ => {}
    }
    Ok(Command::Create {
        name: typ.as_str().into(),
        args,
        input,
    })
}
fn option(input: &Value, key: &str) -> Result<String, String> {
    let v = input
        .get(key)
        .ok_or_else(|| format!("[PD:apiutil:ErrOptionNotExist]the option {key} does not exist"))?;
    v.as_str().map(str::to_owned).ok_or_else(|| {
        format!("[PD:apiutil:ErrOptionTypeInvalid]the type of option {key} is invalid")
    })
}
fn go_uint(f: f64) -> u64 {
    if f >= 18446744073709551616. || f < -9223372036854775808. {
        1 << 63
    } else if f < 0. {
        (f as i64) as u64
    } else {
        f as u64
    }
}
