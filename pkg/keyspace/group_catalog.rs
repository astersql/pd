// Copyright 2026 AsterSQL.
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{
    Base, CallbackTxnError, KeyspaceGroup, KeyspaceGroupStoreError, MetaStore, StorageEndpoint,
    StoreError, UserKind,
};
use std::{collections::BTreeMap, convert::Infallible, sync::Arc};
use tokio::sync::{Mutex, RwLock};

/// Catalog errors retain Go's default and split-group deletion guards.
#[derive(Debug, thiserror::Error)]
pub enum GroupCatalogError {
    #[error("[PD:keyspace:ErrKeyspaceGroupExists]keyspace group already exists")]
    Exists,
    #[error("[PD:keyspace:ErrKeyspaceGroupInSplit]keyspace group {0} is in split state")]
    InSplit(u32),
    #[error(transparent)]
    Storage(#[from] KeyspaceGroupStoreError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("transaction conflict")]
    Conflict,
}
pub(crate) fn commit_error(error: CallbackTxnError<Infallible>) -> GroupCatalogError {
    match error {
        CallbackTxnError::Conflict => GroupCatalogError::Conflict,
        CallbackTxnError::Store(error) => GroupCatalogError::Store(error),
        CallbackTxnError::Callback(error) => match error {},
    }
}
pub(crate) type GroupIndex = BTreeMap<i64, BTreeMap<u32, KeyspaceGroup>>;
pub(crate) fn empty_index() -> GroupIndex {
    (0..3).map(|kind| (kind, BTreeMap::new())).collect()
}
pub(crate) fn put_index(index: &mut GroupIndex, group: KeyspaceGroup) {
    for entries in index.values_mut() {
        entries.remove(&group.id);
    }
    index
        .entry(UserKind::from_name(&group.user_kind).0)
        .or_default()
        .insert(group.id, group);
}

/// TSO keyspace group metadata with a host-owned store and local user-kind index.
pub struct KeyspaceGroupManager {
    pub(crate) store: Arc<dyn MetaStore>,
    pub(crate) paths: ClusterPaths,
    pub(crate) groups: RwLock<GroupIndex>,
    pub(crate) membership_mutex: Mutex<()>,
    pub(crate) nodes: Arc<astersql_pd_balancer::RoundRobin<String>>,
    pub(crate) reconcile: std::sync::Mutex<Option<std::sync::Weak<crate::GroupReconcileTerm>>>,
}
impl KeyspaceGroupManager {
    pub fn new(store: Arc<dyn MetaStore>, paths: ClusterPaths) -> Self {
        Self {
            store,
            paths,
            groups: RwLock::new(empty_index()),
            membership_mutex: Mutex::new(()),
            nodes: Arc::new(astersql_pd_balancer::RoundRobin::new()),
            reconcile: std::sync::Mutex::new(None),
        }
    }
    /// Ensure the classic default group and reload all current user-kind entries.
    pub async fn bootstrap(&self) -> Result<(), GroupCatalogError> {
        let default = KeyspaceGroup {
            id: 0,
            user_kind: "basic".into(),
            keyspaces: Some(vec![0]),
            ..Default::default()
        };
        match self.create_groups(&[default]).await {
            Ok(()) | Err(GroupCatalogError::Exists) => {}
            Err(error) => return Err(error),
        }
        let groups = self.get_groups(0, 0).await?;
        let mut index = empty_index();
        for group in groups {
            put_index(&mut index, group);
        }
        *self.groups.write().await = index;
        Ok(())
    }
    /// Create a batch atomically; an existing group aborts every buffered write.
    pub async fn create_groups(&self, groups: &[KeyspaceGroup]) -> Result<(), GroupCatalogError> {
        let _membership = self.membership_mutex.lock().await;
        let mut index = self.groups.write().await;
        let endpoint = StorageEndpoint::new(self.store.clone());
        let mut txn = Base::new(self.store.as_ref()).begin_txn();
        for group in groups {
            if endpoint
                .load_keyspace_group(&mut txn, &self.paths, group.id)
                .await?
                .is_some()
            {
                return Err(GroupCatalogError::Exists);
            }
            let value = KeyspaceGroup {
                id: group.id,
                user_kind: group.user_kind.clone(),
                members: group.members.clone(),
                keyspaces: group.keyspaces.clone(),
                ..Default::default()
            };
            endpoint.save_keyspace_group(&mut txn, &self.paths, &value)?;
        }
        txn.commit::<Infallible>().await.map_err(commit_error)?;
        for group in groups {
            put_index(&mut index, group.clone());
        }
        Ok(())
    }
    /// Inclusive start; nonpositive limits return all remaining groups.
    pub async fn get_groups(
        &self,
        start: u32,
        limit: i64,
    ) -> Result<Vec<KeyspaceGroup>, GroupCatalogError> {
        Ok(StorageEndpoint::new(self.store.clone())
            .load_keyspace_groups(
                &self.paths,
                start,
                if limit > 0 {
                    limit.try_into().unwrap_or(usize::MAX)
                } else {
                    0
                },
            )
            .await?)
    }
    /// Read current persisted metadata, including absence, with revision protection.
    pub async fn get_group(&self, id: u32) -> Result<Option<KeyspaceGroup>, GroupCatalogError> {
        let endpoint = StorageEndpoint::new(self.store.clone());
        let mut txn = Base::new(self.store.as_ref()).begin_txn();
        let group = endpoint
            .load_keyspace_group(&mut txn, &self.paths, id)
            .await?;
        txn.commit::<Infallible>().await.map_err(commit_error)?;
        Ok(group)
    }
    /// Protect group zero and splitting groups; remove indexes only after commit.
    pub async fn delete_group(&self, id: u32) -> Result<Option<KeyspaceGroup>, GroupCatalogError> {
        if id == 0 {
            return Err(KeyspaceGroupStoreError::ModifyDefaultKeyspaceGroup.into());
        }
        let _membership = self.membership_mutex.lock().await;
        let mut index = self.groups.write().await;
        let endpoint = StorageEndpoint::new(self.store.clone());
        let mut txn = Base::new(self.store.as_ref()).begin_txn();
        let group = endpoint
            .load_keyspace_group(&mut txn, &self.paths, id)
            .await?;
        if let Some(group) = &group {
            if group.is_splitting() {
                return Err(GroupCatalogError::InSplit(id));
            }
            endpoint.delete_keyspace_group(&mut txn, &self.paths, id)?;
        }
        txn.commit::<Infallible>().await.map_err(commit_error)?;
        if group.is_some() {
            for entries in index.values_mut() {
                entries.remove(&id);
            }
        }
        Ok(group)
    }
    /// Inspect owned IDs in an initialized user-kind index; unknown enum values are absent.
    pub async fn indexed_group_ids(&self, kind: UserKind) -> Option<Vec<u32>> {
        self.groups
            .read()
            .await
            .get(&kind.0)
            .map(|entries| entries.keys().copied().collect())
    }
}
