// Copyright 2026 AsterSQL.
use astersql_pd_storage_kv::{Base, CallbackTxn, CallbackTxnError, MetaStore, StoreError};
use tokio_util::sync::CancellationToken;

/// Go's etcdutil.MaxEtcdTxnOps bounds callbacks per transaction, not writes per callback.
pub const MAX_BATCH_OPS: usize = 120;

/// Invokes operations in input order, committing each group of at most 120 once.
/// Stops at the first callback, conflict or backend error; earlier groups stay committed.
/// Callbacks own their cancellation while running. Like Go, even a pre-cancelled
/// nonempty group invokes callbacks before checking cancellation at commit time.
/// Cancellation after dispatch returns Unavailable: that group's outcome is unknown.
/// There is no automatic retry and no atomicity across groups. Each item identifies
/// one operation, allowing a homogeneous async dispatcher over heterogeneous data.
pub async fn run_batch_op_in_txn<'a, T, E, F>(
    storage: &'a dyn MetaStore,
    cancel: &CancellationToken,
    batch: &[T],
    mut operation: F,
) -> Result<(), CallbackTxnError<E>>
where
    F: AsyncFnMut(&T, &mut CallbackTxn<'a>) -> Result<(), E>,
{
    let base = Base::new(storage);
    for chunk in batch.chunks(MAX_BATCH_OPS) {
        // Signal the commit boundary without cancelling arbitrary user callback work.
        let (ready, prepared) = tokio::sync::oneshot::channel();
        let runner = base.run_in_txn(async |txn| {
            for item in chunk {
                operation(item, txn)
                    .await
                    .map_err(CallbackTxnError::Callback)?;
            }
            if cancel.is_cancelled() {
                return Err(CallbackTxnError::Store(StoreError::Cancelled));
            }
            let _ = ready.send(());
            Ok(())
        });
        tokio::pin!(runner);
        let result = tokio::select! {
            biased;
            result = &mut runner => result,
            _ = prepared => {
                tokio::select! {
                    biased;
                    result = &mut runner => result,
                    _ = cancel.cancelled() => return Err(CallbackTxnError::Store(StoreError::Unavailable)),
                }
            }
        };
        result.map_err(|error| match error {
            CallbackTxnError::Callback(error) => error,
            CallbackTxnError::Store(error) => CallbackTxnError::Store(error),
            CallbackTxnError::Conflict => CallbackTxnError::Conflict,
        })?;
    }
    Ok(())
}
