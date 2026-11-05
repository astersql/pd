// Copyright 2026 AsterSQL.
use crate::{
    GroupCatalogError, KeyspaceGroupManager,
    group_catalog::{commit_error, put_index},
};
use astersql_pd_storage_domain::{Base, MAX_BATCH_OPS, MergeState, StorageEndpoint};
use std::{
    collections::{BTreeMap, BTreeSet},
    convert::Infallible,
};
/// Group merge validation and actual transaction failures.
#[derive(Debug, thiserror::Error)]
pub enum GroupMergeError {
    #[error(transparent)]
    Catalog(#[from] GroupCatalogError),
    #[error("[PD:keyspace:ErrKeyspaceGroupNotExists]keyspace group {0} does not exist")]
    NotExists(u32),
    #[error("[PD:keyspace:ErrKeyspaceGroupInMerging]keyspace group {0} is in merging state")]
    InMerging(u32),
    #[error("[PD:keyspace:ErrKeyspaceGroupNotInMerging]keyspace group {0} is not in merging state")]
    NotInMerging(u32),
    #[error("[PD:keyspace:ErrExceedMaxEtcdTxnOps]exceed max etcd txn operations")]
    Capacity,
}
impl KeyspaceGroupManager {
    /// Merge source groups atomically into one persisted target, including target zero.
    /// An empty list succeeds without reading the target; sources may never include zero.
    pub async fn merge_groups(&self, target: u32, sources: &[u32]) -> Result<(), GroupMergeError> {
        if sources.is_empty() {
            return Ok(());
        }
        if sources.len() > MAX_BATCH_OPS / 2 - 1 {
            return Err(GroupMergeError::Capacity);
        }
        if sources.contains(&0) {
            return Err(GroupCatalogError::Storage(
                astersql_pd_storage_domain::KeyspaceGroupStoreError::ModifyDefaultKeyspaceGroup,
            )
            .into());
        }
        let _stability = self.membership_gate.write().await;
        let _membership = self.membership_mutex.lock().await;
        let mut index = self.groups.write().await;
        let endpoint = StorageEndpoint::new(self.store.clone());
        let mut txn = Base::new(self.store.as_ref()).begin_txn();
        let mut groups = BTreeMap::new();
        for id in sources.iter().copied().chain([target]) {
            let group = endpoint
                .load_keyspace_group(&mut txn, &self.paths, id)
                .await
                .map_err(GroupCatalogError::from)?
                .ok_or(GroupMergeError::NotExists(id))?;
            if group.is_splitting() {
                return Err(GroupCatalogError::InSplit(id).into());
            }
            if group.is_merging() {
                return Err(GroupMergeError::InMerging(id));
            }
            groups.insert(id, group);
        }
        let keys: BTreeSet<_> = groups
            .values()
            .flat_map(|g| g.keyspaces.iter().flatten().copied())
            .collect();
        let mut merged = groups.remove(&target).unwrap();
        merged.keyspaces = Some(keys.into_iter().collect());
        merged.merge_state = Some(MergeState {
            merge_list: Some(sources.to_vec()),
        });
        endpoint
            .save_keyspace_group(&mut txn, &self.paths, &merged)
            .map_err(GroupCatalogError::from)?;
        for &id in sources {
            endpoint
                .delete_keyspace_group(&mut txn, &self.paths, id)
                .map_err(GroupCatalogError::from)?;
        }
        txn.commit::<Infallible>().await.map_err(commit_error)?;
        put_index(&mut index, merged);
        for &id in sources {
            for groups in index.values_mut() {
                groups.remove(&id);
            }
        }
        Ok(())
    }
    /// Finish only after every recorded source has disappeared, using guarded reads.
    pub async fn finish_merge(&self, target: u32) -> Result<(), GroupMergeError> {
        self.finish_merge_checked(target, None).await
    }
    /// Complete exactly the transition whose durable TSO handoff was established.
    pub async fn finish_merge_handoff(
        &self,
        expected: &astersql_pd_storage_domain::KeyspaceGroup,
    ) -> Result<(), GroupMergeError> {
        self.finish_merge_checked(expected.id, Some(expected)).await
    }
    async fn finish_merge_checked(
        &self,
        target: u32,
        expected: Option<&astersql_pd_storage_domain::KeyspaceGroup>,
    ) -> Result<(), GroupMergeError> {
        let mut index = self.groups.write().await;
        let endpoint = StorageEndpoint::new(self.store.clone());
        let mut txn = Base::new(self.store.as_ref()).begin_txn();
        let mut group = endpoint
            .load_keyspace_group(&mut txn, &self.paths, target)
            .await
            .map_err(GroupCatalogError::from)?
            .ok_or(GroupMergeError::NotExists(target))?;
        if expected.is_some_and(|e| e != &group) {
            return Err(GroupCatalogError::Conflict.into());
        }
        if !group.is_merge_target() {
            return Err(GroupMergeError::NotInMerging(target));
        }
        for id in group
            .merge_state
            .as_ref()
            .and_then(|state| state.merge_list.as_ref())
            .into_iter()
            .flatten()
            .copied()
        {
            if endpoint
                .load_keyspace_group(&mut txn, &self.paths, id)
                .await
                .map_err(GroupCatalogError::from)?
                .is_some()
            {
                return Err(GroupMergeError::NotInMerging(id));
            }
        }
        group.merge_state = None;
        endpoint
            .save_keyspace_group(&mut txn, &self.paths, &group)
            .map_err(GroupCatalogError::from)?;
        txn.commit::<Infallible>().await.map_err(commit_error)?;
        put_index(&mut index, group);
        Ok(())
    }
}
