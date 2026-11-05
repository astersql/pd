// Copyright 2026 AsterSQL.
use crate::{Kernel, KeyspaceError, KeyspaceManager};
use astersql_pd_storage_domain::{Base, Keyspace};
use std::convert::Infallible;
/// Idempotence accepts unknown enum numbers too, matching Go's equality check before the table.
pub fn keyspace_state_transition_allowed(from: i32, to: i32) -> bool {
    from == to || matches!((from, to), (0, 1) | (1, 0) | (1, 2) | (2, 3))
}
pub(crate) fn protected_state_error(kernel: Kernel) -> KeyspaceError {
    KeyspaceError::Validation(
        match kernel {
            Kernel::Classic => {
                "[PD:keyspace:ErrModifyDefaultKeyspace]cannot modify default keyspace's state"
            }
            Kernel::NextGeneration => {
                "[PD:keyspace:ErrModifyReservedKeyspace]cannot modify reserved keyspace's state"
            }
        }
        .into(),
    )
}
enum StateTarget<'a> {
    Id(u32),
    Name(&'a str),
}
impl KeyspaceManager {
    pub async fn update_state_by_id(
        &self,
        id: u32,
        state: i32,
        now: i64,
    ) -> Result<Keyspace, KeyspaceError> {
        if self.resources.kernel.is_protected_id(id) {
            return Err(protected_state_error(self.resources.kernel));
        }
        self.update_state(StateTarget::Id(id), state, now).await
    }
    pub async fn update_state_by_name(
        &self,
        name: &str,
        state: i32,
        now: i64,
    ) -> Result<Keyspace, KeyspaceError> {
        if self.resources.kernel.is_protected_name(name) {
            return Err(protected_state_error(self.resources.kernel));
        }
        self.update_state(StateTarget::Name(name), state, now).await
    }
    async fn update_state(
        &self,
        target: StateTarget<'_>,
        state: i32,
        now: i64,
    ) -> Result<Keyspace, KeyspaceError> {
        let s = &self.storage;
        let mut txn = Base::new(s.store.as_ref()).begin_txn();
        let id = match target {
            StateTarget::Id(id) => id,
            StateTarget::Name(name) => s
                .load_keyspace_id(&mut txn, &s.paths, name)
                .await?
                .ok_or(KeyspaceError::NotFound)?,
        };
        let guard = self.mutation.lock().await;
        let mut meta = s
            .load_keyspace_meta(&mut txn, &s.paths, id)
            .await?
            .ok_or(KeyspaceError::NotFound)?;
        if !keyspace_state_transition_allowed(meta.state, state) {
            return Err(KeyspaceError::Validation(format!(
                "cannot change keyspace state from {} to {}",
                crate::keyspace_config::state_name(meta.state),
                crate::keyspace_config::state_name(state)
            )));
        }
        if state == 3
            && let Some(group) = meta
                .config
                .get(crate::META_SERVICE_GROUP_ID_KEY)
                .filter(|id| !id.is_empty())
                .cloned()
        {
            meta.config.remove(crate::META_SERVICE_GROUP_ID_KEY);
            // Do not take the directory lock while holding metadata: config updates take them in reverse order.
            if let Some(manager) = &self.resources.meta_groups {
                manager.rollback_in_txn(&mut txn, &group).await?;
            }
        }
        if meta.state != state {
            meta.state = state;
            meta.state_changed_at = now;
            // Preserve Go's cache publication point even when the subsequent commit is rejected.
            self.publish(&meta);
        }
        s.save_keyspace_meta(&mut txn, &s.paths, &meta)?;
        txn.commit::<Infallible>()
            .await
            .map_err(crate::keyspace_create::commit_error)?;
        drop(guard);
        Ok(self.decorate(meta).await)
    }
}
