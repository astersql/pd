// Copyright 2026 AsterSQL.
//! TSO directory HTTP conversions over the host's canonical manager.
use crate::PdServices;
use astersql_pd_storage_domain::{KeyspaceGroup, UserKind};
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{OriginalUri, State},
    http::{Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, sync::Arc};
use tokio_util::sync::CancellationToken;
type Error = (u16, Value);
fn err(e: impl std::fmt::Display) -> Error {
    (500, json!(e.to_string()))
}
fn bad(s: &str) -> Error {
    (400, json!(s))
}
pub(crate) fn router(s: Arc<dyn PdServices>, c: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v2/tso/keyspace-groups", get(handle).post(handle))
        .route(
            "/pd/api/v2/tso/keyspace-groups/{id}",
            get(handle).delete(handle).patch(handle),
        )
        .route(
            "/pd/api/v2/tso/keyspace-groups/{id}/alloc",
            post(handle).patch(handle),
        )
        .route("/pd/api/v2/tso/keyspace-groups/{id}/{*node}", patch(handle))
        .route("/pd/api/v2/tso/keyspace-groups/{id}/split", patch(handle))
        .route("/pd/api/v2/tso/keyspace-groups/{id}/merge", patch(handle))
        .route(
            "/pd/api/v2/tso/keyspace-groups/{id}/keyspaces",
            patch(handle),
        )
        .route(
            "/pd/api/v2/tso/keyspace-groups/",
            get(crate::accept_http_09::redirect).post(crate::accept_http_09::redirect),
        )
        .route(
            "/pd/api/v2/tso/keyspace-groups/{id}/",
            get(crate::accept_http_09::redirect).delete(crate::accept_http_09::redirect),
        )
        .route(
            "/pd/api/v2/tso/keyspace-groups/{id}/alloc/",
            post(crate::accept_http_09::redirect),
        )
        .with_state((s, c))
}
fn group(g: Option<KeyspaceGroup>, hide: bool) -> Value {
    let mut v = json!(g);
    if hide {
        if let Some(o) = v.as_object_mut() {
            o.remove("keyspaces");
        }
    }
    v
}
fn bind(bytes: &[u8], structure: &str) -> Result<Value, Error> {
    use serde::Deserialize;
    let mut d = serde_json::Deserializer::from_slice(bytes);
    let value = Value::deserialize(&mut d).map_err(|e| {
        let message = if bytes.iter().all(u8::is_ascii_whitespace) {
            "EOF".into()
        } else if e.is_eof() {
            "unexpected EOF".into()
        } else {
            crate::schedule_admin_balance_leader::syntax_error(bytes, &e)
        };
        binding(message)
    })?;
    if value.is_null() {
        return Ok(json!({}));
    }
    let fields = value.as_object().ok_or_else(|| {
        binding(format!(
            "json: cannot unmarshal {} into Go value of type handlers.{structure}",
            crate::schedule_admin_balance_leader::json_type(&value)
        ))
    })?;
    let mut out = serde_json::Map::new();
    for (key, value) in fields {
        let key = key.to_lowercase();
        let typ = match (structure, key.as_str()) {
            ("CreateKeyspaceGroupParams", "keyspace-groups") => "[]*endpoint.KeyspaceGroup",
            ("AllocNodesForKeyspaceGroupParams", "replica")
            | ("SetPriorityForKeyspaceGroupParams", "priority") => "int",
            ("SetNodesForKeyspaceGroupParams", "nodes") => "[]string",
            _ => continue,
        };
        if !value.is_null() {
            let valid = if typ == "int" {
                value.as_i64().is_some()
            } else {
                value.is_array()
            };
            if !valid {
                return Err(binding(format!(
                    "json: cannot unmarshal {} into Go struct field {structure}.{key} of type {typ}",
                    crate::schedule_admin_balance_leader::json_type(value)
                )));
            }
            if typ == "[]string" {
                for v in value.as_array().unwrap() {
                    if !v.is_null() && !v.is_string() {
                        return Err(binding(format!(
                            "json: cannot unmarshal {} into Go struct field {structure}.{key} of type string",
                            crate::schedule_admin_balance_leader::json_type(v)
                        )));
                    }
                }
            }
        }
        out.insert(key, value.clone());
    }
    Ok(Value::Object(out))
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
    let mut rendered_error = None;
    let work = async {
        if !s.is_bootstrapped().await.map_err(err)? {
            return Err(err(
                "[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first",
            ));
        }
        let path = uri
            .path()
            .trim_start_matches("/pd/api/v2/tso/keyspace-groups");
        let q = crate::accept_http_09::query(uri.query().unwrap_or_default());
        let hide = q
            .get("hide_keyspaces")
            .is_some_and(|v| v.eq_ignore_ascii_case("true"));
        let parts = path
            .trim_start_matches('/')
            .splitn(2, '/')
            .collect::<Vec<_>>();
        let id = if path.is_empty() {
            0
        } else {
            parts[0]
                .parse::<u64>()
                .ok()
                .filter(|_| !parts[0].starts_with(['+', '-']))
                .map(|id| id as u32)
                .filter(|id| *id <= 4096)
                .ok_or_else(|| bad("invalid keyspace group id"))?
        };
        // Go validates creation and list input before checking API mode.
        let mut create = None;
        if method == Method::POST && path.is_empty() {
            let bytes = to_bytes(body, usize::MAX)
                .await
                .map_err(|_| bad("unexpected EOF"))?;
            let v = bind(&bytes, "CreateKeyspaceGroupParams")?;
            if let Some(groups) = v["keyspace-groups"].as_array() {
                for group in groups {
                    if let Some(fields) = group.as_object() {
                        for (key, value) in fields {
                            let (typ, valid) = match key.to_lowercase().as_str() {
                                "id" => (
                                    "uint32",
                                    value.as_u64().is_some_and(|v| v <= u32::MAX as u64),
                                ),
                                "user-kind" => ("string", value.is_string()),
                                _ => continue,
                            };
                            if !value.is_null() && !valid {
                                let actual = if value.is_number() {
                                    format!("number {value}")
                                } else {
                                    crate::schedule_admin_balance_leader::json_type(value)
                                        .to_string()
                                };
                                return Err(binding(format!(
                                    "json: cannot unmarshal {actual} into Go struct field KeyspaceGroup.keyspace-groups.{} of type {typ}",
                                    key.to_lowercase()
                                )));
                            }
                        }
                    }
                }
            }
            let mut groups: Vec<KeyspaceGroup> = serde_json::from_value(
                v.get("keyspace-groups")
                    .filter(|v| !v.is_null())
                    .cloned()
                    .unwrap_or(json!([])),
            )
            .map_err(err)?;
            for g in &mut groups {
                if g.id > 4096 {
                    return Err(bad("invalid keyspace group id"));
                }
                if g.user_kind.is_empty() {
                    g.user_kind = "basic".into();
                } else if !UserKind::is_valid(&g.user_kind) {
                    return Err(bad("invalid user kind"));
                }
            }
            create = Some(groups);
        } else if method != Method::GET && method != Method::DELETE {
            let m = s.keyspace_group_manager().await.map_err(err)?;
            let bytes = to_bytes(body, usize::MAX)
                .await
                .map_err(|_| bad("unexpected EOF"))?;
            let v = bind(
                &bytes,
                if method == Method::POST {
                    "AllocNodesForKeyspaceGroupParams"
                } else if parts.len() == 1 {
                    "SetNodesForKeyspaceGroupParams"
                } else {
                    "SetPriorityForKeyspaceGroupParams"
                },
            )?;
            if method == Method::POST {
                let replica = v["replica"].as_i64().unwrap_or_default();
                m.refresh_registered_nodes().await.map_err(err)?;
                if replica < 2 || replica as usize > m.get_nodes_count() {
                    return Err(bad("invalid replica, should be in [2, nodes_num]"));
                }
                let g = m
                    .get_group(id)
                    .await
                    .map_err(err)?
                    .ok_or_else(|| bad("keyspace group does not exist"))?;
                if g.members.as_ref().map_or(0, Vec::len) >= replica as usize {
                    return Err(bad("existed replica is larger than the new replica"));
                }
                let existing: BTreeSet<_> = g
                    .members
                    .iter()
                    .flatten()
                    .filter_map(|n| m.registered_node(&n.address))
                    .collect();
                let nodes = m
                    .alloc_nodes(id, Some(existing), replica as usize, &c)
                    .await
                    .map_err(err)?;
                return Ok(json!(nodes));
            }
            let g = m
                .get_group(id)
                .await
                .map_err(err)?
                .ok_or_else(|| bad("keyspace group does not exist"))?;
            if parts.len() == 1 {
                let nodes: Vec<String> = serde_json::from_value(
                    v.get("nodes")
                        .filter(|v| !v.is_null())
                        .cloned()
                        .unwrap_or(json!([])),
                )
                .map_err(err)?;
                m.refresh_registered_nodes().await.map_err(err)?;
                if nodes.iter().any(|n| m.registered_node(n).is_none()) {
                    return Err(bad("node does not exist"));
                }
                m.set_nodes(id, &nodes).await.map_err(err)?;
            } else {
                let raw = parts[1].replace("\\/", "/").replace('+', "%2B");
                let node = crate::http_contract::query_unescape(&raw)
                    .map_err(|_| bad("invalid node address"))?;
                let node = String::from_utf8_lossy(&node);
                if !g
                    .members
                    .iter()
                    .flatten()
                    .any(|n| n.is_address_equivalent(&node))
                {
                    // Go aborts with 400 but continues through the
                    // domain call and appends its JSON result to the response.
                    let tail = match m
                        .set_priority(id, &node, v["priority"].as_i64().unwrap_or_default())
                        .await
                    {
                        Ok(()) => Value::Null,
                        Err(error) => json!(error.to_string()),
                    };
                    rendered_error = Some(format!(
                        "{}{}",
                        json!("tso node does not exist in the keyspace group"),
                        tail
                    ));
                    return Err((400, Value::Null));
                }
                m.set_priority(id, &node, v["priority"].as_i64().unwrap_or_default())
                    .await
                    .map_err(err)?;
            }
            return Ok(Value::Null);
        }
        if method == Method::GET && path.is_empty() {
            crate::accept_http_09::uint(
                q.get("page_token")
                    .map(String::as_str)
                    .filter(|s| !s.is_empty())
                    .unwrap_or("0"),
                32,
                "page",
            )?;
            let raw = q
                .get("limit")
                .map(String::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("0");
            raw.parse::<i64>().map_err(|_| {
                bad(&format!(
                    "strconv.ParseInt: parsing \"{raw}\": invalid syntax"
                ))
            })?;
        }
        let m = s.keyspace_group_manager().await.map_err(err)?;
        if let Some(groups) = create {
            m.create_groups(&groups).await.map_err(err)?;
            return Ok(Value::Null);
        }
        if method == Method::DELETE {
            return m.delete_group(id).await.map(|g| json!(g)).map_err(|e| {
                let e = e.to_string();
                (if id == 0 { 400 } else { 500 }, json!(e))
            });
        }
        if path.is_empty() {
            let start = crate::accept_http_09::uint(
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
                bad(&format!(
                    "strconv.ParseInt: parsing \"{raw}\": invalid syntax"
                ))
            })?;
            let groups = m
                .get_groups(start, if raw == "0" { 0 } else { limit.wrapping_add(1) })
                .await
                .map_err(err)?;
            let state = q.get("state").map(|v| v.to_lowercase());
            return Ok(json!(
                groups
                    .into_iter()
                    .filter(|g| match state.as_deref() {
                        None => true,
                        Some("split") => g.is_splitting(),
                        Some("merge") => g.is_merging(),
                        _ => false,
                    })
                    .map(|g| group(Some(g), hide))
                    .collect::<Vec<_>>()
            ));
        }
        if q.get("fields").is_some_and(|v| v == "primary") {
            return Ok(
                json!({"id":id,"primary":primary(m.primary_record(id).await.map_err(err)?)?}),
            );
        }
        Ok(group(m.get_group(id).await.map_err(err)?, hide))
    };
    let (status, v) = match tokio::select! {biased;_=c.cancelled()=>Err(err("context canceled")),r=work=>r}
    {
        Ok(v) => (200, v),
        Err(e) => e,
    };
    let content = if status == 400
        && v.as_str()
            .is_some_and(|s| s.starts_with("[PD:gin:ErrBindJSON]"))
    {
        "text/plain; charset=utf-8"
    } else {
        "application/json; charset=utf-8"
    };
    (
        StatusCode::from_u16(status).unwrap(),
        [("content-type", content)],
        rendered_error.unwrap_or_else(|| serde_json::to_string(&v).unwrap()),
    )
        .into_response()
}

fn primary(bytes: Vec<u8>) -> Result<String, Error> {
    use prost::Message;
    let p = astersql_pd_proto::tsopb::Participant::decode(bytes.as_slice()).map_err(err)?;
    Ok(p.name
        .rsplit_once('-')
        .map(|(a, _)| a.to_owned())
        .unwrap_or(p.name))
}

fn binding(message: impl std::fmt::Display) -> Error {
    (
        400,
        json!(format!("[PD:gin:ErrBindJSON]{message}: {message}")),
    )
}
