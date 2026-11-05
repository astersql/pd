// Copyright 2026 AsterSQL.
//! Preparatory operations for group transitions, over the canonical metadata transaction.
use crate::{
    GroupCatalogError, GroupMergeError, KeyspaceError, KeyspaceGroupManager, KeyspaceManager,
    group_catalog::put_index,
};
use astersql_pd_storage_domain::{Base, MAX_BATCH_OPS, StorageEndpoint};
use std::{convert::Infallible, time::Duration};
use tokio_util::sync::CancellationToken;
impl KeyspaceManager {
    /// Complete the first HTTP split's legacy unassigned-keyspace patrol once per manager.
    /// A failed batch retains its cursor and never publishes uncommitted metadata.
    pub async fn patrol_group_assignment(
        &self,
        start: u32,
        end: u32,
        cancel: &CancellationToken,
    ) -> Result<(), KeyspaceError> {
        let mut cursor = self.split_patrol.lock().await;
        if *cursor == Some(u32::MAX) {
            return Ok(());
        }
        let mut next = cursor.unwrap_or_default().max(start);
        let groups = self.resources.groups.as_ref().ok_or_else(|| {
            KeyspaceError::Validation("keyspace group manager is not initialized".into())
        })?;
        loop {
            if cancel.is_cancelled() {
                return Err(astersql_pd_storage_domain::StoreError::Cancelled.into());
            }
            if end != 0 && next > end {
                break;
            }
            let _gate = groups.membership_gate.write().await;
            let _membership = groups.membership_mutex.lock().await;
            let mut index = groups.groups.write().await;
            let _mutation = self.mutation.lock().await;
            let s = &self.storage;
            let mut txn = Base::new(s.store.as_ref()).begin_txn();
            let endpoint = StorageEndpoint::new(s.store.clone());
            let mut group = endpoint
                .load_keyspace_group(&mut txn, &s.paths, 0)
                .await
                .map_err(GroupCatalogError::from)?
                .ok_or_else(|| {
                    KeyspaceError::Validation("default keyspace group 0 not found".into())
                })?;
            if group.is_splitting() {
                return Err(GroupCatalogError::InSplit(0).into());
            }
            if group.is_merging() {
                return Err(KeyspaceError::Validation(
                    "[PD:keyspace:ErrKeyspaceGroupInMerging]keyspace group 0 is in merging state"
                        .into(),
                ));
            }
            // Leave room for the group write. Existing Go rows already carrying a group are read-only.
            let metas = s
                .load_range_keyspace(&mut txn, &s.paths, next, MAX_BATCH_OPS - 1)
                .await?;
            let mut more = metas.len() == MAX_BATCH_OPS - 1;
            let mut changed = false;
            for mut meta in metas {
                if end != 0 && meta.id() > end {
                    more = false;
                    break;
                }
                next = meta.id() + 1;
                if meta.config.contains_key("tso_keyspace_group_id") {
                    continue;
                }
                meta.config
                    .insert("tso_keyspace_group_id".into(), "0".into());
                let keys = group.keyspaces.get_or_insert_default();
                if !keys.contains(&meta.id()) {
                    keys.push(meta.id());
                    changed = true;
                }
                s.save_keyspace_meta(&mut txn, &s.paths, &meta)?;
            }
            if changed {
                endpoint
                    .save_keyspace_group(&mut txn, &s.paths, &group)
                    .map_err(GroupCatalogError::from)?;
            }
            txn.commit::<Infallible>()
                .await
                .map_err(crate::keyspace_create::commit_error)?;
            put_index(&mut index, group);
            *cursor = Some(next);
            if !more {
                break;
            }
        }
        *cursor = Some(u32::MAX);
        Ok(())
    }
}
impl KeyspaceGroupManager {
    /// Merge bounded batches in Go user-kind order and await each real service completion.
    /// Cancellation/timeout preserves the last committed transition for recovery.
    pub async fn merge_all_into_default(
        &self,
        cancel: &CancellationToken,
    ) -> Result<(), GroupMergeError> {
        let snapshot: Vec<Vec<u32>> = self
            .groups
            .read()
            .await
            .values()
            .map(|g| g.keys().copied().filter(|id| *id != 0).collect())
            .collect();
        for ids in snapshot {
            for batch in ids.chunks(MAX_BATCH_OPS / 2 - 1) {
                self.merge_groups(0, batch).await?;
                let deadline = tokio::time::sleep(Duration::from_secs(60));
                tokio::pin!(deadline);
                loop {
                    tokio::select! {biased;_=cancel.cancelled()=>return Ok(()),_=&mut deadline=>return Ok(()),_=tokio::time::sleep(Duration::from_secs(1))=>{}}
                    if self
                        .get_group(0)
                        .await?
                        .is_some_and(|g| !g.is_merge_target())
                    {
                        break;
                    }
                }
            }
        }
        Ok(())
    }
}
