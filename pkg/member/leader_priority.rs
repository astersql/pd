// Copyright 2026 AsterSQL.
use crate::Leadership;
use astersql_pd_election::ElectionError;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{Compare, Consistency, MetaStore, Mutation, Read, StoreError};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Storage, decimal parsing, and fenced transaction failures.
#[derive(Debug, thiserror::Error)]
pub enum LeaderPriorityError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("operation cancelled")]
    Cancelled,
    #[error("invalid signed 32-bit leader priority")]
    InvalidPriority,
    #[error("leader transaction conflict")]
    Conflict,
    #[error("leadership belongs to another store or cluster")]
    InvalidTerm,
    #[error(transparent)]
    Election(#[from] ElectionError),
}

/// Independent member priorities; reads need no leadership and values have no lease.
/// Writes reuse the canonical election term, including its ABA protection.
pub struct LeaderPriority {
    store: Arc<dyn MetaStore>,
    cancel: CancellationToken,
    paths: ClusterPaths,
}
impl LeaderPriority {
    /// Use the host store, cluster context and cancellation token.
    pub fn new(store: Arc<dyn MetaStore>, cancel: CancellationToken, cluster_id: u64) -> Self {
        Self {
            store,
            cancel,
            paths: ClusterPaths::new(cluster_id),
        }
    }
    /// Return zero only for an absent key; parse present values as signed decimal i32.
    pub async fn get_member_leader_priority(&self, id: u64) -> Result<i32, LeaderPriorityError> {
        let key = self.paths.member_leader_priority_path(id);
        let snapshot = tokio::select! { biased;
            _ = self.cancel.cancelled() => return Err(LeaderPriorityError::Cancelled),
            result = self.store.read(Read::range(key.as_bytes(), b"", Consistency::Linearizable)) => result?,
        };
        let Some(entry) = snapshot.entries.first() else {
            return Ok(0);
        };
        std::str::from_utf8(&entry.value)
            .ok()
            .and_then(|s| s.parse::<i32>().ok())
            .ok_or(LeaderPriorityError::InvalidPriority)
    }
    async fn write(
        &self,
        term: &Leadership,
        mutation: Mutation,
        compares: Vec<Compare>,
    ) -> Result<(), LeaderPriorityError> {
        if !term.uses_store(&self.store) || term.cluster_id() != self.paths.cluster_id() {
            return Err(LeaderPriorityError::InvalidTerm);
        }
        let result = tokio::select! { biased;
            _ = self.cancel.cancelled() => return Err(LeaderPriorityError::Cancelled),
            result = term.transact(compares, vec![mutation]) => result,
        };
        match result {
            Ok(commit) if commit.succeeded => Ok(()),
            Ok(_) | Err(ElectionError::NotLeader) => Err(LeaderPriorityError::Conflict),
            Err(ElectionError::Store(e)) => Err(LeaderPriorityError::Store(e)),
            Err(ElectionError::Cancelled) => Err(LeaderPriorityError::Cancelled),
            Err(e) => Err(e.into()),
        }
    }
    /// Persist decimal text atomically under leadership fencing.
    /// Like Go on 64-bit hosts, writes accept i64 even when a subsequent i32 read fails.
    pub async fn set_member_leader_priority(
        &self,
        term: &Leadership,
        id: u64,
        priority: i64,
    ) -> Result<(), LeaderPriorityError> {
        self.write(
            term,
            Mutation::put(
                self.paths.member_leader_priority_path(id).as_bytes(),
                priority.to_string().as_bytes(),
            ),
            vec![],
        )
        .await
    }
    /// Update only a registered, non-revoked PD member. The membership checks
    /// share the fenced transaction, preventing stale HTTP lookup/write races.
    pub async fn set_registered_member_leader_priority(
        &self,
        term: &Leadership,
        id: u64,
        priority: i64,
    ) -> Result<(), LeaderPriorityError> {
        let cluster = self.paths.cluster_id();
        self.write(
            term,
            Mutation::put(
                self.paths.member_leader_priority_path(id).as_bytes(),
                priority.to_string().as_bytes(),
            ),
            vec![
                Compare::Exists(format!("/pd/{cluster}/rust/members/{id}").into_bytes()),
                Compare::CreateRevision(
                    format!("/pd/{cluster}/rust/removed-members/{id}").into_bytes(),
                    0,
                ),
            ],
        )
        .await
    }
    /// Delete under leadership fencing, including when the key is already absent.
    pub async fn delete_member_leader_priority(
        &self,
        term: &Leadership,
        id: u64,
    ) -> Result<(), LeaderPriorityError> {
        self.write(
            term,
            Mutation::Delete {
                key: self.paths.member_leader_priority_path(id).into_bytes(),
            },
            vec![],
        )
        .await
    }
}
