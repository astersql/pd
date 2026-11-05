// Copyright 2026 AsterSQL.

use crate::MetaServiceGroupManager;
use astersql_pd_storage_domain::{Base, CallbackTxnError, EndpointError, StorageEndpoint};
use async_trait::async_trait;
use std::{collections::BTreeMap, future::Future, sync::Arc};

/// Keyspace config entry selecting the meta-service group.
pub const META_SERVICE_GROUP_ID_KEY: &str = "meta_service_group_id";
/// Response-only keyspace config entry containing the current endpoints.
pub const META_SERVICE_GROUP_ADDRESSES_KEY: &str = "meta_service_group_addrs";

/// Host adapter checks a fresh endpoint connection, including current TLS inputs.
#[async_trait]
pub trait GroupEndpointHealth: Send + Sync {
    async fn check(&self, endpoint: &str) -> Result<(), String>;
}

/// Host keyspace catalog supplies authoritative assignments under the directory lock.
/// Implementations must not acquire the manager's group lock recursively.
#[async_trait]
pub trait GroupAssignmentCounter: Send + Sync {
    async fn count(&self, ids: &[String]) -> Result<BTreeMap<String, i64>, String>;
}

/// Configuration, health, delete-guard and persistence failures remain distinct.
#[derive(Debug, thiserror::Error)]
pub enum GroupUpdateError {
    #[error("{0}")]
    InvalidConfiguration(String),
    #[error("meta-service group unhealthy: group {group} endpoint {endpoint}: {message}")]
    Unhealthy {
        group: String,
        endpoint: String,
        message: String,
    },
    #[error("group has assigned keyspaces: {0}")]
    AssignedKeyspaces(String),
    #[error("{0}")]
    Persistence(String),
    #[error(transparent)]
    Storage(#[from] EndpointError),
}

/// Status cleanup is best effort after the new configuration was committed.
#[derive(Debug)]
pub struct GroupUpdateOutcome {
    pub cleanup_error: Option<CallbackTxnError<EndpointError>>,
}

impl MetaServiceGroupManager {
    /// Configure the catalog adapter before sharing this manager with the host.
    pub fn with_assignment_counter(mut self, counter: Arc<dyn GroupAssignmentCounter>) -> Self {
        self.assignment_counter = Some(counter);
        self
    }

    /// Return an owned directory snapshot so callers cannot mutate shared state.
    pub async fn get_groups(&self) -> BTreeMap<String, String> {
        self.groups.read().await.clone()
    }

    /// Decorate an owned config snapshot, retaining the caller's original map.
    pub async fn attach_endpoints(
        &self,
        config: &BTreeMap<String, String>,
    ) -> BTreeMap<String, String> {
        let mut result = config.clone();
        if let Some(id) = config
            .get(META_SERVICE_GROUP_ID_KEY)
            .filter(|id| !id.is_empty())
        {
            if let Some(addresses) = self.groups.read().await.get(id).filter(|s| !s.is_empty()) {
                result.insert(META_SERVICE_GROUP_ADDRESSES_KEY.into(), addresses.clone());
            }
        }
        result
    }

    /// Serialize updates, validate every changed endpoint, guard deletions, then
    /// persist before publication. Assignments remain blocked through publication.
    pub async fn update_groups_safely<F, Fut, A>(
        &self,
        candidate: BTreeMap<String, String>,
        deleted: &[String],
        health: &dyn GroupEndpointHealth,
        persist: F,
        after_persist: A,
    ) -> Result<GroupUpdateOutcome, GroupUpdateError>
    where
        F: FnOnce(BTreeMap<String, String>) -> Fut + Send,
        Fut: Future<Output = Result<(), String>> + Send,
        A: FnOnce() + Send,
    {
        let _update = self.update_mutex.lock().await;
        let candidate = normalize_meta_service_groups(candidate)?;
        let previous = self.get_groups().await;
        for (id, addresses) in &candidate {
            if previous.get(id) != Some(addresses) {
                for endpoint in addresses.split(',').map(str::trim) {
                    health.check(endpoint).await.map_err(|message| {
                        GroupUpdateError::Unhealthy {
                            group: id.clone(),
                            endpoint: endpoint.into(),
                            message,
                        }
                    })?;
                }
            }
        }
        let mut groups = self.groups.write().await;
        if !deleted.is_empty() {
            let counts = if let Some(counter) = &self.assignment_counter {
                counter
                    .count(deleted)
                    .await
                    .map_err(GroupUpdateError::Persistence)?
            } else {
                let endpoint = StorageEndpoint::new(self.store.clone());
                let mut txn = Base::new(self.store.as_ref()).begin_txn();
                let statuses = endpoint
                    .load_meta_service_group_status(&mut txn, &self.paths, &groups)
                    .await?;
                txn.commit::<EndpointError>()
                    .await
                    .map_err(|e| GroupUpdateError::Persistence(e.to_string()))?;
                statuses
                    .into_iter()
                    .map(|(id, status)| (id, status.assignment_count))
                    .collect()
            };
            for id in deleted {
                if counts.get(id).copied().unwrap_or_default() > 0 {
                    return Err(GroupUpdateError::AssignedKeyspaces(id.clone()));
                }
            }
        }
        persist(candidate.clone())
            .await
            .map_err(GroupUpdateError::Persistence)?;
        *groups = candidate;
        let endpoint = StorageEndpoint::new(self.store.clone());
        let mut txn = Base::new(self.store.as_ref()).begin_txn();
        for id in deleted {
            endpoint.remove_meta_service_group_status(&mut txn, &self.paths, id)?;
        }
        let cleanup_error = if deleted.is_empty() {
            None
        } else {
            txn.commit().await.err()
        };
        drop(groups);
        after_persist();
        Ok(GroupUpdateOutcome { cleanup_error })
    }
}

/// Normalize a candidate directory without changing its caller-owned input.
pub fn normalize_meta_service_groups(
    groups: BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, GroupUpdateError> {
    let mut normalized = BTreeMap::new();
    for (id, addresses) in groups {
        let id = id.trim();
        let addresses = addresses.trim();
        let message = if id.is_empty() {
            Some("[keyspace] meta-service group ID cannot be empty".into())
        } else if id.contains('/') {
            Some(format!(
                "[keyspace] meta-service group ID cannot contain '/': {id}"
            ))
        } else if addresses.is_empty() {
            Some("[keyspace] meta-service group addresses cannot be empty".into())
        } else if normalized.contains_key(id) {
            Some(format!(
                "[keyspace] meta-service group ID cannot be duplicated: {id}"
            ))
        } else {
            None
        };
        if let Some(message) = message {
            return Err(GroupUpdateError::InvalidConfiguration(message));
        }
        normalized.insert(id.to_owned(), addresses.to_owned());
    }
    Ok(normalized)
}
