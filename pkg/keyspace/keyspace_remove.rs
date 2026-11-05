// Copyright 2026 AsterSQL.
use crate::{KeyspaceError, KeyspaceManager};
use astersql_pd_storage_domain::{Base, CallbackTxn};
use std::{convert::Infallible, sync::Arc};
use tokio::sync::MutexGuard;
/// Host lifecycle observer. Runs after a successful removal commit and outside metadata locks.
/// It may invalidate local cache/metrics, but must not issue storage IO.
pub type GCBarrierInvalidator = Arc<dyn Fn(u32) + Send + Sync>;
/// An actual buffered transaction, discarded without publication when dropped or rejected.
pub struct PreparedKeyspaceRemoval<'a> {
    manager: &'a KeyspaceManager,
    txn: CallbackTxn<'a>,
    guard: MutexGuard<'a, ()>,
    id: u32,
}
impl PreparedKeyspaceRemoval<'_> {
    pub fn id(&self) -> u32 {
        self.id
    }
    pub async fn commit(self) -> Result<(), KeyspaceError> {
        let Self {
            manager,
            txn,
            guard,
            id,
        } = self;
        txn.commit::<Infallible>()
            .await
            .map_err(crate::keyspace_create::commit_error)?;
        drop(guard);
        manager.publish_removed(&[id]);
        Ok(())
    }
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RemovalEligibility {
    Skipped,
    Deferred { meta_service_group: String },
    Staged { meta_service_group: String },
}
impl KeyspaceManager {
    pub fn set_gc_barrier_invalidator(&self, observer: Option<GCBarrierInvalidator>) {
        *self.removal_observer.write().unwrap() = observer;
    }
    pub async fn remove(&self, id: u32) -> Result<(), KeyspaceError> {
        self.prepare_remove(id).await?.commit().await
    }
    /// Holds metadata publication until the caller commits the real backend transaction.
    pub async fn prepare_remove(
        &self,
        id: u32,
    ) -> Result<PreparedKeyspaceRemoval<'_>, KeyspaceError> {
        let guard = self.mutation.lock().await;
        let mut txn = Base::new(self.storage.store.as_ref()).begin_txn();
        let result = self
            .stage_remove_if_eligible(&mut txn, id, false, None)
            .await?;
        let RemovalEligibility::Staged { meta_service_group } = result else {
            unreachable!("strict removal cannot be skipped")
        };
        if !meta_service_group.is_empty()
            && let Some(manager) = &self.resources.meta_groups
        {
            manager
                .rollback_in_txn(&mut txn, &meta_service_group)
                .await?;
        }
        Ok(PreparedKeyspaceRemoval {
            manager: self,
            txn,
            guard,
            id,
        })
    }
    /// Caller owns the metadata lock and transaction. No cache/observer effects are published here.
    pub(crate) async fn stage_remove_if_eligible(
        &self,
        txn: &mut CallbackTxn<'_>,
        id: u32,
        skip_non_removable: bool,
        can_remove: Option<&(dyn Fn(&str) -> bool + Sync)>,
    ) -> Result<RemovalEligibility, KeyspaceError> {
        if self.resources.kernel.is_protected_id(id) {
            return if skip_non_removable {
                Ok(RemovalEligibility::Skipped)
            } else {
                Err(crate::keyspace_state::protected_state_error(
                    self.resources.kernel,
                ))
            };
        }
        let meta = self
            .storage
            .load_keyspace_meta(txn, &self.storage.paths, id)
            .await?;
        let Some(meta) = meta else {
            return if skip_non_removable {
                Ok(RemovalEligibility::Skipped)
            } else {
                Err(KeyspaceError::NotFound)
            };
        };
        if skip_non_removable {
            if !matches!(meta.state, 2 | 3) {
                return Ok(RemovalEligibility::Skipped);
            }
        } else if matches!(meta.state, 0 | 1) {
            return Err(KeyspaceError::Validation(format!(
                "cannot remove keyspace in state {}",
                crate::keyspace_config::state_name(meta.state)
            )));
        }
        let group = meta
            .config
            .get(crate::META_SERVICE_GROUP_ID_KEY)
            .cloned()
            .unwrap_or_default();
        if can_remove.is_some_and(|predicate| !predicate(&group)) {
            return Ok(RemovalEligibility::Deferred {
                meta_service_group: group,
            });
        }
        self.storage
            .remove_keyspace(txn, &self.storage.paths, id, &meta.name)?;
        Ok(RemovalEligibility::Staged {
            meta_service_group: group,
        })
    }
    pub(crate) fn evict_cached(&self, ids: &[u32]) {
        for &id in ids {
            self.cache.delete_keyspace(id);
        }
    }
    pub(crate) fn publish_removed(&self, ids: &[u32]) {
        self.evict_cached(ids);
        let observer = self.removal_observer.read().unwrap().clone();
        if let Some(observer) = observer {
            for &id in ids {
                observer(id);
            }
        }
    }
}
