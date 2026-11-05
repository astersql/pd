// Copyright 2026 AsterSQL.
use crate::{GroupAssignmentError, KeyspaceError, KeyspaceManager, MetaServiceGroupManager};
use astersql_pd_storage_domain::{Base, CallbackTxn, Keyspace, StorageEndpoint, UserKind};
use std::{collections::BTreeMap, convert::Infallible};
#[derive(Clone, Copy, Debug)]
pub enum ConfigOp {
    Put,
    Delete,
    Unknown(i32),
}
#[derive(Clone, Debug)]
pub struct ConfigMutation {
    pub op: ConfigOp,
    pub key: String,
    pub value: String,
}
pub(crate) fn state_name(state: i32) -> String {
    match state {
        0 => "ENABLED".into(),
        1 => "DISABLED".into(),
        2 => "ARCHIVED".into(),
        3 => "TOMBSTONE".into(),
        n => n.to_string(),
    }
}
impl KeyspaceManager {
    pub async fn update_config(
        &self,
        name: &str,
        mutations: &[ConfigMutation],
    ) -> Result<Keyspace, KeyspaceError> {
        self.update_config_with_preconditions(name, mutations, &BTreeMap::new())
            .await
    }
    /// Preconditions and metadata/counter changes share one revision-protected transaction.
    /// Go's independent TSO move can survive failure of the subsequent metadata commit.
    pub async fn update_config_with_preconditions(
        &self,
        name: &str,
        mutations: &[ConfigMutation],
        preconditions: &BTreeMap<String, Option<String>>,
    ) -> Result<Keyspace, KeyspaceError> {
        let may_move = mutations
            .iter()
            .any(|m| matches!(m.key.as_str(), "user_kind" | "tso_keyspace_group_id"));
        let _stability = match (&self.resources.groups, may_move) {
            (Some(g), true) => Some(g.membership_gate.write().await),
            _ => None,
        };
        let _membership = match (&self.resources.groups, may_move) {
            (Some(g), true) => Some(g.membership_mutex.lock().await),
            _ => None,
        };
        let directory = match &self.resources.meta_groups {
            Some(m) => Some(m.groups.read().await),
            None => None,
        };
        let _mutation = self.mutation.lock().await;
        let s = &self.storage;
        let mut txn = Base::new(s.store.as_ref()).begin_txn();
        let id = s
            .load_keyspace_id(&mut txn, &s.paths, name)
            .await?
            .ok_or(KeyspaceError::NotFound)?;
        let mut meta = s
            .load_keyspace_meta(&mut txn, &s.paths, id)
            .await?
            .ok_or(KeyspaceError::NotFound)?;
        if !matches!(meta.state, 0 | 1) {
            return Err(KeyspaceError::Validation(format!(
                "cannot change config for keyspace with state {}",
                state_name(meta.state)
            )));
        }
        for (key, expected) in preconditions {
            let actual = meta.config.get(key);
            let failure = match (expected, actual) {
                (None, Some(_)) => Some(format!("{key} must be absent")),
                (Some(_), None) => Some(format!("{key} does not exist")),
                (Some(expected), Some(actual)) if expected != actual => {
                    Some(format!("key={key} expected={expected} actual={actual}"))
                }
                _ => None,
            };
            if let Some(message) = failure {
                return Err(KeyspaceError::ConfigPrecondition(message));
            }
        }
        let old = meta.config.clone();
        for mutation in mutations {
            match mutation.op {
                ConfigOp::Put => {
                    meta.config
                        .insert(mutation.key.clone(), mutation.value.clone());
                }
                ConfigOp::Delete => {
                    meta.config.remove(&mutation.key);
                }
                ConfigOp::Unknown(_) => {
                    return Err(KeyspaceError::Validation(
                        "[PD:keyspace:ErrIllegalOperation]unknown operation".into(),
                    ));
                }
            }
        }
        meta.config.remove(crate::META_SERVICE_GROUP_ADDRESSES_KEY);
        let config = &meta.config;
        let old_meta = old
            .get(crate::META_SERVICE_GROUP_ID_KEY)
            .map(String::as_str)
            .unwrap_or("");
        let new_meta = config
            .get(crate::META_SERVICE_GROUP_ID_KEY)
            .map(String::as_str)
            .unwrap_or("");
        if old_meta != new_meta
            && let (Some(m), Some(directory)) = (&self.resources.meta_groups, directory.as_ref())
        {
            m.reassign_config(&mut txn, directory, old_meta, new_meta)
                .await?;
        }
        let old_kind = UserKind::from_name(old.get("user_kind").map(String::as_str).unwrap_or(""));
        let new_kind =
            UserKind::from_name(config.get("user_kind").map(String::as_str).unwrap_or(""));
        let old_group = old
            .get("tso_keyspace_group_id")
            .map(String::as_str)
            .unwrap_or("");
        let new_group = config
            .get("tso_keyspace_group_id")
            .map(String::as_str)
            .unwrap_or("");
        let need_move = old_kind != new_kind || old_group != new_group;
        if need_move && let Some(g) = &self.resources.groups {
            g.update_keyspace_group_locked(old_group, new_group, old_kind, new_kind, id)
                .await?;
        }
        if let Err(error) = s.save_keyspace_meta(&mut txn, &s.paths, &meta) {
            if need_move && let Some(g) = &self.resources.groups {
                let _ = g
                    .update_keyspace_group_locked(new_group, old_group, new_kind, old_kind, id)
                    .await;
            }
            return Err(error.into());
        }
        // Preserve Go's stage boundary: a commit rejection does not undo the separately committed group move.
        txn.commit::<Infallible>()
            .await
            .map_err(crate::keyspace_create::commit_error)?;
        drop(directory);
        drop(_mutation);
        drop(_membership);
        drop(_stability);
        Ok(self.decorate(meta).await)
    }
}
impl MetaServiceGroupManager {
    async fn reassign_config(
        &self,
        txn: &mut CallbackTxn<'_>,
        directory: &BTreeMap<String, String>,
        old: &str,
        new: &str,
    ) -> Result<(), GroupAssignmentError> {
        let endpoint = StorageEndpoint::new(self.store.clone());
        if !new.is_empty() {
            if !directory.contains_key(new) {
                return Err(GroupAssignmentError::UnknownGroup);
            }
            let statuses = endpoint
                .load_meta_service_group_status(
                    txn,
                    &self.paths,
                    &BTreeMap::from([(new.into(), String::new())]),
                )
                .await?;
            if !statuses[new].enabled {
                return Err(GroupAssignmentError::DisabledGroup);
            }
        }
        if !old.is_empty() {
            self.rollback_in_txn(txn, old).await?;
        }
        if !new.is_empty() {
            let mut statuses = endpoint
                .load_meta_service_group_status(
                    txn,
                    &self.paths,
                    &BTreeMap::from([(new.into(), String::new())]),
                )
                .await?;
            let status = statuses.get_mut(new).unwrap();
            status.assignment_count = status.assignment_count.wrapping_add(1);
            endpoint.save_meta_service_group_status(txn, &self.paths, new, Some(status))?;
        }
        Ok(())
    }
}
