// Copyright 2026 AsterSQL.
use crate::{
    GroupCatalogError, KeyspaceError, KeyspaceGroupManager, KeyspaceManager,
    group_catalog::{GroupIndex, put_index},
    keyspace_remove::RemovalEligibility,
};
use astersql_pd_election::{ElectionError, Leadership};
use astersql_pd_storage_domain::{
    Base, CallbackTxn, Compare, KeyspaceGroup, MAX_BATCH_OPS, Mutation, StorageEndpoint,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use tokio::sync::{MutexGuard, RwLockReadGuard, RwLockWriteGuard};
use tokio_util::sync::CancellationToken;
pub const KEYSPACE_REMOVAL_MAX_OPS: usize = MAX_BATCH_OPS;
pub const KEYSPACE_REMOVAL_MAX_BATCH: usize = (KEYSPACE_REMOVAL_MAX_OPS - 1) / 2;
pub const KEYSPACE_REMOVAL_SINGLE_LIMIT: usize = (KEYSPACE_REMOVAL_MAX_OPS - 1) / 3;
#[derive(Debug, thiserror::Error)]
pub enum GroupRemovalError {
    #[error(transparent)]
    Catalog(#[from] GroupCatalogError),
    #[error(transparent)]
    Keyspace(#[from] KeyspaceError),
    #[error(transparent)]
    Leadership(#[from] ElectionError),
    #[error("context canceled")]
    Cancelled,
    #[error("keyspace and group managers must share the metadata store")]
    StoreMismatch,
    #[error("[PD:keyspace:ErrKeyspaceGroupNotExists]keyspace group {0} does not exist")]
    NotExists(u32),
    #[error("[PD:keyspace:ErrKeyspaceGroupInMerging]keyspace group {0} is in merging state")]
    InMerging(u32),
    #[error("[PD:etcd:ErrEtcdTxnConflict]etcd transaction failed, conflicted and rolled back")]
    Conflict,
    #[error("removal job stopped after an error; retry from authoritative storage")]
    Stopped,
}
/// One confirmed batch. Earlier reports remain committed if a later batch fails.
#[derive(Clone, Debug)]
pub struct GroupRemovalBatch {
    pub group: KeyspaceGroup,
    pub removed_ids: Vec<u32>,
    pub processed_ids: Vec<u32>,
    pub revision: i64,
    pub operation_count: usize,
    pub has_more: bool,
}
enum Requested {
    Single(Vec<u32>),
    Pending(BTreeSet<u32>),
}
/// The host may drive individual batches or consume run(). Drop releases the membership gate.
pub struct GroupRemovalJob<'a> {
    groups: &'a KeyspaceGroupManager,
    keyspaces: &'a KeyspaceManager,
    leadership: Option<&'a Leadership>,
    cancel: CancellationToken,
    group_id: u32,
    requested: Requested,
    gate: Option<RwLockReadGuard<'a, ()>>,
    finished: bool,
    poisoned: bool,
    last: Option<KeyspaceGroup>,
}
#[derive(Default)]
struct Staged {
    removed: Vec<u32>,
    processed: Vec<u32>,
    assignments: BTreeMap<String, i64>,
    has_more: bool,
}
/// A real buffered batch. Dropping it before commit leaves persistent state and cache untouched.
pub struct PreparedGroupRemovalBatch<'j, 'a> {
    job: &'j mut GroupRemovalJob<'a>,
    group: KeyspaceGroup,
    staged: Staged,
    compares: Vec<Compare>,
    writes: Vec<Mutation>,
    index: RwLockWriteGuard<'a, GroupIndex>,
    metadata: MutexGuard<'a, ()>,
    mutation: MutexGuard<'a, ()>,
}
struct CommitSafety<'j, 'a> {
    job: &'j mut GroupRemovalJob<'a>,
    ids: Vec<u32>,
    complete: bool,
}
impl Drop for CommitSafety<'_, '_> {
    fn drop(&mut self) {
        self.job.keyspaces.evict_cached(&self.ids);
        if !self.complete {
            self.job.stop();
        }
    }
}
fn fits(removals: usize, assignments: usize) -> bool {
    1 + 2 * removals + assignments <= KEYSPACE_REMOVAL_MAX_OPS
}
impl KeyspaceGroupManager {
    pub async fn remove_keyspaces_from_group(
        &self,
        cancel: &CancellationToken,
        group_id: u32,
        keyspaces: &KeyspaceManager,
        leadership: Option<&Leadership>,
        ids: &[u32],
    ) -> Result<KeyspaceGroup, GroupRemovalError> {
        self.removal_job(cancel, group_id, keyspaces, leadership, ids)
            .await?
            .run()
            .await
    }
    pub async fn removal_job<'a>(
        &'a self,
        cancel: &CancellationToken,
        group_id: u32,
        keyspaces: &'a KeyspaceManager,
        leadership: Option<&'a Leadership>,
        ids: &[u32],
    ) -> Result<GroupRemovalJob<'a>, GroupRemovalError> {
        if cancel.is_cancelled() {
            return Err(GroupRemovalError::Cancelled);
        }
        if !Arc::ptr_eq(&self.store, &keyspaces.storage.store)
            || self.paths.cluster_id() != keyspaces.storage.paths.cluster_id()
            || keyspaces.resources.meta_groups.as_ref().is_some_and(|m| {
                !Arc::ptr_eq(&self.store, &m.store)
                    || m.paths.cluster_id() != self.paths.cluster_id()
            })
            || leadership.is_some_and(|term| {
                !term.uses_store(&self.store) || term.cluster_id() != self.paths.cluster_id()
            })
        {
            return Err(GroupRemovalError::StoreMismatch);
        }
        if leadership.is_some_and(Leadership::is_expired) {
            return Err(ElectionError::NotLeader.into());
        }
        let gate = tokio::select! {biased;_=cancel.cancelled()=>return Err(GroupRemovalError::Cancelled),guard=self.membership_gate.read()=>guard};
        let requested = if ids.len() <= KEYSPACE_REMOVAL_SINGLE_LIMIT {
            Requested::Single(ids.to_vec())
        } else {
            Requested::Pending(
                ids.iter()
                    .copied()
                    .filter(|id| !keyspaces.resources.kernel.is_protected_id(*id))
                    .collect(),
            )
        };
        Ok(GroupRemovalJob {
            groups: self,
            keyspaces,
            leadership,
            cancel: cancel.clone(),
            group_id,
            requested,
            gate: Some(gate),
            finished: false,
            poisoned: false,
            last: None,
        })
    }
}
impl<'a> GroupRemovalJob<'a> {
    fn stop(&mut self) {
        self.poisoned = true;
        self.finished = true;
        self.gate.take();
    }
    pub async fn run(mut self) -> Result<KeyspaceGroup, GroupRemovalError> {
        while self.next_batch().await?.is_some() {}
        self.last.ok_or(GroupRemovalError::Stopped)
    }
    pub async fn next_batch(&mut self) -> Result<Option<GroupRemovalBatch>, GroupRemovalError> {
        match self.prepare_next_batch().await? {
            Some(batch) => batch.commit().await.map(Some),
            None => Ok(None),
        }
    }
    async fn refresh(&self, index: &mut GroupIndex) {
        let endpoint = StorageEndpoint::new(self.groups.store.clone());
        let mut txn = Base::new(self.groups.store.as_ref()).begin_txn();
        if let Ok(group) = endpoint
            .load_keyspace_group(&mut txn, &self.groups.paths, self.group_id)
            .await
        {
            // This cleanup is an authoritative read, independent of the cancelled caller/old term.
            if txn.commit::<std::convert::Infallible>().await.is_ok() {
                if let Some(group) = group {
                    put_index(index, group);
                } else {
                    for entries in index.values_mut() {
                        entries.remove(&self.group_id);
                    }
                }
            }
        }
    }
    pub async fn prepare_next_batch(
        &mut self,
    ) -> Result<Option<PreparedGroupRemovalBatch<'_, 'a>>, GroupRemovalError> {
        if self.poisoned {
            return Err(GroupRemovalError::Stopped);
        }
        if self.finished {
            return Ok(None);
        }
        if self.cancel.is_cancelled() {
            self.stop();
            return Err(GroupRemovalError::Cancelled);
        }
        let groups = self.groups;
        let keyspaces = self.keyspaces;
        let mutation = tokio::select! {biased;_=self.cancel.cancelled()=>{self.stop();return Err(GroupRemovalError::Cancelled)},guard=groups.membership_mutex.lock()=>guard};
        let mut index = groups.groups.write().await;
        let metadata = keyspaces.mutation.lock().await;
        let mut txn = Base::new(groups.store.as_ref()).begin_txn();
        let mut staged = Staged::default();
        let result = self.stage(&mut txn, &mut staged).await;
        match result {
            Ok(group) => {
                let (compares, writes) = txn.into_parts();
                Ok(Some(PreparedGroupRemovalBatch {
                    job: self,
                    group,
                    staged,
                    compares,
                    writes,
                    index,
                    metadata,
                    mutation,
                }))
            }
            Err(error) => {
                keyspaces.evict_cached(&staged.removed);
                if !staged.removed.is_empty() {
                    self.refresh(&mut index).await;
                }
                drop(metadata);
                drop(index);
                drop(mutation);
                self.stop();
                Err(error)
            }
        }
    }
    async fn stage(
        &self,
        txn: &mut CallbackTxn<'_>,
        staged: &mut Staged,
    ) -> Result<KeyspaceGroup, GroupRemovalError> {
        let endpoint = StorageEndpoint::new(self.groups.store.clone());
        let mut group = endpoint
            .load_keyspace_group(txn, &self.groups.paths, self.group_id)
            .await
            .map_err(GroupCatalogError::from)?
            .ok_or(GroupRemovalError::NotExists(self.group_id))?;
        if group.is_splitting() {
            return Err(GroupCatalogError::InSplit(self.group_id).into());
        }
        if group.is_merging() {
            return Err(GroupRemovalError::InMerging(self.group_id));
        }
        let members: BTreeSet<_> = group.keyspaces.iter().flatten().copied().collect();
        let (single, large, candidates) = match &self.requested {
            Requested::Single(ids) => (true, false, ids.clone()),
            Requested::Pending(ids) if ids.len() <= KEYSPACE_REMOVAL_MAX_BATCH => {
                (false, false, ids.iter().copied().collect())
            }
            Requested::Pending(_) => (false, true, group.keyspaces.clone().unwrap_or_default()),
        };
        let mut removed = BTreeSet::new();
        let mut processed = BTreeSet::new();
        for id in candidates {
            if self.keyspaces.resources.kernel.is_protected_id(id) {
                continue;
            }
            if let Requested::Pending(ids) = &self.requested
                && !ids.contains(&id)
            {
                continue;
            }
            if removed.contains(&id) || (large && processed.contains(&id)) {
                continue;
            }
            if large && staged.processed.len() == KEYSPACE_REMOVAL_MAX_BATCH {
                staged.has_more = true;
                break;
            }
            if !members.contains(&id) {
                if !single {
                    staged.processed.push(id);
                }
                continue;
            }
            let eligibility = {
                let next = staged.removed.len() + 1;
                let counts = &staged.assignments;
                let capacity = |group: &str| {
                    fits(
                        next,
                        counts.len()
                            + usize::from(!group.is_empty() && !counts.contains_key(group)),
                    )
                };
                self.keyspaces
                    .stage_remove_if_eligible(
                        txn,
                        id,
                        true,
                        if single { None } else { Some(&capacity) },
                    )
                    .await?
            };
            match eligibility {
                RemovalEligibility::Deferred { .. } => {
                    staged.has_more = true;
                    break;
                }
                RemovalEligibility::Skipped => {}
                RemovalEligibility::Staged { meta_service_group } => {
                    removed.insert(id);
                    staged.removed.push(id);
                    if !meta_service_group.is_empty() {
                        *staged.assignments.entry(meta_service_group).or_default() += 1;
                    }
                }
            }
            staged.processed.push(id);
            processed.insert(id);
        }
        if !staged.removed.is_empty() {
            if let Some(manager) = &self.keyspaces.resources.meta_groups {
                for (group, count) in &staged.assignments {
                    manager
                        .decrement_in_txn(txn, group, *count)
                        .await
                        .map_err(KeyspaceError::from)?;
                }
            }
            group.keyspaces = Some(
                group
                    .keyspaces
                    .take()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|id| !removed.contains(id))
                    .collect(),
            );
            endpoint
                .save_keyspace_group(txn, &self.groups.paths, &group)
                .map_err(GroupCatalogError::from)?;
        }
        Ok(group)
    }
}
impl PreparedGroupRemovalBatch<'_, '_> {
    pub fn removed_ids(&self) -> &[u32] {
        &self.staged.removed
    }
    pub fn operation_count(&self) -> usize {
        self.writes.len()
    }
    pub async fn commit(self) -> Result<GroupRemovalBatch, GroupRemovalError> {
        let Self {
            job,
            group,
            staged,
            compares,
            writes,
            mut index,
            metadata,
            mutation,
        } = self;
        let operation_count = writes.len();
        debug_assert!(operation_count <= KEYSPACE_REMOVAL_MAX_OPS);
        let mut safety = CommitSafety {
            job,
            ids: staged.removed.clone(),
            complete: false,
        };
        let groups = safety.job.groups;
        let keyspaces = safety.job.keyspaces;
        let term = safety.job.leadership;
        let cancel = safety.job.cancel.clone();
        let result = tokio::select! {biased;_=cancel.cancelled()=>Err(GroupRemovalError::Cancelled),result=async{let response=if let Some(term)=term{term.transact(compares,writes).await?}else{groups.store.transact(compares,writes).await.map_err(GroupCatalogError::from)?};if !response.succeeded{return Err(GroupRemovalError::Conflict);}Ok(response.revision)}=>result};
        keyspaces.evict_cached(&staged.removed);
        match result {
            Err(error) => {
                if !staged.removed.is_empty() {
                    safety.job.refresh(&mut index).await;
                }
                drop(metadata);
                drop(index);
                drop(mutation);
                safety.job.stop();
                safety.complete = true;
                Err(error)
            }
            Ok(revision) => {
                put_index(&mut index, group.clone());
                drop(metadata);
                drop(index);
                drop(mutation);
                let done = match &mut safety.job.requested {
                    Requested::Single(_) => true,
                    Requested::Pending(pending) => {
                        for id in &staged.processed {
                            pending.remove(id);
                        }
                        if !staged.has_more {
                            pending.clear();
                        }
                        !staged.has_more
                    }
                };
                safety.job.last = Some(group.clone());
                if done {
                    safety.job.finished = true;
                    safety.job.gate.take();
                }
                keyspaces.publish_removed(&staged.removed);
                safety.complete = true;
                Ok(GroupRemovalBatch {
                    group,
                    removed_ids: staged.removed,
                    processed_ids: staged.processed,
                    revision,
                    operation_count,
                    has_more: staged.has_more,
                })
            }
        }
    }
}
