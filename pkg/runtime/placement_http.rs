// Copyright 2026 AsterSQL.
//! Placement operations serialized by the existing scheduling task.
use super::*;
use astersql_pd_schedule::Coordinator;
use astersql_pd_schedule_placement::{Rule, RuleManagerError};
use astersql_pd_server::http_rule::Request;
use serde_json::{Value, json};
fn response(code: u16, message: impl ToString) -> (u16, Value) {
    (code, json!(message.to_string()))
}
fn rule_error(e: RuleManagerError, delete: bool) -> (u16, Value) {
    let code = if !delete && matches!(e, RuleManagerError::Content(_) | RuleManagerError::Hex(_)) {
        400
    } else {
        500
    };
    response(code, e)
}
pub(super) async fn handle(
    request: Request,
    coordinator: &mut Coordinator<ProviderSnapshot<()>>,
    resources: &Arc<AsyncMutex<Option<Resources>>>,
    cancel: &CancellationToken,
) -> Result<(u16, Value), String> {
    let mut guard = resources.lock().await;
    let r = guard.as_mut().ok_or("server not started")?;
    r.leader.validate().await.map_err(|e| e.to_string())?;
    if r.storage
        .load_meta(&r.paths)
        .await
        .map_err(|e| e.to_string())?
        .is_none()
    {
        return Ok(response(
            500,
            "[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first",
        ));
    }
    if !r
        .state
        .lock()
        .unwrap()
        .config
        .replication
        .enable_placement_rules
    {
        return Ok(response(
            412,
            "[PD:placement:ErrPlacementDisabled]placement rules feature is disabled",
        ));
    }
    let path = astersql_pd_server::http_contract::query_unescape(&request.path.replace('+', "%2B"))
        .map_err(|_| "invalid path escape".to_owned())?;
    let path = String::from_utf8_lossy(&path);
    let parts = path
        .trim_start_matches("/pd/api/v1/config/")
        .split('/')
        .collect::<Vec<_>>();
    let delete = request.method == axum::http::Method::DELETE;
    let write = delete || request.method == axum::http::Method::POST;
    if !write {
        let state = r.state.lock().unwrap();
        let rules = &state.rules;
        let value = match parts.as_slice() {
            ["rule", group, id] => {
                return Ok(match rules.get_rule(group, id) {
                    Some(v) => (200, serde_json::to_value(v).unwrap()),
                    None => response(404, "[PD:placement:ErrRuleNotFound]rule not found"),
                });
            }
            ["rules", "group", group] => {
                let values = rules.get_rules_by_group(group);
                if values.is_empty() {
                    Value::Null
                } else {
                    serde_json::to_value(values).unwrap()
                }
            }
            ["rules", "key", key] => {
                let mut decoded = Vec::new();
                for pair in key.as_bytes().chunks(2) {
                    let mut n = 0;
                    for c in pair {
                        let Some(d) = (*c as char).to_digit(16) else {
                            return Ok(response(
                                400,
                                format!(
                                    "[PD:placement:ErrKeyFormat]key should be in hex format, encoding/hex: invalid byte: U+{c:04X} '{}'",
                                    *c as char
                                ),
                            ));
                        };
                        n = n * 16 + d as u8;
                    }
                    if pair.len() != 2 {
                        return Ok(response(
                            400,
                            "[PD:placement:ErrKeyFormat]key should be in hex format, encoding/hex: odd length hex string",
                        ));
                    }
                    decoded.push(n);
                }
                serde_json::to_value(rules.get_rules_by_key(&decoded)).unwrap()
            }
            ["rules", "region", id] => {
                let id = match id.parse::<u64>() {
                    Ok(v) => v,
                    Err(_) => {
                        return Ok(response(
                            400,
                            "[PD:region:ErrRegionInvalidID]invalid region id",
                        ));
                    }
                };
                let Some(region) = state.cluster.regions.get_region(id) else {
                    return Ok(response(
                        404,
                        format!("[PD:region:ErrRegionNotFound]region {id} not found"),
                    ));
                };
                let values = rules.get_rules_for_apply_region(&region);
                if values.is_empty() {
                    Value::Null
                } else {
                    serde_json::to_value(values.iter().map(|r| r.as_ref()).collect::<Vec<_>>())
                        .unwrap()
                }
            }
            _ => return Ok(response(404, "not found")),
        };
        return Ok((200, value));
    }
    let mut incoming = if delete {
        Rule {
            group_id: parts[1].into(),
            id: parts[2].into(),
            ..Default::default()
        }
    } else {
        let _value = match astersql_pd_server::http_rule::parse(&request.body) {
            Ok(v) => v,
            Err(v) => return Ok((400, v)),
        };
        match Rule::from_json(&request.body) {
            Ok(v) => v,
            Err(e) => return Ok(response(400, e)),
        }
    };
    // Go synchronizes replication before validating the default rule, including on failure.
    if !delete && incoming.group_id == "pd" && incoming.id == "default" {
        drop(guard);
        if let Err(e) = super::replication_config::update(
            false,
            json!({"max-replicas":incoming.count as u64}),
            coordinator,
            resources,
            cancel,
        )
        .await
        {
            return Ok(response(400, e));
        }
        guard = resources.lock().await;
    }
    let r = guard.as_mut().ok_or("server not started")?;
    let (update, old) = {
        let mut state = r.state.lock().unwrap();
        state
            .rules
            .set_key_type(&r.persisted_config.config.pd_server.key_type);
        let old = state.rules.get_rule(&incoming.group_id, &incoming.id);
        let update = if delete {
            state
                .rules
                .prepare_delete(&incoming.group_id, &incoming.id, now().0 as u64)
        } else {
            let stores = state
                .cluster
                .stores
                .get_stores()
                .iter()
                .map(|s| s.as_ref().clone())
                .collect::<Vec<_>>();
            state.rules.prepare_rule(
                std::mem::take(&mut incoming),
                now().0 as u64,
                &state.config,
                &stores,
            )
        };
        let update = match update {
            Ok(v) => v,
            Err(e) => return Ok(rule_error(e, delete)),
        };
        (update, old)
    };
    if delete && old.is_none() {
        return Ok(response(200, "Delete rule successfully."));
    }
    let storage = StorageEndpoint::new(Arc::new(crate::member_role_worker::TermStore::new(
        r.meta_store.clone(),
        r.leader.clone(),
        cancel.clone(),
    )));
    let result = storage
        .run_in_txn(async |txn| {
            if !update.changed() {
                return Ok::<_, astersql_pd_storage_endpoint::EndpointError>(());
            }
            if delete {
                storage.delete_rule(txn, &r.paths, &update.rule().store_key())?;
            } else {
                storage.save_rule(txn, &r.paths, &update.rule().store_key(), update.rule())?;
            }
            Ok::<_, astersql_pd_storage_endpoint::EndpointError>(())
        })
        .await;
    if let Err(e) = result {
        return Ok(response(500, e));
    }
    let mut ranges = vec![];
    if !delete {
        ranges.push((
            update.rule().start_key.clone(),
            update.rule().end_key.clone(),
        ));
    }
    if let Some(old) = old {
        ranges.push((old.start_key, old.end_key));
    }
    r.state.lock().unwrap().rules.publish_rule(update);
    for (start, end) in ranges {
        r.suspect_ranges.add(&start, &end);
    }
    Ok(response(
        200,
        if delete {
            "Delete rule successfully."
        } else {
            "Update rule successfully."
        },
    ))
}

/// Process one canonical suspect range on the host's existing scheduling task.
pub(super) async fn check_ranges(
    coordinator: &Coordinator<ProviderSnapshot<()>>,
    resources: &Arc<AsyncMutex<Option<Resources>>>,
) -> Result<(), String> {
    let guard = resources.lock().await;
    let r = guard.as_ref().ok_or("server not started")?;
    r.leader.validate().await.map_err(|e| e.to_string())?;
    let state = r.state.lock().unwrap();
    r.suspect_ranges.check_once(
        &state.cluster,
        &coordinator.get_checker_controller().lock().unwrap(),
    );
    Ok(())
}
