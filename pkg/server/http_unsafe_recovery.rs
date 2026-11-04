// Copyright 2026 AsterSQL.
use crate::{
    PdServices,
    http_contract::{HttpError, json_response, read_json_object},
    request_context,
};
use astersql_pd_unsaferecovery::{RemoveFailedStoresOptions, StartRequest};
use axum::{
    Router,
    body::Body,
    extract::State,
    http::StatusCode,
    response::Response,
    routing::{get, post},
};
use serde_json::Value;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[derive(Clone)]
struct Context {
    services: Arc<dyn PdServices>,
    cancel: CancellationToken,
}
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v1/admin/unsafe/remove-failed-stores", post(handle))
        .route(
            "/pd/api/v1/admin/unsafe/remove-failed-stores/show",
            get(show),
        )
        .route(
            "/pd/api/v1/admin/unsafe/remove-failed-stores/abort",
            post(abort),
        )
        .with_state(Context { services, cancel })
}
fn option<'a>(input: &'a Value, a: &str, b: &str) -> Result<Option<&'a Value>, String> {
    if input.get(a).is_some() && input.get(b).is_some() {
        return Err(format!("{a} is specified multiple times"));
    }
    Ok(input.get(a).or_else(|| input.get(b)))
}
fn seconds(value: &Value, name: &str) -> Result<u64, String> {
    let n = value.as_f64().ok_or_else(|| format!("{name} is invalid"))?;
    if n <= 0. || n.fract() != 0. || n > 4_611_686_018. {
        return Err(format!("{name} is invalid"));
    }
    Ok(n as u64)
}
fn parse(input: Value) -> Result<StartRequest, String> {
    let auto = input
        .get("auto-detect")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut stores = std::collections::BTreeSet::new();
    if !auto {
        for id in input
            .get("stores")
            .and_then(Value::as_array)
            .ok_or("Store ids are invalid")?
        {
            stores.insert(
                astersql_pd_schedule_handler::go_json_u64(id).ok_or("Store ids are invalid")?,
            );
        }
    }
    let timeout = input
        .get("timeout")
        .map(|v| seconds(v, "timeout"))
        .transpose()?
        .unwrap_or(600);
    let period = option(&input, "plan-execution-timeout", "plan_execution_timeout")?
        .map(|v| seconds(v, "plan-execution-timeout"))
        .transpose()?
        .map(|n| (n as i64) * 1_000_000_000)
        .unwrap_or(0);
    let disabled = option(&input, "disable-paranoid-check", "disable_paranoid_check")?
        .map(|v| {
            v.as_bool()
                .ok_or_else(|| "disable-paranoid-check is invalid".to_owned())
        })
        .transpose()?
        .unwrap_or(false);
    Ok(StartRequest {
        failed_stores: stores,
        timeout_seconds: timeout,
        auto_detect: auto,
        options: RemoveFailedStoresOptions {
            plan_execution_timeout_nanos: period,
            disable_paranoid_check: disabled,
        },
    })
}
async fn handle(State(s): State<Context>, body: Body) -> Response {
    let cancel = request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| s.cancel.child_token());
    let work = async {
        if !s.services.is_bootstrapped().await.unwrap_or(false) {
            return json_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &"[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first",
            );
        }
        let input = match read_json_object(body).await {
            Ok(Some(map)) => Value::Object(map),
            Ok(None) => Value::Null,
            Err(HttpError::Input(e)) => return json_response(StatusCode::BAD_REQUEST, &e),
            Err(_) => {
                return json_response(StatusCode::INTERNAL_SERVER_ERROR, &"internal server error");
            }
        };
        let request = match parse(input) {
            Ok(request) => request,
            Err(e) => return json_response(StatusCode::BAD_REQUEST, &e),
        };
        match s.services.start_unsafe_recovery(request).await {
            Ok(()) => json_response(StatusCode::OK, &"Request has been accepted."),
            Err(e) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        }
    };
    tokio::select! {biased;_=cancel.cancelled()=>json_response(StatusCode::INTERNAL_SERVER_ERROR,&"context canceled"),response=work=>response}
}

async fn show(State(s): State<Context>) -> Response {
    if !s.services.is_bootstrapped().await.unwrap_or(false) {
        return json_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &"[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first",
        );
    }
    match s.services.show_unsafe_recovery().await {
        Err(e) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        Ok(output) => {
            let values = output
                .into_iter()
                .map(|o| {
                    let mut v = serde_json::Map::new();
                    v.insert("info".into(), Value::String(o.info));
                    if let Some(nanos) = o.unix_nanos {
                        let time = chrono::DateTime::from_timestamp(
                            nanos.div_euclid(1_000_000_000) as i64,
                            nanos.rem_euclid(1_000_000_000) as u32,
                        )
                        .expect("host wall clock");
                        v.insert(
                            "time".into(),
                            Value::String(
                                time.with_timezone(&chrono::Local)
                                    .format("%Y-%m-%d %H:%M:%S%.3f")
                                    .to_string(),
                            ),
                        );
                    }
                    if !o.actions.is_empty() {
                        v.insert(
                            "actions".into(),
                            serde_json::to_value(o.actions).expect("plain actions"),
                        );
                    }
                    if !o.details.is_empty() {
                        v.insert(
                            "details".into(),
                            serde_json::to_value(o.details).expect("plain details"),
                        );
                    }
                    Value::Object(v)
                })
                .collect::<Vec<_>>();
            json_response(StatusCode::OK, &values)
        }
    }
}

async fn abort(State(s): State<Context>) -> Response {
    if !s.services.is_bootstrapped().await.unwrap_or(false) {
        return json_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &"[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first",
        );
    }
    let cancel = request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| s.cancel.child_token());
    let work = async {
        match s.services.abort_unsafe_recovery().await {
            Ok(()) => json_response(StatusCode::OK, &"Request has been accepted."),
            Err(e) => {
                let invalid = astersql_pd_unsaferecovery::RecoveryError::Invalid(
                    "no ongoing unsafe recovery".into(),
                )
                .to_string();
                json_response(
                    if e == invalid {
                        StatusCode::BAD_REQUEST
                    } else {
                        StatusCode::INTERNAL_SERVER_ERROR
                    },
                    &e,
                )
            }
        }
    };
    tokio::select! {biased;_=cancel.cancelled()=>json_response(StatusCode::INTERNAL_SERVER_ERROR,&"context canceled"),response=work=>response}
}
