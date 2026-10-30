// Copyright 2026 AsterSQL.
//! v2 writes on the existing term's single affinity manager and labeler.
use super::*;
use astersql_pd_keyutil::KeyRange;
use astersql_pd_schedule_affinity::{
    group::validate_group_id, manager::Manager, ranges::GroupKeyRanges, txn::TransactionError,
};
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_server::{http_affinity_read::Request, http_contract::query_unescape};
use base64::{
    Engine,
    engine::{GeneralPurpose, GeneralPurposeConfig},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
type Failure = (u16, Value);
fn message(code: u16, text: impl ToString) -> Failure {
    (code, json!(text.to_string()))
}
fn content(text: impl AsRef<str>) -> Failure {
    message(
        400,
        format!(
            "[PD:affinity:ErrAffinityGroupContent]invalid affinity group content, {}",
            text.as_ref()
        ),
    )
}
fn empty(text: &str) -> Failure {
    message(400, format!("[PD:affinity:ErrEmptyRequest]{text}"))
}
fn missing(id: &str) -> Failure {
    message(
        404,
        format!("[PD:affinity:ErrAffinityGroupNotFound]affinity group {id} not found"),
    )
}
fn transaction(e: TransactionError) -> Failure {
    let text = e.to_string();
    let code = if text.contains("ErrAffinityGroupNotFound") {
        404
    } else if text.contains("ErrAffinityGroupExist") {
        409
    } else if [
        "ErrAffinityGroupContent",
        "ErrInvalidGroupID",
        "ErrAffinityGroupConflict",
        "ErrInvalidKeyFormat",
    ]
    .iter()
    .any(|s| text.contains(s))
    {
        400
    } else {
        500
    };
    message(code, text)
}
fn field<'a>(v: &'a Value, name: &str) -> &'a Value {
    v.get(name)
        .or_else(|| {
            v.as_object()?
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| v)
        })
        .unwrap_or(&Value::Null)
}
fn bind(text: impl ToString) -> Failure {
    let text = text.to_string();
    message(400, format!("[PD:gin:ErrBindJSON]{text}: {text}"))
}

fn typed(v: &Value, typ: &str, structure: &str, path: &str) -> Failure {
    let kind = match v {
        Value::String(_) => "string".into(),
        Value::Number(n) => {
            if typ == "uint64" {
                format!("number {n}")
            } else {
                "number".into()
            }
        }
        Value::Bool(_) => "bool".into(),
        Value::Array(_) => "array".into(),
        _ => "object".into(),
    };
    bind(if path.is_empty() {
        format!("json: cannot unmarshal {kind} into Go value of type {typ}")
    } else {
        format!(
            "json: cannot unmarshal {kind} into Go struct field {structure}.{path} of type {typ}"
        )
    })
}
fn object(v: &Value, typ: &str) -> Result<(), Failure> {
    if v.is_null() || v.is_object() {
        Ok(())
    } else {
        Err(typed(v, typ, "", ""))
    }
}
fn string(v: &Value, structure: &str, path: &str) -> Result<String, Failure> {
    if v.is_null() {
        Ok(String::new())
    } else {
        v.as_str()
            .map(str::to_owned)
            .ok_or_else(|| typed(v, "string", structure, path))
    }
}
fn array<'a>(v: &'a Value, typ: &str, structure: &str, path: &str) -> Result<&'a [Value], Failure> {
    if v.is_null() {
        Ok(&[])
    } else {
        v.as_array()
            .map(Vec::as_slice)
            .ok_or_else(|| typed(v, typ, structure, path))
    }
}
fn boolean(v: &Value, structure: &str, path: &str) -> Result<bool, Failure> {
    if v.is_null() {
        Ok(false)
    } else {
        v.as_bool().ok_or_else(|| typed(v, "bool", structure, path))
    }
}
fn uint(v: &Value, structure: &str, path: &str) -> Result<u64, Failure> {
    if v.is_null() {
        Ok(0)
    } else {
        v.as_u64()
            .ok_or_else(|| typed(v, "uint64", structure, path))
    }
}
fn bytes(v: &Value, path: &str) -> Result<Vec<u8>, Failure> {
    if v.is_null() {
        return Ok(vec![]);
    }
    if let Some(s) = v.as_str() {
        let s = s.replace(['\r', '\n'], "");
        let engine = GeneralPurpose::new(
            &base64::alphabet::STANDARD,
            GeneralPurposeConfig::new().with_decode_allow_trailing_bits(true),
        );
        return engine.decode(&s).map_err(|e| {
            let index = match e {
                base64::DecodeError::InvalidByte(i, _)
                | base64::DecodeError::InvalidLastSymbol(i, _) => i,
                base64::DecodeError::InvalidLength(n) => n / 4 * 4,
                _ => s.len().saturating_sub(1),
            };
            bind(format!("illegal base64 data at input byte {index}"))
        });
    }
    if let Some(a) = v.as_array() {
        return a
            .iter()
            .map(|v| {
                if v.is_null() {
                    Ok(0)
                } else {
                    v.as_u64()
                        .filter(|n| *n <= 255)
                        .map(|n| n as u8)
                        .ok_or_else(|| typed(v, "uint8", "AffinityKeyRange", path))
                }
            })
            .collect();
    }
    Err(typed(v, "[]uint8", "AffinityKeyRange", path))
}
fn ranges(v: &Value, structure: &str, path: &str) -> Result<Vec<KeyRange>, Failure> {
    let mut out = vec![];
    for v in array(v, "[]handlers.AffinityKeyRange", structure, path)? {
        if !v.is_null() && !v.is_object() {
            return Err(typed(v, "handlers.AffinityKeyRange", structure, path));
        }
        let start = bytes(field(v, "start_key"), &format!("{path}.start_key"))?;
        let end = bytes(field(v, "end_key"), &format!("{path}.end_key"))?;
        out.push(KeyRange {
            start_key: start,
            end_key: end,
        });
    }
    Ok(out)
}
fn valid_ranges(ranges: &[KeyRange]) -> Result<(), Failure> {
    for r in ranges {
        if r.start_key.is_empty() && r.end_key.is_empty() {
            continue;
        }
        if r.start_key.is_empty() || r.end_key.is_empty() {
            return Err(content(
                "key range must have both start_key and end_key, or both empty for entire key space",
            ));
        }
        if r.start_key >= r.end_key {
            return Err(content("start_key must be less than end_key"));
        }
    }
    Ok(())
}
fn valid_id(id: &str) -> Result<(), Failure> {
    validate_group_id(id).map_err(|e| message(400, e))
}
fn query(raw: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for pair in raw.split('&').filter(|p| !p.contains(';')) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        if let (Ok(k), Ok(v)) = (query_unescape(k), query_unescape(v)) {
            out.entry(String::from_utf8_lossy(&k).into_owned())
                .or_insert_with(|| String::from_utf8_lossy(&v).into_owned());
        }
    }
    out
}
fn bool_query(value: &str) -> Option<bool> {
    match value {
        "1" | "t" | "T" | "TRUE" | "true" | "True" => Some(true),
        "0" | "f" | "F" | "FALSE" | "false" | "False" => Some(false),
        _ => None,
    }
}
fn states(manager: &Manager, ids: impl IntoIterator<Item = String>, now: u64) -> Value {
    let groups = ids
        .into_iter()
        .filter_map(|id| {
            manager
                .groups
                .get(&id)
                .map(|g| (id, json!(g.info.state(now))))
        })
        .collect::<serde_json::Map<_, _>>();
    json!({"affinity_groups":groups})
}
pub(super) async fn handle(
    mut request: Request,
    manager: &mut Manager,
    labels: &mut RegionLabeler,
    r: &Resources,
    now: u64,
) -> Result<Failure, String> {
    let write = request.write.take().expect("write envelope");
    let result = execute(&request, &write.method, write.body, manager, labels, r, now).await;
    Ok(result.unwrap_or_else(|e| e))
}
async fn execute(
    request: &Request,
    method: &str,
    body: axum::body::Body,
    manager: &mut Manager,
    labels: &mut RegionLabeler,
    r: &Resources,
    now: u64,
) -> Result<Failure, Failure> {
    let options = query(&request.query);
    let id = request
        .path
        .strip_prefix("/pd/api/v2/affinity-groups/")
        .map(|s| {
            String::from_utf8_lossy(&query_unescape(&s.replace('+', "%2B")).unwrap_or_default())
                .into_owned()
        });
    if method == "DELETE" {
        let id = id.unwrap_or_default();
        valid_id(&id)?;
        let force = match options.get("force") {
            None => false,
            Some(s) if s.is_empty() => false,
            Some(s) => bool_query(s)
                .ok_or_else(|| bind(format!("strconv.ParseBool: parsing {s:?}: invalid syntax")))?,
        };
        if !manager.groups.contains_key(&id) {
            return if force {
                Ok(message(200, "Affinity group deleted successfully."))
            } else {
                Err(missing(&id))
            };
        }
        manager
            .delete_affinity_groups(&r.storage, &r.paths, labels, &[id], force)
            .await
            .map_err(transaction)?;
        return Ok(message(200, "Affinity group deleted successfully."));
    }
    let body = axum::body::to_bytes(body, usize::MAX)
        .await
        .map_err(|e| message(500, e))?;
    // Gin BindJSON decodes one value and accepts trailing values.
    let value = serde_json::Deserializer::from_slice(&body)
        .into_iter::<Value>()
        .next()
        .unwrap_or_else(|| Err(serde_json::from_slice::<Value>(b"").unwrap_err()));
    let value = value.map_err(|e| {
        if body.iter().all(u8::is_ascii_whitespace) {
            return bind("EOF");
        }
        if e.is_eof() {
            return bind("unexpected EOF");
        }
        let err = astersql_pd_server::http_rule::parse(&body)
            .err()
            .unwrap_or(Value::Null);
        bind(
            err.get("msg")
                .and_then(Value::as_str)
                .unwrap_or("invalid JSON input"),
        )
    })?;
    match method {
        "POST" if options.contains_key("delete") => {
            object(&value, "handlers.BatchDeleteAffinityGroupsRequest")?;
            let ids = array(
                field(&value, "ids"),
                "[]string",
                "BatchDeleteAffinityGroupsRequest",
                "ids",
            )?
            .iter()
            .map(|v| string(v, "BatchDeleteAffinityGroupsRequest", "ids"))
            .collect::<Result<Vec<_>, _>>()?;
            let force = boolean(
                field(&value, "force"),
                "BatchDeleteAffinityGroupsRequest",
                "force",
            )?;
            if ids.is_empty() {
                return Err(empty("no group ids provided"));
            }
            for id in &ids {
                valid_id(id)?
            }
            manager
                .delete_affinity_groups(&r.storage, &r.paths, labels, &ids, force)
                .await
                .map_err(transaction)?;
            Ok((200, json!({"deleted":ids})))
        }
        "POST" => {
            object(&value, "handlers.CreateAffinityGroupsRequest")?;
            let input = field(&value, "affinity_groups");
            if !input.is_null() && !input.is_object() {
                return Err(typed(
                    input,
                    "map[string]handlers.CreateAffinityGroupInput",
                    "CreateAffinityGroupsRequest",
                    "affinity_groups",
                ));
            }
            let mut parsed = vec![];
            if let Some(groups) = input.as_object() {
                for (id, v) in groups {
                    if !v.is_null() && !v.is_object() {
                        return Err(typed(
                            v,
                            "handlers.CreateAffinityGroupInput",
                            "CreateAffinityGroupsRequest",
                            "affinity_groups",
                        ));
                    }
                    parsed.push(GroupKeyRanges {
                        group_id: id.clone(),
                        key_ranges: ranges(
                            field(v, "ranges"),
                            "CreateAffinityGroupInput",
                            "affinity_groups.ranges",
                        )?,
                    });
                }
            }
            if parsed.is_empty() {
                return Err(content("no affinity groups provided"));
            }
            let skip=match options.get("skip_exist_check"){None=>false,Some(s)=>bool_query(s).ok_or_else(||message(400,format!("[PD:common:ErrInvalidArgument]invalid argument for skip_exist_check: strconv.ParseBool: parsing {s:?}: invalid syntax")))?};
            let ids = parsed
                .iter()
                .map(|g| g.group_id.clone())
                .collect::<Vec<_>>();
            let mut changes = vec![];
            for g in parsed {
                valid_id(&g.group_id)?;
                if manager.groups.contains_key(&g.group_id) {
                    if skip {
                        continue;
                    } else {
                        return Err(message(
                            409,
                            format!(
                                "[PD:affinity:ErrAffinityGroupExist]affinity group {} already exists",
                                g.group_id
                            ),
                        ));
                    }
                }
                if g.key_ranges.is_empty() {
                    return Err(content(format!(
                        "no key ranges provided for group {}",
                        g.group_id
                    )));
                }
                valid_ranges(&g.key_ranges)?;
                changes.push(g);
            }
            manager
                .create_affinity_groups(&r.storage, &r.paths, labels, &changes, now)
                .await
                .map_err(transaction)?;
            Ok((200, states(manager, ids, now)))
        }
        "PATCH" => {
            object(&value, "handlers.BatchModifyAffinityGroupsRequest")?;
            let mut parsed = vec![];
            for name in ["add", "remove"] {
                let mut ops = vec![];
                for op in array(
                    field(&value, name),
                    "[]handlers.GroupRangesModification",
                    "BatchModifyAffinityGroupsRequest",
                    name,
                )? {
                    if !op.is_null() && !op.is_object() {
                        return Err(typed(
                            op,
                            "handlers.GroupRangesModification",
                            "BatchModifyAffinityGroupsRequest",
                            name,
                        ));
                    }
                    ops.push(GroupKeyRanges {
                        group_id: string(
                            field(op, "id"),
                            "GroupRangesModification",
                            &format!("{name}.id"),
                        )?,
                        key_ranges: ranges(
                            field(op, "ranges"),
                            "GroupRangesModification",
                            &format!("{name}.ranges"),
                        )?,
                    });
                }
                parsed.push(ops);
            }
            let removes = parsed.pop().unwrap();
            let adds = parsed.pop().unwrap();
            if adds.is_empty() && removes.is_empty() {
                return Err(empty("no add or remove operations provided"));
            }
            for op in &removes {
                if adds.iter().any(|a| a.group_id == op.group_id) {
                    return Err(message(
                        400,
                        format!(
                            "[PD:affinity:ErrAffinityGroupConflict]affinity group {} cannot appear in both add and remove operations in the same request",
                            op.group_id
                        ),
                    ));
                }
            }
            let mut affected = BTreeSet::new();
            let mut merged = vec![];
            for ops in [adds, removes] {
                let mut groups = BTreeMap::<String, Vec<KeyRange>>::new();
                for op in ops {
                    valid_id(&op.group_id)?;
                    if !manager.groups.contains_key(&op.group_id) {
                        return Err(missing(&op.group_id));
                    }
                    if op.key_ranges.is_empty() {
                        return Err(content("no key ranges provided"));
                    }
                    valid_ranges(&op.key_ranges)?;
                    affected.insert(op.group_id.clone());
                    groups.entry(op.group_id).or_default().extend(op.key_ranges);
                }
                merged.push(
                    groups
                        .into_iter()
                        .map(|(group_id, key_ranges)| GroupKeyRanges {
                            group_id,
                            key_ranges,
                        })
                        .collect::<Vec<_>>(),
                );
            }
            manager
                .update_affinity_group_key_ranges(
                    &r.storage, &r.paths, labels, &merged[0], &merged[1], now,
                )
                .await
                .map_err(transaction)?;
            Ok((200, states(manager, affected, now)))
        }
        "PUT" => {
            object(&value, "handlers.UpdateAffinityGroupPeersRequest")?;
            let leader = uint(
                field(&value, "leader_store_id"),
                "UpdateAffinityGroupPeersRequest",
                "leader_store_id",
            )?;
            let voters = array(
                field(&value, "voter_store_ids"),
                "[]uint64",
                "UpdateAffinityGroupPeersRequest",
                "voter_store_ids",
            )?
            .iter()
            .map(|v| uint(v, "UpdateAffinityGroupPeersRequest", "voter_store_ids"))
            .collect::<Result<Vec<_>, _>>()?;
            let id = id.unwrap_or_default();
            valid_id(&id)?;
            if leader == 0 || voters.is_empty() {
                return Err(content("leader_store_id and voter_store_ids are required"));
            }
            let stores = r
                .state
                .lock()
                .unwrap()
                .cluster
                .stores
                .get_stores()
                .into_iter()
                .map(|s| s.as_ref().clone())
                .collect::<Vec<_>>();
            let result = manager
                .update_affinity_group_peers(
                    &r.storage, &r.paths, &stores, &id, leader, &voters, now,
                )
                .await
                .map_err(transaction)?;
            Ok((200, json!(result.state)))
        }
        _ => Ok(message(405, "method not allowed")),
    }
}
