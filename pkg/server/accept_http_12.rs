// Copyright 2026 AsterSQL.
//! Meta-service HTTP binding over the host's shared canonical manager.
use crate::PdServices;
use astersql_pd_keyspace::GroupAssignmentError;
use astersql_pd_storage_domain::CallbackTxnError;
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{OriginalUri, State},
    http::{Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, patch},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};
use tokio_util::sync::CancellationToken;
type Error = (u16, Value);
fn err(e: impl std::fmt::Display) -> Error {
    (500, json!(e.to_string()))
}
fn bad(e: impl std::fmt::Display) -> Error {
    (400, json!(e.to_string()))
}
fn binding(e: impl std::fmt::Display) -> Error {
    bad(format!("[PD:gin:ErrBindJSON]{e}: {e}"))
}
pub(crate) fn router(s: Arc<dyn PdServices>, c: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v2/meta-service-groups", get(handle).patch(handle))
        .route("/pd/api/v2/meta-service-groups/{id}/status", patch(handle))
        .route(
            "/pd/api/v2/meta-service-groups/",
            get(crate::accept_http_09::redirect).patch(crate::accept_http_09::redirect),
        )
        .route(
            "/pd/api/v2/meta-service-groups/{id}/status/",
            patch(crate::accept_http_09::redirect),
        )
        .with_state((s, c))
}
fn decode(bytes: &[u8]) -> Result<Value, Error> {
    Value::deserialize(&mut serde_json::Deserializer::from_slice(bytes)).map_err(|e| {
        binding(if bytes.iter().all(u8::is_ascii_whitespace) {
            "EOF".into()
        } else if e.is_eof() {
            "unexpected EOF".into()
        } else {
            crate::schedule_admin_balance_leader::syntax_error(bytes, &e)
        })
    })
}
// Preserve wire order and repeated fields, as Go's decoder does.
struct Fields(Vec<(String, Value)>);
impl<'de> Deserialize<'de> for Fields {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Fields;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("JSON object")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Fields, M::Error> {
                let mut fields = Vec::new();
                while let Some(field) = map.next_entry()? {
                    fields.push(field);
                }
                Ok(Fields(fields))
            }
        }
        d.deserialize_map(Visitor)
    }
}
fn fields(bytes: &[u8]) -> Result<Vec<(String, Value)>, Error> {
    Fields::deserialize(&mut serde_json::Deserializer::from_slice(bytes))
        .map(|f| f.0)
        .map_err(|e| binding(e.to_string()))
}
fn typ(v: &Value) -> String {
    crate::schedule_admin_balance_leader::json_type(v).into()
}
fn count_type(v: &Value) -> String {
    if v.is_number() {
        format!("number {v}")
    } else {
        typ(v)
    }
}
fn assignment_error(e: CallbackTxnError<GroupAssignmentError>) -> Error {
    match e {
        CallbackTxnError::Callback(GroupAssignmentError::UnknownGroup) => (
            404,
            json!("[PD:keyspace:ErrUnknownMetaServiceGroup]unknown meta-service group"),
        ),
        CallbackTxnError::Callback(GroupAssignmentError::InvalidCount) => {
            bad("[PD:keyspace:ErrInvalidAssignmentCount]assignment count must be non-negative")
        }
        CallbackTxnError::Conflict => (
            409,
            json!(
                "[PD:etcd:ErrEtcdTxnConflict]etcd transaction failed, conflicted and rolled back"
            ),
        ),
        e => err(e),
    }
}
async fn handle(
    State((s, c)): State<(Arc<dyn PdServices>, CancellationToken)>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    body: Body,
) -> Response {
    let c = crate::request_context::current()
        .map(|r| r.cancel)
        .unwrap_or_else(|| c.child_token());
    let _guard = c.clone().drop_guard();
    let status_route = uri.path().ends_with("/status");
    let work = async {
        if !s.is_bootstrapped().await.map_err(err)? {
            return Err(err(
                "[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first",
            ));
        }
        let manager = s.meta_service_group_manager().await.map_err(err)?;
        if method == Method::PATCH {
            let bytes = to_bytes(body, usize::MAX)
                .await
                .map_err(|_| bad("unexpected EOF"))?;
            let v = decode(&bytes)?;
            if status_route {
                if !v.is_null() && !v.is_object() {
                    return Err(binding(format!(
                        "json: cannot unmarshal {} into Go value of type keyspace.MetaServiceGroupStatusPatch",
                        typ(&v)
                    )));
                }
                let mut count = None;
                let mut enabled = None;
                if v.is_object() {
                    for (k, v) in fields(&bytes)? {
                        if v.is_null() {
                            if k.eq_ignore_ascii_case("assignment_count") {
                                count = None;
                            }
                            if k.eq_ignore_ascii_case("enabled") {
                                enabled = None;
                            }
                            continue;
                        }
                        if k.eq_ignore_ascii_case("assignment_count") {
                            count=Some(v.as_i64().ok_or_else(||binding(format!("json: cannot unmarshal {} into Go struct field MetaServiceGroupStatusPatch.assignment_count of type int",count_type(&v))))?);
                        } else if k.eq_ignore_ascii_case("enabled") {
                            enabled=Some(v.as_bool().ok_or_else(||binding(format!("json: cannot unmarshal {} into Go struct field MetaServiceGroupStatusPatch.enabled of type bool",typ(&v))))?);
                        }
                    }
                }
                let id = uri
                    .path()
                    .strip_prefix("/pd/api/v2/meta-service-groups/")
                    .unwrap()
                    .strip_suffix("/status")
                    .unwrap();
                let id = crate::accept_http_09::query(&format!("id={}", id.replace('+', "%2B")))
                    .remove("id")
                    .unwrap();
                manager
                    .patch_status(&id, count, enabled)
                    .await
                    .map_err(assignment_error)?;
            } else {
                if v.is_null() {
                    return Err(bad("request body must be a JSON object"));
                }
                let object = v.as_object().ok_or_else(|| {
                    binding(format!(
                        "json: cannot unmarshal {} into Go value of type map[string]*string",
                        typ(&v)
                    ))
                })?;
                for (_, value) in fields(&bytes)? {
                    if !value.is_null() && !value.is_string() {
                        return Err(binding(format!(
                            "json: cannot unmarshal {} into Go value of type string",
                            typ(&value)
                        )));
                    }
                }
                let mut patch = BTreeMap::new();
                for (id, value) in object {
                    if !value.is_null() && !value.is_string() {
                        return Err(binding(format!(
                            "json: cannot unmarshal {} into Go value of type string",
                            typ(value)
                        )));
                    }
                    let id = id.trim();
                    if id.is_empty() {
                        return Err(bad("group ID cannot be empty or whitespace-only"));
                    }
                    if patch.contains_key(id) {
                        return Err(bad("duplicate meta-service group ID after trimming"));
                    }
                    let address = if value.is_null() {
                        None
                    } else {
                        if id.contains('/') {
                            return Err(bad("group ID cannot contain '/'"));
                        }
                        let address = value.as_str().unwrap().trim();
                        if address.is_empty() {
                            return Err(bad("group addresses cannot be empty or whitespace-only"));
                        }
                        Some(address.to_owned())
                    };
                    patch.insert(id.to_owned(), address);
                }
                s.patch_meta_service_groups(patch, c.clone())
                    .await
                    .map_err(|(code, message)| (code, json!(message)))?;
            }
        }
        let groups = manager.get_groups().await;
        let statuses = manager.get_status().await.map_err(err)?;
        Ok(json!(groups.into_iter().map(|(id,addresses)|{
            let status=statuses.get(&id).cloned().unwrap_or_default();
            json!({"id":id,"addresses":addresses,"assigned_keyspaces":status.assignment_count,"status":status})
        }).collect::<Vec<_>>()))
    };
    let (code, value) = match work.await {
        Ok(value) => (200, value),
        Err(e) => e,
    };
    let ct = if !status_route
        && code == 400
        && value
            .as_str()
            .is_some_and(|s| s.starts_with("[PD:gin:ErrBindJSON]"))
    {
        "text/plain; charset=utf-8"
    } else {
        "application/json; charset=utf-8"
    };
    (
        StatusCode::from_u16(code).unwrap(),
        [("content-type", ct)],
        serde_json::to_string(&value).unwrap(),
    )
        .into_response()
}
