// Copyright 2026 AsterSQL.
//! Status mutations share assignment locks and the host's transactional store.
use crate::{GroupAssignmentError, MetaServiceGroupManager};
use astersql_pd_storage_domain::{
    Base, CallbackTxnError, EndpointError, MetaServiceGroupStatus, StorageEndpoint,
};
use std::collections::BTreeMap;
impl MetaServiceGroupManager {
    /// Read persisted status while holding the directory stable.
    pub async fn get_status(
        &self,
    ) -> Result<BTreeMap<String, MetaServiceGroupStatus>, CallbackTxnError<EndpointError>> {
        let groups = self.groups.read().await;
        let mut txn = Base::new(self.store.as_ref()).begin_txn();
        let statuses = StorageEndpoint::new(self.store.clone())
            .load_meta_service_group_status(&mut txn, &self.paths, &groups)
            .await
            .map_err(CallbackTxnError::Callback)?;
        txn.commit().await?;
        Ok(statuses)
    }
    /// Patch only the selected group's compare set; null fields retain their value.
    pub async fn patch_status(
        &self,
        id: &str,
        count: Option<i64>,
        enabled: Option<bool>,
    ) -> Result<(), CallbackTxnError<GroupAssignmentError>> {
        if count.is_some_and(|n| n < 0) {
            return Err(CallbackTxnError::Callback(
                GroupAssignmentError::InvalidCount,
            ));
        }
        let groups = self.groups.read().await;
        if !groups.contains_key(id) {
            return Err(CallbackTxnError::Callback(
                GroupAssignmentError::UnknownGroup,
            ));
        }
        let endpoint = StorageEndpoint::new(self.store.clone());
        let mut txn = Base::new(self.store.as_ref()).begin_txn();
        let mut statuses = endpoint
            .load_meta_service_group_status(
                &mut txn,
                &self.paths,
                &BTreeMap::from([(id.to_owned(), String::new())]),
            )
            .await
            .map_err(|e| CallbackTxnError::Callback(GroupAssignmentError::Storage(e)))?;
        let status = statuses.get_mut(id).expect("requested group");
        if let Some(n) = count {
            status.assignment_count = n;
        }
        if let Some(v) = enabled {
            status.enabled = v;
        }
        endpoint
            .save_meta_service_group_status(&mut txn, &self.paths, id, Some(status))
            .map_err(|e| CallbackTxnError::Callback(GroupAssignmentError::Storage(e)))?;
        txn.commit().await
    }
}

/// Authoritative catalog scan; it must not decorate results under the group write lock.
pub struct MetaGroupKeyspaceCounter(pub astersql_pd_storage_domain::KeyspaceStorage);
#[async_trait::async_trait]
impl crate::GroupAssignmentCounter for MetaGroupKeyspaceCounter {
    async fn count(&self, ids: &[String]) -> Result<BTreeMap<String, i64>, String> {
        let mut counts = BTreeMap::new();
        let mut start = 0;
        loop {
            let mut txn = Base::new(self.0.store.as_ref()).begin_txn();
            let metas = self
                .0
                .load_range_keyspace(&mut txn, &self.0.paths, start, 64)
                .await
                .map_err(|e| e.to_string())?;
            txn.commit::<std::convert::Infallible>()
                .await
                .map_err(|e| e.to_string())?;
            for meta in &metas {
                if let Some(id) = meta
                    .config
                    .get("meta_service_group_id")
                    .filter(|id| ids.contains(id))
                {
                    *counts.entry(id.clone()).or_default() += 1;
                }
            }
            if metas.len() < 64 {
                break;
            }
            start = metas.last().expect("full page").id() + 1;
        }
        Ok(counts)
    }
}
