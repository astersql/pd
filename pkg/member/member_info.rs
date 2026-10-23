// Copyright 2026 AsterSQL.
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Consistency, MetaStore, Mutation, Read, StoreError};
use std::{future::Future, sync::Arc};
use tokio_util::sync::CancellationToken;

/// Errors from the independent member publication keys.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MemberInfoError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("operation cancelled")]
    Cancelled,
    #[error("no value")]
    NoValue,
    #[error("failed to save {0}")]
    SaveFailed(&'static str),
}

/// Publication metadata, separate from immutable identity and election leases.
/// Values are bytes because Go strings can contain arbitrary bytes.
pub struct MemberInfo {
    store: Arc<dyn MetaStore>,
    cancel: CancellationToken,
    paths: ClusterPaths,
}
impl MemberInfo {
    pub fn new(store: Arc<dyn MetaStore>, cancel: CancellationToken, cluster_id: u64) -> Self {
        Self {
            store,
            cancel,
            paths: ClusterPaths::new(cluster_id),
        }
    }
    async fn run<T>(
        &self,
        operation: impl Future<Output = Result<T, StoreError>>,
    ) -> Result<T, MemberInfoError> {
        tokio::select! { biased; _ = self.cancel.cancelled() => Err(MemberInfoError::Cancelled), result = operation => Ok(result?) }
    }
    async fn load(&self, key: String) -> Result<Vec<u8>, MemberInfoError> {
        let snapshot = self
            .run(
                self.store
                    .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable)),
            )
            .await?;
        snapshot
            .entries
            .into_iter()
            .next()
            .map(|entry| entry.value)
            .ok_or(MemberInfoError::NoValue)
    }
    async fn save(
        &self,
        key: String,
        value: &[u8],
        field: &'static str,
    ) -> Result<(), MemberInfoError> {
        let commit = self
            .run(
                self.store
                    .transact(vec![], vec![Mutation::put(key.as_bytes(), value)]),
            )
            .await?;
        if !commit.succeeded {
            return Err(MemberInfoError::SaveFailed(field));
        }
        Ok(())
    }
    /// Load deploy_path; an absent key is an error, while stored empty bytes are valid.
    pub async fn get_member_deploy_path(&self, id: u64) -> Result<Vec<u8>, MemberInfoError> {
        self.load(self.paths.member_binary_deploy_path(id)).await
    }
    /// Publish a deployment directory resolved by the host from its executable path.
    /// The host must propagate executable lookup failures before calling this method.
    pub async fn set_member_deploy_path(
        &self,
        id: u64,
        directory: &[u8],
    ) -> Result<(), MemberInfoError> {
        self.save(
            self.paths.member_binary_deploy_path(id),
            directory,
            "deploy path",
        )
        .await
    }
    /// Load binary_version without parsing or normalizing it.
    pub async fn get_member_binary_version(&self, id: u64) -> Result<Vec<u8>, MemberInfoError> {
        self.load(self.paths.member_binary_version_path(id)).await
    }
    /// Publish binary_version without requiring leadership.
    pub async fn set_member_binary_version(
        &self,
        id: u64,
        version: &[u8],
    ) -> Result<(), MemberInfoError> {
        self.save(
            self.paths.member_binary_version_path(id),
            version,
            "binary version",
        )
        .await
    }
    /// Load git_hash without parsing or normalizing it.
    pub async fn get_member_git_hash(&self, id: u64) -> Result<Vec<u8>, MemberInfoError> {
        self.load(self.paths.member_git_hash_path(id)).await
    }
    /// Publish git_hash without requiring leadership.
    pub async fn set_member_git_hash(&self, id: u64, hash: &[u8]) -> Result<(), MemberInfoError> {
        self.save(self.paths.member_git_hash_path(id), hash, "git hash")
            .await
    }
}
