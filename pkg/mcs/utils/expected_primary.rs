// Copyright 2026 AsterSQL.
use astersql_pd_election::Leadership;
use astersql_pd_keypath::{ClusterPaths, MsParam};
use astersql_pd_mcs_discovery::ServiceRegistryEntry;
use astersql_pd_storage_domain::{Compare, Consistency, MetaStore, Mutation, Read};
use astersql_pd_typeutil::equal_base_urls;
use std::collections::BTreeMap;
/// Dedupe a logical node by its newest registered start timestamp.
pub fn latest_entries(entries: &[ServiceRegistryEntry]) -> Vec<&ServiceRegistryEntry> {
    let mut latest = BTreeMap::<&str, &ServiceRegistryEntry>::new();
    for e in entries {
        if latest
            .get(e.name.as_str())
            .is_none_or(|old| old.start_timestamp < e.start_timestamp)
        {
            latest.insert(&e.name, e);
        }
    }
    latest.into_values().collect()
}
pub fn identifies(entry: &ServiceRegistryEntry, name: &str) -> bool {
    !name.is_empty() && (entry.name == name || equal_base_urls(&entry.service_addr, name))
}
pub fn is_valid_primary_candidate(
    entries: &[ServiceRegistryEntry],
    name: &str,
    members: &[String],
) -> bool {
    name.is_empty()
        || entries.iter().any(|e| {
            identifies(e, name) && members.iter().any(|m| equal_base_urls(m, &e.service_addr))
        })
}
/// Read the affinity marker. Read errors must not become an empty successful result.
pub async fn expected_primary(
    store: &dyn MetaStore,
    paths: &ClusterPaths,
    param: &MsParam,
) -> Result<Option<astersql_pd_storage_domain::Entry>, String> {
    Ok(store
        .read(Read::range(
            paths.expected_primary_path(param).as_bytes(),
            b"",
            Consistency::Linearizable,
        ))
        .await
        .map_err(|e| e.to_string())?
        .entries
        .into_iter()
        .next())
}
/// Publish the new target under the serving term before revoking that term's lease.
pub async fn transfer_primary(
    store: &dyn MetaStore,
    term: &Leadership,
    paths: &ClusterPaths,
    param: &MsParam,
    entries: &[ServiceRegistryEntry],
    old: &str,
    target: &str,
    members: &[String],
    ttl: i64,
) -> Result<(), String> {
    use rand::seq::SliceRandom;
    term.validate().await.map_err(|_| {
        "current member is not serving as primary, please check leadership".to_owned()
    })?;
    let entries = latest_entries(entries);
    if !target.is_empty()
        && entries.iter().any(|e| {
            identifies(e, target)
                && identifies(e, old)
                && members.iter().any(|m| equal_base_urls(m, &e.service_addr))
        })
    {
        return Ok(());
    }
    if entries.len() == 1 {
        return Err(format!(
            "no valid secondary to transfer primary, the only member is {}",
            entries[0].name
        ));
    }
    let candidates = entries
        .iter()
        .filter(|e| {
            members.iter().any(|m| equal_base_urls(m, &e.service_addr))
                && if target.is_empty() {
                    !identifies(e, old)
                } else {
                    identifies(e, target)
                }
        })
        .collect::<Vec<_>>();
    let selected = candidates
        .choose(&mut rand::thread_rng())
        .ok_or_else(|| format!("no valid secondary to transfer primary, from {old} to {target}"))?
        .service_addr
        .clone();
    let key = paths.expected_primary_path(param).into_bytes();
    let observed = expected_primary(store, paths, param).await?;
    let guard = if let Some(row) = observed {
        vec![Compare::ModRevision(key.clone(), row.mod_revision)]
    } else {
        vec![Compare::CreateRevision(key.clone(), 0)]
    };
    let lease = store
        .grant(ttl.max(1) * 3)
        .await
        .map_err(|e| format!("failed to grant lease for expected primary, err: {e}"))?;
    let result = term
        .transact(
            guard,
            vec![Mutation::Put {
                key,
                value: selected.into_bytes(),
                lease: Some(lease.id),
            }],
        )
        .await;
    match result {
        Ok(c) if c.succeeded => term.resign().await.map_err(|e| e.to_string()),
        other => {
            let _ = store.revoke(lease.id).await;
            Err(format!(
                "failed to mark expected primary flag for {}, err: {other:?}",
                param.service_name
            ))
        }
    }
}
/// Delete only the marker observed while campaigning; never delete a newer transfer.
pub async fn clear_expected_primary(
    store: &dyn MetaStore,
    key: Vec<u8>,
    entry: &astersql_pd_storage_domain::Entry,
) -> Result<(), String> {
    let c = store
        .transact(
            vec![
                Compare::Value(key.clone(), entry.value.clone()),
                Compare::ModRevision(key.clone(), entry.mod_revision),
            ],
            vec![Mutation::Delete { key }],
        )
        .await
        .map_err(|e| e.to_string())?;
    if c.succeeded {
        if let Some(id) = entry.lease {
            store.revoke(id).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
