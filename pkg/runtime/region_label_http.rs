// Copyright 2026 AsterSQL.
//! Transport adaptation over the existing labeler, storage, and term command queue.
use super::*;
use astersql_pd_schedule_labeler::{LabelRule, LabelRulePatch, LabelerError, RegionLabeler};
use astersql_pd_server::http_region_label::Request;
use serde_json::{Value, json};
fn response(code: u16, message: impl ToString) -> (u16, Value) {
    (code, json!(message.to_string()))
}
fn error(e: LabelerError) -> (u16, Value) {
    let status = match &e {
        LabelerError::Rule(_) => 400,
        LabelerError::NotFound(_) => 404,
        _ => 500,
    };
    response(status, e)
}
pub(super) async fn handle(
    request: Request,
    labels: &mut RegionLabeler,
    resources: &Arc<AsyncMutex<Option<Resources>>>,
    now: i128,
) -> Result<(u16, Value), String> {
    let guard = resources.lock().await;
    let r = guard.as_ref().ok_or("server not started")?;
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
    let path = request.path.as_str();
    if path.starts_with("/pd/api/v1/region/id/") {
        let raw = path
            .trim_start_matches("/pd/api/v1/region/id/")
            .split('/')
            .next()
            .unwrap_or_default();
        let raw = astersql_pd_server::http_contract::query_unescape(&raw.replace('+', "%2B"))
            .map_err(|e| format!("{e:?}"))?;
        let raw = String::from_utf8_lossy(&raw);
        let id = match astersql_pd_server::http_contract::parse_uint64_field(Some(&raw), "id") {
            Ok(v) => v,
            Err(astersql_pd_server::http_contract::HttpError::Input(e)) => {
                return Ok(response(400, e));
            }
            Err(_) => return Err("invalid input".into()),
        };
        let region = r.state.lock().unwrap().cluster.regions.get_region(id);
        return Ok(match region {
            Some(region) => (200, json!(labels.get_region_labels(&region, now))),
            None => (404, Value::Null),
        });
    }
    let suffix = path.trim_start_matches("/pd/api/v1/config/region-label/");
    let parse = |schema| astersql_pd_server::http_rule_group::parse(&request.body, schema);
    if let Some(raw) = suffix.strip_prefix("rule/") {
        // escapeRouter uses the encoded path; PathUnescape preserves literal plus.
        let id = match astersql_pd_server::http_contract::query_unescape(&raw.replace('+', "%2B")) {
            Ok(v) => String::from_utf8_lossy(&v).into_owned(),
            Err(_) => return Ok(response(400, "invalid URL escape")),
        };
        if request.method == axum::http::Method::DELETE {
            return Ok(match labels.delete_label_rule(&id).await {
                Ok(()) => response(200, "Delete rule successfully."),
                Err(e) => error(e),
            });
        }
        return Ok((
            if labels.get_label_rule(&id, now).is_some() {
                200
            } else {
                404
            },
            json!(labels.get_label_rule(&id, now)),
        ));
    }
    if suffix == "rules/ids" {
        if let Err(e) = parse("[]string") {
            return Ok((400, e));
        }
        let ids: Option<Vec<Option<String>>> =
            serde_json::from_slice(&request.body).map_err(|e| e.to_string())?;
        let ids = ids
            .unwrap_or_default()
            .into_iter()
            .map(Option::unwrap_or_default)
            .collect::<Vec<_>>();
        return Ok((
            200,
            json!(labels.get_label_rules(&ids.iter().map(String::as_str).collect::<Vec<_>>(), now)),
        ));
    }
    if request.method == axum::http::Method::GET {
        return Ok((200, json!(labels.get_all_label_rules(now))));
    }
    if suffix == "rule" {
        if let Err(e) = parse("labeler.LabelRule") {
            return Ok((400, e));
        }
        let rule = LabelRule::from_json_without_check(&request.body).map_err(|e| e.to_string())?;
        return Ok(match labels.set_label_rule(rule, now).await {
            Ok(()) => response(200, "Update region label rule successfully."),
            Err(e) => error(e),
        });
    }
    if let Err(e) = parse("labeler.LabelRulePatch") {
        return Ok((400, e));
    }
    let patch: astersql_pd_server::http_region_label::Patch<LabelRule> =
        serde_json::from_slice(&request.body).map_err(|e| e.to_string())?;
    Ok(
        match labels
            .patch(
                LabelRulePatch {
                    set_rules: patch.sets,
                    delete_rules: patch.deletes,
                },
                now,
            )
            .await
        {
            Ok(()) => response(200, "Update region label rules successfully."),
            Err(e) => error(e),
        },
    )
}
