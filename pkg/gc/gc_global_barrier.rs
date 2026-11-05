// Copyright 2026 AsterSQL.
use crate::{GCReadError, GCStateManager};
use astersql_pd_keypath::NULL_KEYSPACE_ID;
use astersql_pd_storage_domain::{BarrierTime, GlobalGCBarrier, PreparedGCStateTxn};
use astersql_pd_typeutil::Duration;
use tokio::sync::RwLockWriteGuard;
use tokio_util::sync::CancellationToken;
/// The first enabled scope in iterator order wins ties; null is visited last.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MaxTxnSafePoint {
    pub safe_point: u64,
    pub keyspace_name: String,
    pub keyspace_id: u32,
}
/// One global change validated against the GC revision; point cache remains untouched.
pub struct PreparedGlobalGCBarrierChange<'a> {
    guard: RwLockWriteGuard<'a, ()>,
    transaction: PreparedGCStateTxn<'a>,
    barrier: Option<GlobalGCBarrier>,
    cancel: CancellationToken,
}
impl GCStateManager {
    pub async fn set_global_gc_barrier(
        &self,
        name: &str,
        ts: u64,
        ttl: Duration,
        now: BarrierTime,
    ) -> Result<GlobalGCBarrier, GCReadError> {
        Ok(self
            .prepare_set_global_gc_barrier(name, ts, ttl, now, &CancellationToken::new())
            .await?
            .commit()
            .await?
            .expect("set prepares a global barrier"))
    }
    pub async fn prepare_set_global_gc_barrier(
        &self,
        name: &str,
        ts: u64,
        ttl: Duration,
        now: BarrierTime,
        cancel: &CancellationToken,
    ) -> Result<PreparedGlobalGCBarrierChange<'_>, GCReadError> {
        if ttl.as_nanos() <= 0 {
            return Err(GCReadError::InvalidArgument {
                argument: "ttl",
                value: ttl.to_string(),
            });
        }
        validate_name(name)?;
        let guard = tokio::select! {biased;_=cancel.cancelled()=>return Err(GCReadError::Cancelled),guard=self.mutation_lock()=>guard};
        let mut expiration = now.add_nanos(ttl.as_nanos());
        let barrier = GlobalGCBarrier::new(
            name,
            ts,
            if ttl.as_nanos() == i64::MAX {
                None
            } else {
                Some(&mut expiration)
            },
        );
        let transaction = tokio::select! {biased;_=cancel.cancelled()=>return Err(GCReadError::Cancelled),result=async {
            let mut transaction=self.storage.begin_gc_state_transaction(&self.paths).await?;
            let maximum=self.max_txn_in_transaction(&transaction).await?;
            if ts<maximum.safe_point {
                let keyspace=if maximum.keyspace_id==NULL_KEYSPACE_ID{"<null_keyspace>".into()}else{format!("{{ name: {}, id: {} }}",crate::txn_safe_point::go_quote(maximum.keyspace_name.as_bytes()),maximum.keyspace_id)};
                return Err(GCReadError::GlobalBarrierBehind{barrier:ts,txn:maximum.safe_point,keyspace});
            }
            transaction.batch.set_global_gc_barrier(&barrier)?;Ok::<_,GCReadError>(transaction)
        }=>result}?;
        Ok(PreparedGlobalGCBarrierChange {
            guard,
            transaction,
            barrier: Some(barrier),
            cancel: cancel.clone(),
        })
    }
    pub(crate) async fn max_txn_in_transaction(
        &self,
        _transaction: &PreparedGCStateTxn<'_>,
    ) -> Result<MaxTxnSafePoint, GCReadError> {
        let mut maximum = MaxTxnSafePoint::default();
        let mut start = 0;
        loop {
            let metas = self
                .keyspaces
                .load_range(start, 100)
                .await
                .map_err(|e| GCReadError::Keyspace(std::sync::Arc::new(e)))?;
            if metas.is_empty() {
                break;
            }
            for meta in &metas {
                if meta.state != 0 {
                    continue;
                }
                // Go checks every enabled metadata record, including unified-management records.
                let point = self
                    .storage
                    .load_txn_safe_point(&self.paths, meta.id())
                    .await?;
                if point > maximum.safe_point {
                    maximum = MaxTxnSafePoint {
                        safe_point: point,
                        keyspace_name: meta.name.clone(),
                        keyspace_id: meta.id(),
                    };
                }
            }
            let next = metas
                .last()
                .unwrap()
                .id()
                .checked_add(1)
                .ok_or(GCReadError::NonAdvancingKeyspaceScan)?;
            if next <= start {
                return Err(GCReadError::NonAdvancingKeyspaceScan);
            }
            start = next;
        }
        let point = self
            .storage
            .load_txn_safe_point(&self.paths, NULL_KEYSPACE_ID)
            .await?;
        if point > maximum.safe_point {
            maximum = MaxTxnSafePoint {
                safe_point: point,
                keyspace_name: String::new(),
                keyspace_id: NULL_KEYSPACE_ID,
            };
        }
        Ok(maximum)
    }
    pub async fn get_max_txn_safe_point(&self) -> Result<MaxTxnSafePoint, GCReadError> {
        let _guard = self.mutation_lock().await;
        let transaction = self.storage.begin_gc_state_transaction(&self.paths).await?;
        let result = self.max_txn_in_transaction(&transaction).await?;
        transaction.commit().await?;
        Ok(result)
    }
    pub async fn delete_global_gc_barrier(
        &self,
        name: &str,
    ) -> Result<Option<GlobalGCBarrier>, GCReadError> {
        self.prepare_delete_global_gc_barrier(name, &CancellationToken::new())
            .await?
            .commit()
            .await
    }
    pub async fn prepare_delete_global_gc_barrier(
        &self,
        name: &str,
        cancel: &CancellationToken,
    ) -> Result<PreparedGlobalGCBarrierChange<'_>, GCReadError> {
        validate_name(name)?;
        let guard = tokio::select! {biased;_=cancel.cancelled()=>return Err(GCReadError::Cancelled),guard=self.mutation_lock()=>guard};
        let (transaction, barrier) = tokio::select! {biased;_=cancel.cancelled()=>return Err(GCReadError::Cancelled),result=async {
            let mut transaction=self.storage.begin_gc_state_transaction(&self.paths).await?;let barrier=self.storage.load_global_gc_barrier(&self.paths,name).await?;transaction.batch.delete_global_gc_barrier(name);Ok::<_,GCReadError>((transaction,barrier))
        }=>result}?;
        Ok(PreparedGlobalGCBarrierChange {
            guard,
            transaction,
            barrier,
            cancel: cancel.clone(),
        })
    }
}
fn validate_name(name: &str) -> Result<(), GCReadError> {
    if name.is_empty() {
        return Err(GCReadError::InvalidArgument {
            argument: "barrierID",
            value: String::new(),
        });
    }
    Ok(())
}
impl PreparedGlobalGCBarrierChange<'_> {
    pub fn barrier(&self) -> Option<&GlobalGCBarrier> {
        self.barrier.as_ref()
    }
    pub async fn commit(self) -> Result<Option<GlobalGCBarrier>, GCReadError> {
        let Self {
            guard,
            transaction,
            barrier,
            cancel,
        } = self;
        tokio::select! {biased;_=cancel.cancelled()=>return Err(GCReadError::Cancelled),result=transaction.commit()=>result?};
        drop(guard);
        Ok(barrier)
    }
}
