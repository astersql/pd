// Copyright 2026 AsterSQL.
use astersql_pd_storage_kv::{Lease, LeaseId, MetaStore, Mutation, StoreError};

/// Failure to grant or write, retaining the lease for recovery if cleanup fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TtlPutError {
    pub cause: StoreError,
    pub lease: Option<LeaseId>,
    pub cleanup_error: Option<StoreError>,
}
impl std::fmt::Display for TtlPutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TTL put failed: {}", self.cause)?;
        if let Some(error) = &self.cleanup_error {
            write!(f, "; lease {:?} cleanup failed: {error}", self.lease)?;
        }
        Ok(())
    }
}
impl std::error::Error for TtlPutError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

/// Grant a dedicated lease and write raw bytes once. Use MetaStore::keep_alive
/// or MetaStore::revoke on the returned lease; no renewal worker is started.
///
/// A failed write is compensated by revoking its dedicated lease, including when
/// the commit response was lost. The write is never retried. A failed revocation
/// is reported with the lease ID for host recovery; its TTL still bounds lifetime.
/// Await this future to completion: dropping it cannot perform async cleanup.
pub async fn put_with_ttl(
    store: &dyn MetaStore,
    key: &[u8],
    value: &[u8],
    ttl_seconds: i64,
) -> std::result::Result<Lease, TtlPutError> {
    let lease = store
        .grant(ttl_seconds)
        .await
        .map_err(|cause| TtlPutError {
            cause,
            lease: None,
            cleanup_error: None,
        })?;
    match store
        .transact(
            vec![],
            vec![Mutation::Put {
                key: key.to_vec(),
                value: value.to_vec(),
                lease: Some(lease.id),
            }],
        )
        .await
    {
        Ok(_) => Ok(lease),
        Err(cause) => {
            let cleanup_error = match store.revoke(lease.id).await {
                Ok(()) | Err(StoreError::LeaseNotFound) => None,
                Err(error) => Some(error),
            };
            Err(TtlPutError {
                cause,
                lease: Some(lease.id),
                cleanup_error,
            })
        }
    }
}
