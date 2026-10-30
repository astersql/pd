// Copyright 2026 AsterSQL.
//! HTTP mapping over canonical affinity state; no transport types enter the domain.
use super::*;
use astersql_pd_schedule_affinity::{
    group::{GroupState, validate_group_id},
    manager::Manager,
    parse::{GroupStateProvider, collect_group_states},
};
use astersql_pd_server::{http_affinity_read::Request, http_contract::query_unescape};
use serde_json::{Value, json};
struct States<'a>(&'a Manager, u64);
impl GroupStateProvider for States<'_> {
    fn all_group_states(&self) -> Vec<GroupState> {
        self.0
            .groups
            .values()
            .map(|g| g.info.state(self.1))
            .collect()
    }
    fn group_state(&self, id: &str) -> Option<GroupState> {
        self.0.groups.get(id).map(|g| g.info.state(self.1))
    }
}
pub(super) async fn handle(
    request: Request,
    manager: &mut Manager,
    labels: &mut Arc<astersql_pd_schedule_labeler::RegionLabeler>,
    resources: &Arc<AsyncMutex<Option<Resources>>>,
    now: u64,
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
        return Ok((
            500,
            json!(
                "[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first"
            ),
        ));
    }
    if request.write.is_some() {
        return super::affinity_write_http::handle(
            request,
            manager,
            Arc::get_mut(labels).ok_or("labeler is in use")?,
            r,
            now,
        )
        .await;
    }
    let states = States(manager, now);
    if let Some(raw) = request.path.strip_prefix("/pd/api/v2/affinity-groups/") {
        // Gin decodes the path once; plus is literal in path components.
        let id = query_unescape(&raw.replace('+', "%2B")).map_err(|_| "invalid URL escape")?;
        let id = String::from_utf8_lossy(&id);
        if let Err(e) = validate_group_id(&id) {
            return Ok((400, json!(e.to_string())));
        }
        return Ok(match states.group_state(&id) {
            Some(s) => (200, json!(s)),
            None => (
                404,
                json!(format!(
                    "[PD:affinity:ErrAffinityGroupNotFound]affinity group {id} not found"
                )),
            ),
        });
    }
    // net/url skips malformed pairs and unescaped semicolons, retains repeated ids.
    let ids = request
        .query
        .split('&')
        .filter(|p| !p.contains(';'))
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let (key, value) = (query_unescape(key).ok()?, query_unescape(value).ok()?);
            (key == b"ids").then(|| String::from_utf8_lossy(&value).into_owned())
        })
        .collect::<Vec<_>>();
    Ok(match collect_group_states(&states, &ids) {
        Ok(groups) => (200, json!({"affinity_groups":groups})),
        Err(e) => (400, json!(e.to_string())),
    })
}

/// Reuse the domain availability policy on the existing term task and clock.
/// The timer is owned by schedule_worker, including while its prepare gate waits.
pub(super) async fn check_availability(
    manager: &mut Manager,
    resources: &Arc<AsyncMutex<Option<Resources>>>,
) {
    let guard = resources.lock().await;
    let Some(r) = guard.as_ref() else { return };
    let state = r.state.lock().unwrap();
    let stores = state
        .cluster
        .stores
        .get_stores()
        .into_iter()
        .map(|s| s.as_ref().clone())
        .collect::<Vec<_>>();
    let (seconds, nanos) = now();
    manager.check_groups_availability(&stores, &state.config, nanos as i128, seconds as u64);
}
