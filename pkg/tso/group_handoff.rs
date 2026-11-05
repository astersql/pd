// Copyright 2026 AsterSQL.
//! Durable upper-window handoff before a split/merge target can serve.
use crate::{GroupWatchConfig, TsoError};
use astersql_pd_keypath::{ClusterPaths, MsParam};
use astersql_pd_member::Leadership;
use astersql_pd_storage_domain::{Compare, Consistency, KeyspaceGroup, Mutation, Read};
/// Reserve a target boundary above every source's outstanding durable window.
/// Merge source windows are atomically replaced to fence stale allocators.
/// Metadata and window comparisons prevent publishing a partially observed transition.
pub async fn reserve_group_handoff(
    config: &GroupWatchConfig,
    group: &KeyspaceGroup,
) -> Result<(), TsoError> {
    let p = ClusterPaths::new(config.cluster_id);
    let target_key = p.keyspace_group_id_path(group.id).into_bytes();
    let meta = config
        .store
        .read(Read::prefix(&target_key, Consistency::Linearizable))
        .await?;
    let row = meta
        .entries
        .iter()
        .find(|e| e.key == target_key)
        .ok_or(TsoError::WindowLost)?;
    let current: KeyspaceGroup =
        serde_json::from_slice(&row.value).map_err(|_| TsoError::CorruptBoundary)?;
    if &current != group {
        return Err(TsoError::WindowLost);
    }
    let sources = if group.is_split_target() {
        vec![group.split_source()]
    } else if group.is_merge_target() {
        group
            .merge_state
            .as_ref()
            .and_then(|s| s.merge_list.clone())
            .unwrap_or_default()
    } else {
        return Ok(());
    };
    let cancel = config.cancel.child_token();
    let term = Leadership::campaign_with_key(
        config.store.clone(),
        cancel.clone(),
        config.cluster_id,
        p.election_path(Some(&MsParam {
            service_name: "tso".into(),
            group_id: group.id,
        }))
        .into_bytes(),
        (config.participant)(group.id),
        config.lease_seconds,
        vec![Compare::Value(target_key.clone(), row.value.clone())],
    )
    .await?;
    let work = async {
        let mut compares = vec![Compare::Value(target_key, row.value.clone())];
        let mut windows = vec![];
        let mut maximum = 0u64;
        for id in sources.iter().copied().chain([group.id]) {
            if id != group.id && group.is_merge_target() {
                compares.push(Compare::CreateRevision(
                    p.keyspace_group_id_path(id).into_bytes(),
                    0,
                ));
            }
            let key = p.timestamp_path(id).into_bytes();
            let snapshot = config
                .store
                .read(Read::prefix(&key, Consistency::Linearizable))
                .await?;
            let entry = snapshot.entries.iter().find(|e| e.key == key);
            let value = if let Some(e) = entry {
                compares.push(Compare::ModRevision(key.clone(), e.mod_revision));
                u64::from_be_bytes(
                    e.value
                        .as_slice()
                        .try_into()
                        .map_err(|_| TsoError::CorruptBoundary)?,
                )
            } else {
                compares.push(Compare::CreateRevision(key.clone(), 0));
                0
            };
            if value > i64::MAX as u64 {
                return Err(TsoError::CorruptBoundary);
            }
            maximum = maximum.max(value);
            windows.push((id, key, value));
        }
        let boundary = maximum
            .checked_add(1_000_000)
            .filter(|v| *v <= i64::MAX as u64)
            .ok_or(TsoError::Exhausted)?;
        let mut writes = vec![];
        for (id, key, _) in windows {
            if id == group.id || group.is_merge_target() {
                writes.push(Mutation::put(&key, &boundary.to_be_bytes()));
            }
        }
        let result = term.transact(compares, writes).await?;
        if !result.succeeded {
            return Err(TsoError::WindowLost);
        }
        Ok(())
    }
    .await;
    let cleanup = term.resign().await;
    cancel.cancel();
    work?;
    cleanup?;
    Ok(())
}
