// Copyright 2026 AsterSQL.
use crate::{
    GroupCatalogError, Kernel, KeyspaceGroupManager,
    group_catalog::{commit_error, put_index},
};
use astersql_pd_mcs_constant::DEFAULT_KEYSPACE_GROUP_REPLICA_COUNT;
use astersql_pd_storage_domain::{Base, KeyspaceGroup, SplitState, StorageEndpoint};
use std::{collections::BTreeSet, convert::Infallible};
/// Split validation and actual storage transaction failures.
#[derive(Debug, thiserror::Error)]
pub enum GroupSplitError {
    #[error(transparent)]
    Catalog(#[from] GroupCatalogError),
    #[error(transparent)]
    Leadership(#[from] astersql_pd_election::ElectionError),
    #[error("[PD:keyspace:ErrKeyspaceGroupNotExists]keyspace group {0} does not exist")]
    NotExists(u32),
    #[error("[PD:keyspace:ErrKeyspaceGroupNotInSplit]keyspace group {0} is not in split state")]
    NotInSplit(u32),
    #[error("[PD:keyspace:ErrKeyspaceGroupInMerging]keyspace group {0} is in merging state")]
    InMerging(u32),
    #[error(
        "[PD:keyspace:ErrKeyspaceGroupNotEnoughReplicas]not enough replicas in the keyspace group"
    )]
    NotEnoughReplicas,
    #[error("[PD:keyspace:ErrKeyspaceNotInKeyspaceGroup]keyspace is not in this keyspace group")]
    NotInGroup,
    #[error("[PD:keyspace:ErrKeyspaceGroupWithEmptyKeyspace]keyspace group with empty keyspace")]
    Empty,
    #[error("{0}")]
    Validation(String),
}
/// Partition by an explicit list (taking precedence) or an inclusive range.
/// Duplicate explicit IDs are deduplicated; their Go map order is unspecified.
pub fn build_split_keyspaces(
    old: &[u32],
    new: &[u32],
    start: u32,
    end: u32,
    kernel: Kernel,
) -> Result<(Vec<u32>, Vec<u32>), GroupSplitError> {
    let mut selected = BTreeSet::new();
    let target = if !new.is_empty() {
        if new.len() > old.len() {
            return Err(GroupSplitError::NotInGroup);
        }
        let existing: BTreeSet<_> = old.iter().copied().collect();
        for &id in new {
            if kernel.is_protected_id(id) {
                return Err(GroupSplitError::Validation(match kernel {
                    Kernel::Classic=>"[PD:keyspace:ErrModifyDefaultKeyspace]cannot modify default keyspace's state",
                    Kernel::NextGeneration=>"[PD:keyspace:ErrModifyReservedKeyspace]cannot modify reserved keyspace's state",
                }.into()));
            }
            if !existing.contains(&id) {
                return Err(GroupSplitError::NotInGroup);
            }
            selected.insert(id);
        }
        if selected.len() == new.len() {
            new.to_vec()
        } else {
            selected.iter().copied().collect()
        }
    } else {
        if start == 0 && end == 0 {
            return Err(GroupSplitError::NotInGroup);
        }
        let target: Vec<_> = old
            .iter()
            .copied()
            .filter(|&id| !kernel.is_protected_id(id) && start <= id && id <= end)
            .collect();
        if target.is_empty() {
            return Err(GroupSplitError::Empty);
        }
        selected.extend(target.iter().copied());
        target
    };
    Ok((
        old.iter()
            .copied()
            .filter(|id| !selected.contains(id))
            .collect(),
        target,
    ))
}
impl KeyspaceGroupManager {
    /// Atomically partition source metadata and create its split target.
    pub async fn split_group(
        &self,
        source: u32,
        target: u32,
        keys: &[u32],
        range: &[u32],
    ) -> Result<(), GroupSplitError> {
        self.split_group_in_term(source, target, keys, range, None, None)
            .await
    }
    pub(crate) async fn split_group_in_term(
        &self,
        source: u32,
        target: u32,
        keys: &[u32],
        range: &[u32],
        cancel: Option<&tokio_util::sync::CancellationToken>,
        term: Option<&crate::GroupReconcileTerm>,
    ) -> Result<(), GroupSplitError> {
        if cancel.is_some_and(tokio_util::sync::CancellationToken::is_cancelled) {
            return Err(GroupCatalogError::Store(
                astersql_pd_storage_domain::StoreError::Cancelled,
            )
            .into());
        }
        if let Some(term) = term {
            self.with_reconcile_term(term, || ())
                .map_err(GroupCatalogError::Store)?;
        }
        let _stability = self.membership_gate.write().await;
        let _membership = self.membership_mutex.lock().await;
        let mut index = self.groups.write().await;
        let endpoint = StorageEndpoint::new(self.store.clone());
        let mut txn = Base::new(self.store.as_ref()).begin_txn();
        let mut source_group = endpoint
            .load_keyspace_group(&mut txn, &self.paths, source)
            .await
            .map_err(GroupCatalogError::from)?
            .ok_or(GroupSplitError::NotExists(source))?;
        if source_group.is_splitting() {
            return Err(GroupCatalogError::InSplit(source).into());
        }
        if source_group.is_merging() {
            return Err(GroupSplitError::InMerging(source));
        }
        let (start, end) = if range.len() >= 2 {
            (range[0], range[1])
        } else {
            (0, 0)
        };
        let (remaining, moved) = build_split_keyspaces(
            source_group.keyspaces.as_deref().unwrap_or_default(),
            keys,
            start,
            end,
            Kernel::current(),
        )?;
        if source_group.members.as_ref().map_or(0, Vec::len) < DEFAULT_KEYSPACE_GROUP_REPLICA_COUNT
        {
            return Err(GroupSplitError::NotEnoughReplicas);
        }
        if endpoint
            .load_keyspace_group(&mut txn, &self.paths, target)
            .await
            .map_err(GroupCatalogError::from)?
            .is_some()
        {
            return Err(GroupCatalogError::Exists.into());
        }
        source_group.keyspaces = Some(remaining);
        source_group.split_state = Some(SplitState {
            split_source: source,
        });
        let target_group = KeyspaceGroup {
            id: target,
            user_kind: source_group.user_kind.clone(),
            members: source_group.members.clone(),
            keyspaces: Some(moved),
            split_state: Some(SplitState {
                split_source: source,
            }),
            ..Default::default()
        };
        endpoint
            .save_keyspace_group(&mut txn, &self.paths, &target_group)
            .map_err(GroupCatalogError::from)?;
        endpoint
            .save_keyspace_group(&mut txn, &self.paths, &source_group)
            .map_err(GroupCatalogError::from)?;
        if cancel.is_some_and(tokio_util::sync::CancellationToken::is_cancelled) {
            return Err(GroupCatalogError::Store(
                astersql_pd_storage_domain::StoreError::Cancelled,
            )
            .into());
        }
        let commit = async {
            if let Some(term) = term {
                let (compares, writes) = txn.into_parts();
                let commit = term.leadership.transact(compares, writes).await?;
                if !commit.succeeded {
                    return Err(GroupCatalogError::Conflict.into());
                }
                Ok(())
            } else {
                txn.commit::<Infallible>()
                    .await
                    .map_err(commit_error)
                    .map_err(GroupSplitError::from)
            }
        };
        if let Some(cancel) = cancel {
            tokio::select! {biased;result=commit=>result?,_=cancel.cancelled()=>return Err(GroupCatalogError::Store(astersql_pd_storage_domain::StoreError::Unavailable).into())}
        } else {
            commit.await?;
        }
        if let Some(term) = term {
            self.with_reconcile_term(term, || {
                put_index(&mut index, source_group);
                put_index(&mut index, target_group);
            })
            .map_err(GroupCatalogError::Store)?;
        } else {
            put_index(&mut index, source_group);
            put_index(&mut index, target_group);
        }
        Ok(())
    }
    /// Clear both split states by target ID after validating current stored roles.
    pub async fn finish_split(&self, target: u32) -> Result<(), GroupSplitError> {
        let mut index = self.groups.write().await;
        let endpoint = StorageEndpoint::new(self.store.clone());
        let mut txn = Base::new(self.store.as_ref()).begin_txn();
        let mut target_group = endpoint
            .load_keyspace_group(&mut txn, &self.paths, target)
            .await
            .map_err(GroupCatalogError::from)?
            .ok_or(GroupSplitError::NotExists(target))?;
        if !target_group.is_split_target() {
            return Err(GroupSplitError::NotInSplit(target));
        }
        let source = target_group.split_source();
        let mut source_group = endpoint
            .load_keyspace_group(&mut txn, &self.paths, source)
            .await
            .map_err(GroupCatalogError::from)?
            .ok_or(GroupSplitError::NotExists(source))?;
        if !source_group.is_split_source() {
            return Err(GroupSplitError::NotInSplit(source));
        }
        target_group.split_state = None;
        source_group.split_state = None;
        endpoint
            .save_keyspace_group(&mut txn, &self.paths, &target_group)
            .map_err(GroupCatalogError::from)?;
        endpoint
            .save_keyspace_group(&mut txn, &self.paths, &source_group)
            .map_err(GroupCatalogError::from)?;
        txn.commit::<Infallible>().await.map_err(commit_error)?;
        put_index(&mut index, target_group);
        put_index(&mut index, source_group);
        Ok(())
    }
}
