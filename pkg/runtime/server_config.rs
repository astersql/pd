// Copyright 2026 AsterSQL.
//! Serialized PD configuration publication using the active leader transaction.
use super::*;
use astersql_pd_schedule::Coordinator;
use serde_json::Value;

pub(super) async fn update(
    key: String,
    value: Value,
    coordinator: &mut Coordinator<ProviderSnapshot<()>>,
    resources: &Arc<AsyncMutex<Option<Resources>>>,
    cancel: &CancellationToken,
) -> Result<(), String> {
    let mut guard = resources.lock().await;
    let r = guard.as_mut().ok_or("server not started")?;
    let mut next = r.persisted_config.clone();
    if key.split('.').next() == Some("cluster-version") {
        let text = value
            .as_str()
            .ok_or_else(|| format!("input value {} is illegal", go_value(&value)))?;
        let text = if text.is_empty() {
            "1.0.0"
        } else {
            text.strip_prefix('v').unwrap_or(text)
        };
        let parsed = serde_json::from_value(Value::String(text.into()));
        next.config.cluster_version = if text.is_empty() {
            Err(" is not in dotted-tri format".into())
        } else {
            parsed.map_err(|e| version_error(text, e.to_string()))
        }
        .map_err(|e: String| format!("[PD:semver:ErrSemverNewVersion]{e}: {e}"))?;
    } else {
        let field = key.rsplit('.').next().unwrap();
        let mut fields = serde_json::to_value(&next.config.pd_server).map_err(|e| e.to_string())?;
        if fields.get(field).is_none() {
            return Err(format!("config item {field} not found"));
        }
        validate_type(field, &value)?;
        if !value.is_null() {
            fields[field] = value;
        }
        let candidate: astersql_pd_server_config::PDServerConfig =
            serde_json::from_value(fields).map_err(|e| e.to_string())?;
        if candidate == next.config.pd_server {
            return Ok(());
        }
        candidate.validate().map_err(|e| e.to_string())?;
        if candidate.dashboard_address != next.config.pd_server.dashboard_address
            && !matches!(candidate.dashboard_address.as_str(), "auto" | "none")
        {
            use prost::Message;
            let prefix = format!("/pd/{}/rust/members/", r.paths.cluster_id());
            let entries = r
                .meta_store
                .read(astersql_pd_storage::Read::prefix(
                    prefix.as_bytes(),
                    astersql_pd_storage::Consistency::Linearizable,
                ))
                .await
                .map_err(|e| e.to_string())?;
            let mut found = false;
            for entry in entries.entries {
                let member = astersql_pd_proto::pdpb::Member::decode(entry.value.as_slice())
                    .map_err(|e| e.to_string())?;
                found |= member.client_urls.contains(&candidate.dashboard_address);
            }
            if !found {
                return Err(format!(
                    "dashboard address {} is not the client url of any member",
                    candidate.dashboard_address
                ));
            }
        }
        next.config.pd_server = candidate;
    }
    let document = serde_json::to_vec(&next.persistence_snapshot()).map_err(|e| e.to_string())?;
    let storage = StorageEndpoint::new(Arc::new(crate::member_role_worker::TermStore::new(
        r.meta_store.clone(),
        r.leader.clone(),
        cancel.clone(),
    )));
    storage
        .run_in_txn(async |txn| {
            txn.save(r.paths.config_path().as_bytes(), &document)
                .map_err(astersql_pd_storage_endpoint::EndpointError::from)?;
            Ok::<_, astersql_pd_storage_endpoint::EndpointError>(())
        })
        .await
        .map_err(|e| e.to_string())?;
    // No yield between durable success and publication to all active consumers.
    r.resolved_interval.store(
        next.config
            .pd_server
            .min_resolved_ts_persistence_interval
            .as_nanos()
            .max(0) as u64,
        std::sync::atomic::Ordering::Relaxed,
    );
    let mut state = r.state.lock().unwrap();
    state.config.flow_round_by_digit = next.config.pd_server.flow_round_by_digit;
    state.config.key_type = match next.config.pd_server.key_type.as_str() {
        "raw" => astersql_pd_core_constant::KeyType::RAW,
        "txn" => astersql_pd_core_constant::KeyType::TXN,
        _ => astersql_pd_core_constant::KeyType::TABLE,
    };
    r.persisted_config = next;
    coordinator.update_config(Arc::new(state.config.clone()));
    coordinator
        .get_schedulers_controller_mut()
        .set_configuration(serde_json::from_slice(&document).expect("serialized configuration"));
    Ok(())
}
fn go_value(v: &Value) -> String {
    match v {
        Value::Null => "<nil>".into(),
        Value::String(s) => s.clone(),
        _ => v.to_string(),
    }
}
fn validate_type(field: &str, v: &Value) -> Result<(), String> {
    if matches!(
        field,
        "use-region-storage" | "enable-gogc-tuner" | "block-safe-point-v1"
    ) {
        if v.is_null() || v == "true" || v == "false" {
            return Ok(());
        }
        let detail = v
            .as_str()
            .map(|v| format!("{v:?}"))
            .unwrap_or_else(|| "unquoted value".into());
        return Err(format!(
            "json: invalid use of ,string struct tag, trying to unmarshal {detail} into bool"
        ));
    }
    if matches!(
        field,
        "runtime-services" | "max-gap-reset-ts" | "min-resolved-ts-persistence-interval"
    ) {
        return if v.is_string() {
            Ok(())
        } else {
            Err("invalid syntax".into())
        };
    }
    if v.is_null() {
        return Ok(());
    }
    let typ = match field {
        "flow-round-by-digit"
        | "gc-keyspace-index-load-concurrency"
        | "gc-state-cache-warmup-concurrency" => "int",
        "server-memory-limit" | "server-memory-limit-gc-trigger" | "gc-tuner-threshold" => {
            "float64"
        }
        _ => "string",
    };
    if match typ {
        "int" => v.as_i64().is_some(),
        "float64" => v.is_number(),
        _ => v.is_string(),
    } {
        return Ok(());
    }
    let kind = match v {
        Value::Bool(_) => "bool".into(),
        Value::Number(_) if typ == "int" => format!("number {v}"),
        Value::Number(_) => "number".into(),
        Value::Array(_) => "array".into(),
        Value::Object(_) => "object".into(),
        _ => "string".into(),
    };
    Err(format!(
        "json: cannot unmarshal {kind} into Go struct field PDServerConfig.{field} of type {typ}"
    ))
}

// Preserve the underlying Go semver diagnostics while using the canonical type.
fn version_error(text: &str, fallback: String) -> String {
    let number = text.split('+').next().unwrap().split('-').next().unwrap();
    let parts: Vec<_> = number.splitn(3, '.').collect();
    if parts.len() != 3 {
        return format!("{number} is not in dotted-tri format");
    }
    if fallback.starts_with("failed to validate") {
        return fallback;
    }
    for part in parts {
        if let Err(e) = part.parse::<i64>() {
            let reason = match e.kind() {
                std::num::IntErrorKind::PosOverflow | std::num::IntErrorKind::NegOverflow => {
                    "value out of range"
                }
                _ => "invalid syntax",
            };
            return format!("strconv.ParseInt: parsing {part:?}: {reason}");
        }
    }
    fallback
}
