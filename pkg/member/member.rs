// Copyright 2026 AsterSQL.
use astersql_pd_storage::*;
use std::{future::Future, sync::Arc};
use tokio_util::sync::CancellationToken;

#[derive(Debug, thiserror::Error)]
pub enum MemberError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("operation cancelled")]
    Cancelled,
    #[error("member identity conflicts with persisted identity")]
    IdentityConflict,
    #[error("leadership is held by another term")]
    Contended,
    #[error("leadership lost")]
    NotLeader,
    #[error("invalid member request")]
    InvalidArgument,
}
pub type Result<T> = std::result::Result<T, MemberError>;

async fn cancellable<T>(
    cancel: &CancellationToken,
    f: impl Future<Output = astersql_pd_storage::Result<T>>,
) -> Result<T> {
    tokio::select! { biased; _ = cancel.cancelled() => Err(MemberError::Cancelled), result = f => Ok(result?) }
}

/// Persistent PD identity, independent of etcd cluster membership.
/// `value` is adapter-encoded identity (Go interoperability requires pdpb.Member wire bytes).
pub struct Member {
    store: Arc<dyn MetaStore>,
    cancel: CancellationToken,
    cluster_id: u64,
    value: Vec<u8>,
}
impl Member {
    /// Idempotently register immutable member identity. Registration survives lease loss.
    pub async fn bootstrap(
        store: Arc<dyn MetaStore>,
        cancel: CancellationToken,
        cluster_id: u64,
        member_id: u64,
        value: Vec<u8>,
    ) -> Result<Self> {
        if cluster_id == 0 || member_id == 0 || value.is_empty() {
            return Err(MemberError::InvalidArgument);
        }
        let key = format!("/pd/{cluster_id}/rust/members/{member_id}").into_bytes();
        let commit = cancellable(
            &cancel,
            store.transact(
                vec![Compare::CreateRevision(key.clone(), 0)],
                vec![Mutation::put(&key, &value)],
            ),
        )
        .await?;
        if !commit.succeeded {
            let snapshot = cancellable(
                &cancel,
                store.read(Read::prefix(&key, Consistency::Linearizable)),
            )
            .await?;
            if !snapshot
                .entries
                .iter()
                .any(|e| e.key == key && e.value == value)
            {
                return Err(MemberError::IdentityConflict);
            }
        }
        Ok(Self {
            store,
            cancel,
            cluster_id,
            value,
        })
    }
    /// Attempt one election. No private runtime or detached keepalive task is created.
    pub async fn campaign(&self, ttl_seconds: i64) -> Result<Leadership> {
        let lease = cancellable(&self.cancel, self.store.grant(ttl_seconds)).await?;
        let key = format!("/pd/{}/leader", self.cluster_id).into_bytes();
        let result = cancellable(
            &self.cancel,
            self.store.transact(
                vec![Compare::CreateRevision(key.clone(), 0)],
                vec![Mutation::Put {
                    key: key.clone(),
                    value: self.value.clone(),
                    lease: Some(lease.id),
                }],
            ),
        )
        .await;
        match result {
            Ok(commit) if commit.succeeded => Ok(Leadership {
                store: self.store.clone(),
                cancel: self.cancel.clone(),
                cluster_id: self.cluster_id,
                key,
                value: self.value.clone(),
                lease: lease.id,
                revision: commit.revision,
            }),
            other => {
                // Revoke our own lease even for an uncertain commit; TTL bounds cleanup if unavailable.
                let _ = self.store.revoke(lease.id).await;
                match other {
                    Err(e) => Err(e),
                    _ => Err(MemberError::Contended),
                }
            }
        }
    }
}

/// A single immutable election term. Value, lease and revision fence ABA/re-election.
pub struct Leadership {
    store: Arc<dyn MetaStore>,
    cancel: CancellationToken,
    cluster_id: u64,
    key: Vec<u8>,
    value: Vec<u8>,
    lease: LeaseId,
    revision: Revision,
}
impl Leadership {
    pub fn lease_id(&self) -> LeaseId {
        self.lease
    }
    pub fn revision(&self) -> Revision {
        self.revision
    }
    pub fn cluster_id(&self) -> u64 {
        self.cluster_id
    }
    pub fn uses_store(&self, store: &Arc<dyn MetaStore>) -> bool {
        Arc::ptr_eq(&self.store, store)
    }
    fn comparisons(&self) -> Vec<Compare> {
        vec![
            Compare::Value(self.key.clone(), self.value.clone()),
            Compare::Lease(self.key.clone(), self.lease),
            Compare::ModRevision(self.key.clone(), self.revision),
        ]
    }
    /// Linearizable validation, including cached-window allocations.
    pub async fn validate(&self) -> Result<()> {
        let commit = cancellable(
            &self.cancel,
            self.store.transact(self.comparisons(), vec![]),
        )
        .await?;
        if commit.succeeded {
            Ok(())
        } else {
            Err(MemberError::NotLeader)
        }
    }
    /// Atomically fence application writes. Compare conflicts are returned to the caller.
    pub async fn transact(
        &self,
        mut compares: Vec<Compare>,
        writes: Vec<Mutation>,
    ) -> Result<Commit> {
        if writes.iter().any(|w| match w {
            Mutation::Put { key, .. } | Mutation::Delete { key } => key == &self.key,
        }) {
            return Err(MemberError::InvalidArgument);
        }
        compares.extend(self.comparisons());
        let commit = cancellable(&self.cancel, self.store.transact(compares, writes)).await?;
        if !commit.succeeded {
            self.validate().await?;
        }
        Ok(commit)
    }
    /// Caller schedules explicit renewals on its shared runtime.
    pub async fn renew(&self) -> Result<()> {
        self.validate().await?;
        match cancellable(&self.cancel, self.store.keep_alive(self.lease)).await {
            Err(MemberError::Store(StoreError::LeaseNotFound)) => Err(MemberError::NotLeader),
            result => {
                result?;
                self.validate().await
            }
        }
    }
    /// Cleanup remains possible after cancellation. Only this term's lease is revoked.
    pub async fn resign(&self) -> Result<()> {
        match self.store.revoke(self.lease).await {
            Ok(()) | Err(StoreError::LeaseNotFound) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
