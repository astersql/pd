// Copyright 2026 AsterSQL.

use crate::kv::{Base, Mutation, RawTxn, RawTxnCmpType, RawTxnCondition, RawTxnOp, StoreError};
use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};

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
#[doc(hidden)]
pub fn encode_gc_safe_point(keyspace_id: u32, safe_point: u64) -> String {
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
    #[error(transparent)]
    ParseRevision(astersql_pd_typeutil::GoParseUintError),
    #[error("[PD:etcd:ErrEtcdTxnConflict]etcd transaction failed, conflicted and rolled back")]
    Conflict,
    #[error("unexpected number of results: {expected} != {actual}")]
    ResponseCount { expected: usize, actual: usize },
    #[error("{}",gc_transaction_store_message(.0))]
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
        let mut transaction = self
            .begin_gc_state_transaction(paths)
            .await
            .map_err(promote_txn_error)?;
        callback(&mut transaction.batch)
            .await
            .map_err(GCStateTxnError::Callback)?;
        transaction.commit().await.map_err(promote_txn_error)
    }
    /// Begins a revision-validated transaction without retaining a generic async callback.
    pub async fn begin_gc_state_transaction<'a>(
        &self,
        paths: &'a ClusterPaths,
    ) -> Result<PreparedGCStateTxn<'a>, GCStateTxnError<std::convert::Infallible>> {
        let key = paths.gc_state_revision_path().into_bytes();
        let current = Base::new(self.store.as_ref()).load(&key).await?;
        let revision = if current.is_empty() {
            0u64
        } else {
            let (value, error) = astersql_pd_typeutil::parse_uint64_with_base(
                &current,
                astersql_pd_typeutil::GoUintBase::Decimal,
            );
            if let Some(error) = error {
                return Err(GCStateTxnError::ParseRevision(error));
            }
            value
        };
        Ok(PreparedGCStateTxn {
            store: self.store.clone(),
            key,
            current,
            revision,
            batch: GCStateWriteBatch::new(paths),
        })
    }
}
/// Owned backend transaction state; buffered writes remain local until commit.
pub struct PreparedGCStateTxn<'a> {
    store: std::sync::Arc<dyn crate::kv::MetaStore>,
    key: Vec<u8>,
    current: Vec<u8>,
    revision: u64,
    pub batch: GCStateWriteBatch<'a>,
}
impl PreparedGCStateTxn<'_> {
    pub async fn commit(self) -> Result<(), GCStateTxnError<std::convert::Infallible>> {
        let Self {
            store,
            key,
            current,
            revision,
            mut batch,
        } = self;
        if !batch.ops.is_empty() {
            batch
                .ops
                .push(Mutation::put(&key, revision.wrapping_add(1).to_string().as_bytes()).into());
        }
        let count = batch.ops.len();
        let result = RawTxn::new(store.as_ref())
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

fn promote_txn_error<E>(error: GCStateTxnError<std::convert::Infallible>) -> GCStateTxnError<E> {
    match error {
        GCStateTxnError::Callback(never) => match never {},
        GCStateTxnError::ParseRevision(error) => GCStateTxnError::ParseRevision(error),
        GCStateTxnError::Conflict => GCStateTxnError::Conflict,
        GCStateTxnError::ResponseCount { expected, actual } => {
            GCStateTxnError::ResponseCount { expected, actual }
        }
        GCStateTxnError::Store(error) => GCStateTxnError::Store(error),
    }
}

fn gc_transaction_store_message(error: &StoreError) -> String {
    match error {
        StoreError::RequestTooLarge=>"[PD:etcd:ErrEtcdTxnInternal]internal etcd transaction error occurred: etcdserver: request is too large".into(),
        _=>error.to_string(),
    }
}
