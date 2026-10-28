// Copyright 2026 AsterSQL.
use astersql_pd_storage_domain::*;
use std::{future::Future, sync::Arc};
use tokio_util::sync::CancellationToken;

pub use astersql_pd_election::{ElectionError as MemberError, LeaderWatch, Leadership, Result};

async fn cancellable<T>(
    cancel: &CancellationToken,
    f: impl Future<Output = astersql_pd_storage_domain::Result<T>>,
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
        Leadership::campaign(
            self.store.clone(),
            self.cancel.clone(),
            self.cluster_id,
            self.value.clone(),
            ttl_seconds,
        )
        .await
    }
}
