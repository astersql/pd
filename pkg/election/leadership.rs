// Copyright 2026 AsterSQL.
use crate::{LeaseClock, LeaseExpiry, MonotonicLeaseClock};
use astersql_pd_storage_kv::*;
use std::{future::Future, sync::Arc};
use tokio_util::sync::CancellationToken;

#[derive(Debug, thiserror::Error)]
pub enum ElectionError {
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
pub type Result<T> = std::result::Result<T, ElectionError>;

async fn cancellable<T>(
    cancel: &CancellationToken,
    f: impl Future<Output = astersql_pd_storage_kv::Result<T>>,
) -> Result<T> {
    tokio::select! { biased; _ = cancel.cancelled() => Err(ElectionError::Cancelled), result = f => Ok(result?) }
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
    expiry: LeaseExpiry,
}
impl Leadership {
    /// Attempt one election using the existing PD leader path and opaque value.
    /// The caller owns renewal scheduling and cancellation.
    pub async fn campaign(
        store: Arc<dyn MetaStore>,
        cancel: CancellationToken,
        cluster_id: u64,
        value: Vec<u8>,
        ttl_seconds: i64,
    ) -> Result<Self> {
        Self::campaign_with_key(
            store,
            cancel,
            cluster_id,
            format!("/pd/{cluster_id}/leader").into_bytes(),
            value,
            ttl_seconds,
            vec![],
        )
        .await
    }

    /// Attempt election at an explicit key, subject to additional atomic comparisons.
    /// A failed attempt revokes only its own lease, leaving existing leaders untouched.
    pub async fn campaign_with_key(
        store: Arc<dyn MetaStore>,
        cancel: CancellationToken,
        cluster_id: u64,
        key: Vec<u8>,
        value: Vec<u8>,
        ttl_seconds: i64,
        compares: Vec<Compare>,
    ) -> Result<Self> {
        Self::campaign_with_key_and_clock(
            store,
            cancel,
            cluster_id,
            key,
            value,
            ttl_seconds,
            compares,
            Arc::new(MonotonicLeaseClock),
        )
        .await
    }

    /// Campaign with an injected monotonic clock for local expiry accounting.
    pub async fn campaign_with_key_and_clock(
        store: Arc<dyn MetaStore>,
        cancel: CancellationToken,
        cluster_id: u64,
        key: Vec<u8>,
        value: Vec<u8>,
        ttl_seconds: i64,
        mut compares: Vec<Compare>,
        clock: Arc<dyn LeaseClock>,
    ) -> Result<Self> {
        let expiry = LeaseExpiry::new(clock);
        let start = expiry.request_start();
        let lease = cancellable(&cancel, store.grant(ttl_seconds)).await?;
        expiry.granted(start, lease.ttl_seconds);
        compares.push(Compare::CreateRevision(key.clone(), 0));
        let result = cancellable(
            &cancel,
            store.transact(
                compares,
                vec![Mutation::Put {
                    key: key.clone(),
                    value: value.clone(),
                    lease: Some(lease.id),
                }],
            ),
        )
        .await;
        match result {
            Ok(commit) if commit.succeeded => Ok(Leadership {
                store: store.clone(),
                cancel: cancel.clone(),
                cluster_id,
                key,
                value: value.clone(),
                lease: lease.id,
                revision: commit.revision,
                expiry,
            }),
            other => {
                // Revoke our own lease even for an uncertain commit; TTL bounds cleanup if unavailable.
                let _ = store.revoke(lease.id).await;
                match other {
                    Err(e) => Err(e),
                    _ => Err(ElectionError::Contended),
                }
            }
        }
    }

    /// Local safety check; backend ownership is checked separately by validate.
    pub fn is_expired(&self) -> bool {
        self.expiry.is_expired()
    }

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
            Err(ElectionError::NotLeader)
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
            return Err(ElectionError::InvalidArgument);
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
        let start = self.expiry.request_start();
        match cancellable(&self.cancel, self.store.keep_alive(self.lease)).await {
            Err(ElectionError::Store(StoreError::LeaseNotFound)) => Err(ElectionError::NotLeader),
            result => {
                let lease = result?;
                self.expiry.renewed(start, lease.ttl_seconds);
                self.validate().await
            }
        }
    }
    /// Cleanup remains possible after cancellation. Only this term's lease is revoked.
    pub async fn resign(&self) -> Result<()> {
        self.expiry.close();
        match self.store.revoke(self.lease).await {
            Ok(()) | Err(StoreError::LeaseNotFound) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
