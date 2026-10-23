// Copyright 2026 AsterSQL.

use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
use astersql_pd_storage_kv::{
    Base, Mutation, RawTxn, RawTxnCmpType, RawTxnCondition, RawTxnOp, StoreError,
};

use crate::StorageEndpoint;

/// Buffered GC writes; no writes reach storage until the callback succeeds.
pub struct GCStateWriteBatch<'a> {
    pub(super) paths: &'a ClusterPaths,
    pub(super) ops: Vec<RawTxnOp>,
}

impl<'a> GCStateWriteBatch<'a> {
    pub(crate) fn new(paths: &'a ClusterPaths) -> Self {
        Self {
            paths,
            ops: Vec::new(),
        }
    }

    /// Sets the legacy hexadecimal unified point or the keyspace JSON point.
    pub fn set_gc_safe_point(&mut self, keyspace_id: u32, safe_point: u64) {
        let value = encode_gc_safe_point(keyspace_id, safe_point);
        self.ops.push(
            Mutation::put(
                self.paths.gc_safe_point_path(keyspace_id).as_bytes(),
                value.as_bytes(),
            )
            .into(),
        );
    }

    /// Sets a decimal transaction safe point, including zero and decreasing values.
    pub fn set_txn_safe_point(&mut self, keyspace_id: u32, safe_point: u64) {
        self.ops.push(
            Mutation::put(
                self.paths.txn_safe_point_path(keyspace_id).as_bytes(),
                safe_point.to_string().as_bytes(),
            )
            .into(),
        );
    }
}

// Numeric fields are infallible and need no JSON string escaping.
pub(crate) fn encode_gc_safe_point(keyspace_id: u32, safe_point: u64) -> String {
    if keyspace_id == NULL_KEYSPACE_ID {
        format!("{safe_point:x}")
    } else {
        format!(r#"{{"keyspace_id":{keyspace_id},"safe_point":{safe_point}}}"#)
    }
}

/// Callback, revision syntax, optimistic conflict and backend errors stay distinct.
#[derive(Debug, thiserror::Error)]
pub enum GCStateTxnError<E> {
    #[error("callback failed: {0}")]
    Callback(E),
    #[error("parse uint error")]
    ParseRevision,
    #[error("etcd transaction conflict")]
    Conflict,
    #[error("unexpected number of results: {expected} != {actual}")]
    ResponseCount { expected: usize, actual: usize },
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl StorageEndpoint {
    /// Runs one optimistic GC transaction, without retrying the callback. Reads in
    /// the callback use this endpoint normally; a revision race rolls back the batch.
    pub async fn run_in_gc_state_transaction<E, F>(
        &self,
        paths: &ClusterPaths,
        callback: F,
    ) -> Result<(), GCStateTxnError<E>>
    where
        F: AsyncFnOnce(&mut GCStateWriteBatch<'_>) -> Result<(), E>,
    {
        let key = paths.gc_state_revision_path().into_bytes();
        let current = Base::new(self.store.as_ref()).load(&key).await?;
        let revision = if current.is_empty() {
            0u64
        } else {
            if !current.iter().all(u8::is_ascii_digit) {
                return Err(GCStateTxnError::ParseRevision);
            }
            std::str::from_utf8(&current)
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .ok_or(GCStateTxnError::ParseRevision)?
        };
        let mut batch = GCStateWriteBatch::new(paths);
        callback(&mut batch)
            .await
            .map_err(GCStateTxnError::Callback)?;
        if !batch.ops.is_empty() {
            batch
                .ops
                .push(Mutation::put(&key, revision.wrapping_add(1).to_string().as_bytes()).into());
        }
        let count = batch.ops.len();
        let result = RawTxn::new(self.store.as_ref())
            .when(vec![RawTxnCondition {
                key,
                cmp_type: if current.is_empty() {
                    RawTxnCmpType::NotExists
                } else {
                    RawTxnCmpType::Equal
                },
                value: current,
            }])
            .then_ops(batch.ops)
            .commit()
            .await?;
        if !result.succeeded {
            return Err(GCStateTxnError::Conflict);
        }
        if result.responses.len() != count {
            return Err(GCStateTxnError::ResponseCount {
                expected: count,
                actual: result.responses.len(),
            });
        }
        Ok(())
    }
}
