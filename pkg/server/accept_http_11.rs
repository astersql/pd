// Copyright 2026 AsterSQL.
//! Split, merge and removal HTTP decoding; transactions belong to keyspace managers.
use crate::{
    PdServices,
    accept_http_10::{Error, bind},
};
use axum::{
    body::{Body, to_bytes},
    http::Method,
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
fn error(e: impl std::fmt::Display) -> Error {
    (500, json!(e.to_string()))
}
fn bad(s: &str) -> Error {
    (400, json!(s))
}
fn ids(v: &Value, key: &str) -> Vec<u32> {
    v[key]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| v.as_u64().unwrap_or_default() as u32)
        .collect()
}
pub(super) async fn operate(
    s: &dyn PdServices,
    c: &CancellationToken,
    id: u32,
    operation: &str,
    method: &Method,
    body: Body,
) -> Result<Value, Error> {
    if method == Method::DELETE && operation != "keyspaces" {
        let m = s.keyspace_group_manager().await.map_err(error)?;
        if operation == "split" {
            m.finish_split(id).await.map_err(error)?;
        } else {
            m.finish_merge(id).await.map_err(error)?;
        }
        return Ok(Value::Null);
    }
    let bytes = to_bytes(body, usize::MAX)
        .await
        .map_err(|_| bad("unexpected EOF"))?;
    let v = bind(
        &bytes,
        match operation {
            "split" => "SplitKeyspaceGroupByIDParams",
            "merge" => "MergeKeyspaceGroupsParams",
            _ => "RemoveKeyspacesFromGroupParams",
        },
    )?;
    match operation {
        "split" => {
            let target = v["new-id"].as_u64().unwrap_or_default() as u32;
            let keys = ids(&v, "keyspaces");
            let start = v["start-keyspace-id"].as_u64().unwrap_or_default() as u32;
            let end = v["end-keyspace-id"].as_u64().unwrap_or_default() as u32;
            if target > 4096 {
                return Err(bad("invalid keyspace group id"));
            }
            if keys.is_empty() && start == 0 && end == 0 {
                return Err(bad("invalid empty keyspaces"));
            }
            if start > end {
                return Err(bad("invalid start/end keyspace id"));
            }
            let k = s.keyspace_manager().await.map_err(error)?;
            let m = s.keyspace_group_manager().await.map_err(error)?;
            k.patrol_group_assignment(start, end, c)
                .await
                .map_err(error)?;
            m.split_group(id, target, &keys, &[start, end])
                .await
                .map_err(error)?;
        }
        "merge" => {
            let keys = ids(&v, "merge-list");
            let all = v["merge-all-into-default"].as_bool().unwrap_or_default();
            if keys.is_empty() && !all {
                return Err(bad("invalid empty merge list"));
            }
            if !keys.is_empty() && all {
                return Err(bad("non-empty merge list when merge all into default"));
            }
            if keys.iter().any(|n| *n > 4096) {
                return Err(bad("invalid keyspace group id"));
            }
            let m = s.keyspace_group_manager().await.map_err(error)?;
            if all {
                m.merge_all_into_default(c).await.map_err(error)?;
            } else {
                m.merge_groups(id, &keys).await.map_err(error)?;
            }
        }
        _ => {
            let keys = ids(&v, "keyspaces");
            if keys.is_empty() {
                return Err(bad("keyspaces list cannot be empty"));
            }
            let k = s.keyspace_manager().await.map_err(error)?;
            let m = s.keyspace_group_manager().await.map_err(error)?;
            return m
                .remove_keyspaces_from_group(c, id, &k, None, &keys)
                .await
                .map(|g| json!(g))
                .map_err(|e| {
                    use astersql_pd_keyspace::GroupRemovalError;
                    (
                        match e {
                            GroupRemovalError::NotExists(_) => 404,
                            GroupRemovalError::Conflict => 409,
                            _ => 500,
                        },
                        json!(e.to_string()),
                    )
                });
        }
    }
    Ok(Value::Null)
}
