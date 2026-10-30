// Copyright 2026 AsterSQL.
//! Bulk adapter for the canonical manager; never holds the state mutex across IO.
use super::*;
use astersql_pd_schedule_placement::{GroupBundle, RuleGroup, RuleOp};
use astersql_pd_server::http_rule_group::OperationFields;

pub(super) async fn handle(
    request: Request,
    coordinator: &mut Coordinator<ProviderSnapshot<()>>,
    resources: &Arc<AsyncMutex<Option<Resources>>>,
    cancel: &CancellationToken,
) -> Result<(u16, Value), String> {
    let (raw_path, query) = request.path.split_once('?').unwrap_or((&request.path, ""));
    let decoded = astersql_pd_server::http_contract::query_unescape(&raw_path.replace('+', "%2B"))
        .map_err(|e| format!("{e:?}"))?;
    let path = String::from_utf8_lossy(&decoded);
    let parts = path
        .trim_start_matches("/pd/api/v1/config/")
        .split('/')
        .collect::<Vec<_>>();
    let has = |name: &str| {
        query.split('&').any(|p| {
            astersql_pd_server::http_contract::query_unescape(p.split('=').next().unwrap())
                .is_ok_and(|v| v == name.as_bytes())
        })
    };
    let delete = request.method == axum::http::Method::DELETE;
    let mut guard = resources.lock().await;
    let r = guard.as_mut().ok_or("server not started")?;
    if request.method == axum::http::Method::GET {
        let state = r.state.lock().unwrap();
        let rules = &state.rules;
        let value = match parts.as_slice() {
            ["rules"] => serde_json::to_value(rules.get_all_rules()).unwrap(),
            ["rule_group", id] => {
                return Ok(match rules.get_rule_group(id) {
                    Some(v) => (200, serde_json::to_value(v).unwrap()),
                    None => (404, Value::Null),
                });
            }
            ["rule_groups"] => serde_json::to_value(rules.get_rule_groups()).unwrap(),
            ["placement-rule"] => serde_json::to_value(rules.get_all_group_bundles()).unwrap(),
            ["placement-rule", id] => serde_json::to_value(rules.get_group_bundle(id)).unwrap(),
            ["rules", "region", id, "detail"] => {
                let Ok(id) = id.parse::<u64>() else {
                    return Ok(response(
                        400,
                        "[PD:region:ErrRegionInvalidID]invalid region id",
                    ));
                };
                let Some(region) = state.cluster.regions.get_region(id) else {
                    return Ok(response(
                        404,
                        format!("[PD:region:ErrRegionNotFound]region {id} not found"),
                    ));
                };
                let stores = state.cluster.stores.get_stores();
                let fit = rules.fit_region_without_cache(&stores, &region, &state.config);
                let peer = |p: &astersql_pd_core::Peer| {
                    let mut v = json!({"id":p.id,"store_id":p.store_id});
                    let role = match p.role {
                        astersql_pd_core::PeerRole::Voter => 0,
                        astersql_pd_core::PeerRole::Learner => 1,
                        astersql_pd_core::PeerRole::IncomingVoter => 2,
                        astersql_pd_core::PeerRole::DemotingVoter => 3,
                        astersql_pd_core::PeerRole::Unknown(v) => v,
                    };
                    if role != 0 {
                        v["role"] = json!(role);
                    }
                    if p.is_witness {
                        v["is_witness"] = json!(true);
                    }
                    v
                };
                json!({"rule-fits":fit.rule_fits.iter().map(|f| json!({
                    "rule":f.rule.as_ref(), "peers":if f.peers.is_empty(){Value::Null}else{json!(f.peers.iter().map(&peer).collect::<Vec<_>>())},
                    "peers-different-role":if f.peers_with_different_role.is_empty(){Value::Null}else{json!(f.peers_with_different_role.iter().map(&peer).collect::<Vec<_>>())},
                    "isolation-score":if f.isolation_score.fract()==0.0 && f.isolation_score>=0.0 && f.isolation_score<(u64::MAX as f64) {json!(f.isolation_score as u64)}else{json!(f.isolation_score)}, "witness-score":f.witness_score
                })).collect::<Vec<_>>(), "orphan-peers":if fit.orphan_peers_is_nil(){Value::Null}else{json!(fit.orphan_peers.iter().map(&peer).collect::<Vec<_>>())}})
            }
            _ => return Ok(response(404, "not found")),
        };
        return Ok((200, value));
    }
    let value = if delete {
        Value::Null
    } else {
        let schema = match parts.as_slice() {
            ["rules"] => "[]*placement.Rule",
            ["rules", "batch"] => "[]placement.RuleOp",
            ["rule_group"] => "placement.RuleGroup",
            ["placement-rule"] => "[]placement.GroupBundle",
            _ => "placement.GroupBundle",
        };
        match astersql_pd_server::http_rule_group::parse(&request.body, schema) {
            Ok(v) => v,
            Err(v) => return Ok((400, v)),
        }
    };
    // SetRules synchronizes default replication first, including if a later rule is invalid.
    if parts.as_slice() == ["rules"] {
        if let Ok(values) = serde_json::from_slice::<Vec<Rule>>(&request.body) {
            for rule in values {
                if rule.group_id == "pd" && rule.id == "default" {
                    drop(guard);
                    let result = super::super::replication_config::update(
                        false,
                        json!({"max-replicas":rule.count as u64}),
                        coordinator,
                        resources,
                        cancel,
                    )
                    .await;
                    guard = resources.lock().await;
                    if let Err(e) = result {
                        return Ok(response(400, e));
                    }
                }
            }
        }
    }
    let r = guard.as_mut().ok_or("server not started")?;
    let (update, ranges, message) = {
        let mut state = r.state.lock().unwrap();
        state
            .rules
            .set_key_type(&r.persisted_config.config.pd_server.key_type);
        let rules = &state.rules;
        let stores = state
            .cluster
            .stores
            .get_stores()
            .iter()
            .map(|s| s.as_ref().clone())
            .collect::<Vec<_>>();
        let mut ranges = vec![];
        let mut patch = rules.begin_patch();
        macro_rules! decode {
            ($ty:ty) => {
                match serde_json::from_slice::<$ty>(&request.body) {
                    Ok(v) => v,
                    Err(e) => return Ok(response(400, e)),
                }
            };
        }
        let (patch, message) = match parts.as_slice() {
            ["rules"] => {
                let values: Vec<Rule> = if value.is_null() {
                    vec![]
                } else {
                    decode!(Vec<Rule>)
                };
                (
                    rules.prepare_set_rules(values, &state.config, &stores),
                    "Update rules successfully.",
                )
            }
            ["rules", "batch"] => {
                let values: Vec<Rule> = if value.is_null() {
                    vec![]
                } else {
                    decode!(Vec<Rule>)
                };
                let fields: Vec<OperationFields> = if value.is_null() {
                    vec![]
                } else {
                    decode!(Vec<OperationFields>)
                };
                let ops = values
                    .into_iter()
                    .zip(fields)
                    .map(|(rule, fields)| RuleOp {
                        rule,
                        action: fields.action,
                        delete_by_id_prefix: fields.prefix,
                    })
                    .collect();
                (
                    rules.prepare_batch(ops, &state.config, &stores),
                    "Batch operations successfully.",
                )
            }
            ["rule_group"] => {
                let group = decode!(RuleGroup);
                ranges.extend(
                    rules
                        .get_rules_by_group(&group.id)
                        .iter()
                        .map(|r| (r.start_key.clone(), r.end_key.clone())),
                );
                patch.set_group(group);
                (Ok(patch), "Update rule group successfully.")
            }
            ["rule_group", id] if delete => {
                ranges.extend(
                    rules
                        .get_rules_by_group(id)
                        .iter()
                        .map(|r| (r.start_key.clone(), r.end_key.clone())),
                );
                patch.delete_group(id);
                (Ok(patch), "Delete rule group successfully.")
            }
            ["placement-rule"] => {
                let bundles = if value.is_null() {
                    vec![]
                } else {
                    decode!(Vec<GroupBundle>)
                };
                (
                    rules.prepare_set_all_group_bundles(
                        bundles,
                        !has("partial"),
                        &state.config,
                        &stores,
                    ),
                    "Update rules and groups successfully.",
                )
            }
            ["placement-rule", id] if delete => {
                // The Go handler applies PathUnescape once more for bundle deletion.
                let id = match astersql_pd_server::http_contract::query_unescape(
                    &id.replace('+', "%2B"),
                ) {
                    Ok(v) => String::from_utf8_lossy(&v).into_owned(),
                    Err(_) => {
                        let pos = id.find('%').unwrap();
                        let escape = &id.as_bytes()[pos..(pos + 3).min(id.len())];
                        return Ok(response(
                            400,
                            format!(
                                "invalid URL escape {}",
                                json!(String::from_utf8_lossy(escape))
                            ),
                        ));
                    }
                };
                (
                    if has("regexp") {
                        let pattern = match astersql_pd_server::http_rule_group::group_regexp(&id) {
                            Ok(v) => v,
                            Err(e) => return Ok(response(500, e)),
                        };
                        Ok(rules.prepare_delete_matching_groups(|id| pattern.is_match(id)))
                    } else {
                        rules.prepare_delete_group_bundle(&id, false)
                    },
                    "Delete group and rules successfully.",
                )
            }
            ["placement-rule", id] => {
                let mut bundle = decode!(GroupBundle);
                if bundle.id.is_empty() {
                    bundle.id = (*id).into();
                }
                if bundle.id != *id {
                    return Ok(response(
                        400,
                        format!("group id {} does not match request URI {id}", bundle.id),
                    ));
                }
                (
                    rules.prepare_set_all_group_bundles(
                        vec![bundle],
                        false,
                        &state.config,
                        &stores,
                    ),
                    "Update group and rules successfully.",
                )
            }
            _ => return Ok(response(404, "not found")),
        };
        let patch = match patch.and_then(|p| rules.prepare_patch(p, now().0 as u64)) {
            Ok(v) => v,
            Err(e) => {
                return Ok(rule_error(
                    e,
                    delete || parts.first() == Some(&"rule_group"),
                ));
            }
        };
        (patch, ranges, message)
    };
    let storage = StorageEndpoint::new(Arc::new(crate::member_role_worker::TermStore::new(
        r.meta_store.clone(),
        r.leader.clone(),
        cancel.clone(),
    )));
    if let Err(e) = update.persist(&storage, &r.paths).await {
        return Ok(response(500, e));
    }
    r.state.lock().unwrap().rules.publish_patch(update);
    for (start, end) in ranges {
        r.suspect_ranges.add(&start, &end);
    }
    Ok(response(200, message))
}
