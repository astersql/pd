// Copyright 2026 AsterSQL.
//! Region-label requests are serialized with the host's canonical scheduler labeler.
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
/// Raw HTTP input, decoded only after the host bootstrap gate.
pub struct Request {
    pub method: Method,
    pub path: String,
    pub body: Vec<u8>,
}
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route(
            "/pd/api/v1/config/region-label/rules",
            get(handle).patch(handle),
        )
        .route("/pd/api/v1/config/region-label/rules/ids", get(handle))
        .route("/pd/api/v1/config/region-label/rule", post(handle))
        .route(
            "/pd/api/v1/config/region-label/rule/{id}",
            get(handle).delete(handle),
        )
        .route("/pd/api/v1/region/id/{id}/labels", get(handle))
        .with_state((services, cancel))
}
async fn handle(
    State((services, cancel)): State<(Arc<dyn PdServices>, CancellationToken)>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    body: Body,
) -> Response {
    let cancel = crate::request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| cancel.child_token());
    let _guard = cancel.clone().drop_guard();
    let work = async {
        let bytes = match to_bytes(body, usize::MAX).await {
            Ok(v) => v.to_vec(),
            Err(e) => return json_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        };
        let delete = method == Method::DELETE;
        match services
            .region_label_rules(Request {
                method,
                path: uri.path().into(),
                body: bytes,
            })
            .await
        {
            Ok((200, _)) if delete => (
                StatusCode::OK,
                [("content-type", "text/plain; charset=UTF-8")],
                "Delete rule successfully.",
            )
                .into_response(),
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
    tokio::select! {biased;_=cancel.cancelled()=>json_response(StatusCode::INTERNAL_SERVER_ERROR,&"context canceled"),r=work=>r}
}

/// Patch envelope; domain rule decoding remains in the canonical labeler.
pub struct Patch<T> {
    pub sets: Vec<T>,
    pub deletes: Vec<String>,
}
impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for Patch<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor<T>(std::marker::PhantomData<T>);
        impl<'de, T: serde::Deserialize<'de>> serde::de::Visitor<'de> for Visitor<T> {
            type Value = Patch<T>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a patch object or null")
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Patch<T>, E> {
                Ok(Patch {
                    sets: Vec::new(),
                    deletes: Vec::new(),
                })
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Patch<T>, M::Error> {
                let mut patch = Patch {
                    sets: Vec::new(),
                    deletes: Vec::new(),
                };
                while let Some(key) = map.next_key::<String>()? {
                    match key.to_ascii_lowercase().as_str() {
                        "sets" => {
                            patch.sets = map.next_value::<Option<Vec<T>>>()?.unwrap_or_default()
                        }
                        "deletes" => {
                            patch.deletes = map
                                .next_value::<Option<Vec<Option<String>>>>()?
                                .unwrap_or_default()
                                .into_iter()
                                .map(Option::unwrap_or_default)
                                .collect()
                        }
                        _ => {
                            map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(patch)
            }
        }
        d.deserialize_any(Visitor(std::marker::PhantomData))
    }
}
