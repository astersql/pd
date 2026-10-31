// Copyright 2026 AsterSQL.
//! Classic reset-ts adapter; all window state belongs to the host's Oracle.
use crate::{
    PdServices,
    http_contract::{parse_uint64_field, read_json_object},
    request_context,
};
use axum::{
    Router,
    body::Body,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[derive(Clone)]
struct Context {
    services: Arc<dyn PdServices>,
    cancel: CancellationToken,
}
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v1/admin/reset-ts", post(handle))
        .with_state(Context { services, cancel })
}
async fn handle(State(s): State<Context>, body: Body) -> Response {
    let cancel = request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| s.cancel.child_token());
    let work = async {
        let input = match read_json_object(body).await {
            Ok(v) => v.unwrap_or_default(),
            Err(e) => return e.into_response(),
        };
        let tso = match input
            .get("tso")
            .and_then(|v| v.as_str())
            .and_then(|v| parse_uint64_field(Some(v), "tso").ok())
        {
            Some(v) => v,
            None => return json_response(StatusCode::BAD_REQUEST, &"invalid tso value"),
        };
        let force = match input.get("force-use-larger") {
            None => false,
            Some(serde_json::Value::Bool(v)) => *v,
            _ => return json_response(StatusCode::BAD_REQUEST, &"invalid force-use-larger value"),
        };
        match s.services.reset_timestamp(tso, force).await {
            Ok(()) => json_response(StatusCode::OK, &"Reset ts successfully."),
            Err(e) if e.starts_with("[PD:etcd:ErrEtcdTxnConflict]") => json_response(
                StatusCode::SERVICE_UNAVAILABLE,
                &format!("It's a temporary failure with error {e}, please retry."),
            ),
            Err(e) if e == "[PD:server:ErrServerNotStarted]server not started" => {
                json_response(StatusCode::INTERNAL_SERVER_ERROR, &e)
            }
            Err(e) => json_response(StatusCode::FORBIDDEN, &e),
        }
    };
    tokio::select! {biased;_=cancel.cancelled()=>json_response(StatusCode::INTERNAL_SERVER_ERROR,&"context canceled"),response=work=>response}
}

// The TSO handler uses its own Go renderer, whose JSON encoder appends a newline.
fn json_response(status: StatusCode, value: &str) -> Response {
    (
        status,
        [(
            axum::http::header::CONTENT_TYPE,
            "application/json; charset=UTF-8",
        )],
        format!("{}\n", serde_json::to_string(value).unwrap()),
    )
        .into_response()
}
