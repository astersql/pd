// Copyright 2026 AsterSQL.
use crate::{CreateKeyspaceRequest, Kernel, KeyspaceError, KeyspaceManager};
use astersql_pd_keyspace_constant::{
    DEFAULT_KEYSPACE_ID, DEFAULT_KEYSPACE_NAME, SYSTEM_KEYSPACE_ID, SYSTEM_KEYSPACE_NAME,
};
use astersql_pd_storage_domain::{Keyspace, KeyspaceIdentity, UserKind};
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;
/// Host-owned work. Bootstrap never detaches private preallocation workers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootstrapWork {
    pub preallocations: Vec<String>,
}
#[derive(Debug)]
pub enum PreallocationOutcome {
    Created(Keyspace),
    AlreadyExists,
}
impl Kernel {
    pub fn bootstrap_identity(self) -> (u32, &'static str) {
        match self {
            Self::Classic => (DEFAULT_KEYSPACE_ID, DEFAULT_KEYSPACE_NAME),
            Self::NextGeneration => (SYSTEM_KEYSPACE_ID, SYSTEM_KEYSPACE_NAME),
        }
    }
}
impl KeyspaceManager {
    /// Re-register boundaries on every bootstrap, preserving existing metadata and timestamps.
    pub async fn bootstrap(&self, now: i64) -> Result<(), KeyspaceError> {
        let (id, name) = self.resources.kernel.bootstrap_identity();
        let bound = self.resources.config.read().unwrap().bound;
        let splitter = self.resources.splitter.as_ref().ok_or_else(|| {
            KeyspaceError::Validation("cluster does not support region label".into())
        })?;
        splitter
            .split(
                &CancellationToken::new(),
                id,
                false,
                bound,
                i128::from(now) * 1_000_000_000,
            )
            .await
            .map_err(|error| KeyspaceError::Split {
                error,
                cleanup_error: None,
            })?;
        let mut config = HashMap::new();
        let group = if let Some(groups) = &self.resources.groups {
            let id = groups.creation_group(UserKind::BASIC).await?;
            config.insert("user_kind".into(), "basic".into());
            config.insert("tso_keyspace_group_id".into(), id.to_string());
            Some(id)
        } else {
            None
        };
        config.insert("region_bound_type".into(), bound.to_string());
        if id == SYSTEM_KEYSPACE_ID {
            config.insert("gc_management_type".into(), "keyspace_level".into());
        }
        let meta = Keyspace {
            identity: Some(KeyspaceIdentity::Id(id)),
            name: name.into(),
            state: 0,
            created_at: now,
            state_changed_at: now,
            config,
        };
        match self.save_new(&meta).await {
            Ok(()) | Err(KeyspaceError::Exists) => {}
            Err(error) => return Err(error),
        }
        if let (Some(groups), Some(group)) = (&self.resources.groups, group) {
            groups
                .add_created_keyspace(UserKind::BASIC, &group.to_string(), id)
                .await?;
        }
        Ok(())
    }
    /// The host schedules these names with its structured concurrency and lifetime.
    pub async fn bootstrap_with_preallocations(
        &self,
        now: i64,
        names: &[String],
    ) -> Result<BootstrapWork, KeyspaceError> {
        self.bootstrap(now).await?;
        Ok(BootstrapWork {
            preallocations: names.to_vec(),
        })
    }
    pub async fn preallocate(
        &self,
        cancel: &CancellationToken,
        name: &str,
        now: i64,
    ) -> Result<PreallocationOutcome, KeyspaceError> {
        let mut config = HashMap::new();
        let initial_group = if let Some(groups) = &self.resources.groups {
            let id = groups.creation_group(UserKind::BASIC).await?;
            config.insert("user_kind".into(), "basic".into());
            config.insert("tso_keyspace_group_id".into(), id.to_string());
            Some(id)
        } else {
            None
        };
        let result = self
            .create(
                cancel,
                CreateKeyspaceRequest {
                    name: name.into(),
                    config,
                    create_time: now,
                },
            )
            .await;
        match result {
            Ok(meta) => {
                if let Some(groups) = &self.resources.groups {
                    groups
                        .add_created_keyspace(
                            UserKind::BASIC,
                            &meta.config["tso_keyspace_group_id"],
                            meta.id(),
                        )
                        .await?;
                }
                Ok(PreallocationOutcome::Created(meta))
            }
            Err(KeyspaceError::Exists) => {
                // Go ignores Exists then uses nil KeyspaceMeta.GetId(), which is zero.
                if let (Some(groups), Some(id)) = (&self.resources.groups, initial_group) {
                    groups
                        .add_created_keyspace(UserKind::BASIC, &id.to_string(), 0)
                        .await?;
                }
                Ok(PreallocationOutcome::AlreadyExists)
            }
            Err(error) => Err(error),
        }
    }
}
