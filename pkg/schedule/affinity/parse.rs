// Copyright 2026 AsterSQL.
use crate::group::{GroupState, InvalidGroupId, validate_group_id};
use std::collections::{BTreeMap, HashSet};
pub fn parse_group_ids(raw: &[String]) -> Result<Option<Vec<String>>, InvalidGroupId> {
    if raw.is_empty() {
        return Ok(None);
    }
    let mut ids = Vec::new();
    let mut seen = HashSet::new();
    for value in raw {
        let id = value.trim();
        if id.is_empty() || seen.contains(id) {
            continue;
        }
        validate_group_id(id)?;
        seen.insert(id.to_owned());
        ids.push(id.to_owned());
    }
    Ok(Some(ids))
}
pub trait GroupStateProvider {
    fn all_group_states(&self) -> Vec<GroupState>;
    fn group_state(&self, id: &str) -> Option<GroupState>;
}
pub fn collect_all_group_states(
    provider: &impl GroupStateProvider,
) -> BTreeMap<String, GroupState> {
    provider
        .all_group_states()
        .into_iter()
        .map(|s| (s.group.id.clone(), s))
        .collect()
}
pub fn collect_group_states(
    provider: &impl GroupStateProvider,
    raw: &[String],
) -> Result<BTreeMap<String, GroupState>, InvalidGroupId> {
    let Some(ids) = parse_group_ids(raw)? else {
        return Ok(collect_all_group_states(provider));
    };
    Ok(ids
        .into_iter()
        .filter_map(|id| provider.group_state(&id).map(|s| (id, s)))
        .collect())
}
