// Copyright 2026 AsterSQL.
//! Classic operator mutations backed by the host's canonical scheduling queue.
use crate::{
    PdServices,
    http_contract::{json_response, query_unescape, read_json_object},
    request_context,
};
use axum::{
    body::Body,
    http::{Method, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[derive(Clone, Debug)]
pub enum Command {
    Create(Value),
    Delete(Option<u64>),
}
pub(crate) async fn handle(
    services: Arc<dyn PdServices>,
    cancel: CancellationToken,
    uri: Uri,
    method: Method,
    body: Body,
) -> Response {
    let path = uri
        .path()
        .strip_prefix("/pd/api/v1/operators")
        .unwrap_or_default();
    if method == Method::POST && !path.is_empty() {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let cancel = request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| cancel.child_token());
    let work = async {
        let command = if method == Method::POST {
            let input = match read_json_object(body).await {
                Ok(v) => v.map(Value::Object).unwrap_or(Value::Null),
                Err(e) => return e.into_response(),
            };
            if let Err(e) = astersql_pd_schedule_handler::OperatorCreation::parse(&input) {
                return json_response(StatusCode::BAD_REQUEST, &e.to_string());
            }
            Command::Create(input)
        } else if path.is_empty() {
            Command::Delete(None)
        } else {
            let raw = &path[1..];
            let decoded = query_unescape(&raw.replace('+', "%2B"))
                .unwrap_or_else(|_| raw.as_bytes().to_vec());
            let text = String::from_utf8_lossy(&decoded);
            match super::http_operator_read::decimal(&text) {
                Ok(id) => Command::Delete(Some(id)),
                Err(reason) => {
                    return super::http_operator_read::number_error("ParseUint", &text, reason);
                }
            }
        };
        match services.is_bootstrapped().await {
            Ok(true) => {}
            Ok(false) => {
                let e = "[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first";
                return if matches!(&command,Command::Create(v) if v["name"]=="scatter-regions") {
                    json_response(StatusCode::OK, &json!({"processed-percentage":0,"error":e}))
                } else {
                    json_response(StatusCode::INTERNAL_SERVER_ERROR, &e)
                };
            }
            Err(e) => return json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        }
        match services.operator_write(command).await {
            Ok((status, value)) => json_response(
                StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                &value,
            ),
            Err(e) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        }
    };
    tokio::select! {biased;_=cancel.cancelled()=>json_response(StatusCode::INTERNAL_SERVER_ERROR,&"server not started"), response=work=>response}
}
