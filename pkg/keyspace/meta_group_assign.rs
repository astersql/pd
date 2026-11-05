// Copyright 2026 AsterSQL.

use std::{collections::BTreeMap, sync::Arc};

use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{
    Base, CallbackTxn, CallbackTxnError, EndpointError, MetaStore, StorageEndpoint,
};
use tokio::sync::{Mutex, RwLock, RwLockReadGuard};

/// Failure to select or persist a meta-service group reservation.
#[derive(Debug, thiserror::Error)]
pub enum GroupAssignmentError {
    #[error("no available meta-service groups")]
    NoAvailableGroups,
    #[error("[PD:keyspace:ErrUnknownMetaServiceGroup]unknown meta-service group")]
    UnknownGroup,
    #[error("[PD:keyspace:ErrMetaServiceGroupDisabled]meta-service group is disabled")]
    DisabledGroup,
    #[error("invalid assignment count")]
    InvalidCount,
    #[error(transparent)]
    Storage(#[from] EndpointError),
}

/// Group selection and reservations over the host's shared MetaStore.
/// Assignment guards serialize directory publication with persisted reservations.
pub struct MetaServiceGroupManager {
    pub(crate) store: Arc<dyn MetaStore>,
    pub(crate) paths: ClusterPaths,
    pub(crate) groups: RwLock<BTreeMap<String, String>>,
    pub(crate) update_mutex: Mutex<()>,
    pub(crate) assignment_counter: Option<Arc<dyn crate::GroupAssignmentCounter>>,
}

impl MetaServiceGroupManager {
    /// Use the existing host store and configured group IDs and addresses.
    pub fn new(
        store: Arc<dyn MetaStore>,
        paths: ClusterPaths,
        groups: BTreeMap<String, String>,
    ) -> Self {
        Self {
            store,
            paths,
            groups: RwLock::new(groups),
            update_mutex: Mutex::new(()),
            assignment_counter: None,
        }
    }

    /// Reserve one keyspace in an enabled group having the minimum count.
    /// Like Go's implementation, selection increments the persisted counter.
    pub async fn pick_group(&self) -> Result<String, CallbackTxnError<GroupAssignmentError>> {
        self.assign_to_group(1).await
    }

    /// Add a nonnegative count in one guarded transaction, without automatic retry.
    pub async fn assign_to_group<'store>(
        &'store self,
        count: i64,
    ) -> Result<String, CallbackTxnError<GroupAssignmentError>> {
        if count < 0 {
            return Err(CallbackTxnError::Callback(
                GroupAssignmentError::InvalidCount,
            ));
        }
        let mut txn = Base::new(self.store.as_ref()).begin_txn();
        let group = self
            .assign_in_txn(&mut txn, count)
            .await
            .map_err(CallbackTxnError::Callback)?;
        txn.commit().await?;
        Ok(group.group)
    }

    /// Buffer a reservation in a caller-owned transaction. Keep the returned guard
    /// alive until commit or rollback to exclude concurrent directory publication.
    /// Callback errors discard both the reservation and other buffered writes.
    pub async fn assign_in_txn<'store>(
        &'store self,
        txn: &mut CallbackTxn<'store>,
        count: i64,
    ) -> Result<GroupReservation<'store>, GroupAssignmentError> {
        if count < 0 {
            return Err(GroupAssignmentError::InvalidCount);
        }
        let groups = self.groups.read().await;
        let endpoint = StorageEndpoint::new(self.store.clone());
        let statuses = endpoint
            .load_meta_service_group_status(txn, &self.paths, &groups)
            .await?;
        let (id, status) = statuses
            .iter()
            .filter(|(id, status)| {
                !id.is_empty() && status.enabled && status.assignment_count < i64::MAX
            })
            .min_by_key(|(_, status)| status.assignment_count)
            .ok_or(GroupAssignmentError::NoAvailableGroups)?;
        let mut status = status.clone();
        status.assignment_count = status.assignment_count.wrapping_add(count);
        endpoint.save_meta_service_group_status(txn, &self.paths, id, Some(&status))?;
        Ok(GroupReservation {
            group: id.clone(),
            _groups: groups,
        })
    }

    /// Compensate a failed keyspace creation without reviving a deleted group.
    pub async fn rollback_assignment<'store>(
        &'store self,
        group: &str,
    ) -> Result<(), CallbackTxnError<GroupAssignmentError>> {
        let mut txn = Base::new(self.store.as_ref()).begin_txn();
        self.rollback_in_txn(&mut txn, group)
            .await
            .map_err(CallbackTxnError::Callback)?;
        txn.commit().await
    }

    pub(crate) async fn rollback_in_txn(
        &self,
        txn: &mut CallbackTxn<'_>,
        group: &str,
    ) -> Result<(), GroupAssignmentError> {
        let endpoint = StorageEndpoint::new(self.store.clone());
        let groups = BTreeMap::from([(group.to_owned(), String::new())]);
        let mut statuses = endpoint
            .load_meta_service_group_status(txn, &self.paths, &groups)
            .await?;
        let status = statuses.get_mut(group).expect("requested group");
        if !group.is_empty() && status.assignment_count != 0 {
            status.assignment_count = status.assignment_count.wrapping_sub(1).max(0);
            endpoint.save_meta_service_group_status(txn, &self.paths, group, Some(status))?;
        }
        Ok(())
    }
}

/// Reservation holding directory membership stable until the caller commits.
pub struct GroupReservation<'a> {
    pub group: String,
    _groups: RwLockReadGuard<'a, BTreeMap<String, String>>,
}
