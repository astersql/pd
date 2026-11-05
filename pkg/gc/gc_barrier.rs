// Copyright 2026 AsterSQL.
use crate::{GCReadError, GCStateManager};
use astersql_pd_keypath::NULL_KEYSPACE_ID;
use astersql_pd_storage_domain::{BarrierTime, GCBarrier, PreparedGCStateTxn};
use astersql_pd_typeutil::Duration;
use tokio::sync::RwLockWriteGuard;
use tokio_util::sync::CancellationToken;
/// A revision-validated barrier change under the shared manager write lock.
pub struct PreparedGCBarrierChange<'a> {
    guard: RwLockWriteGuard<'a, ()>,
    transaction: PreparedGCStateTxn<'a>,
    barrier: Option<GCBarrier>,
    cancel: CancellationToken,
}
impl GCStateManager {
    pub async fn set_gc_barrier(
        &self,
        id: u32,
        name: &str,
        ts: u64,
        ttl: Duration,
        now: BarrierTime,
    ) -> Result<GCBarrier, GCReadError> {
        Ok(self
            .prepare_set_gc_barrier(id, name, ts, ttl, now, &CancellationToken::new())
            .await?
            .commit()
            .await?
            .expect("set prepares a new barrier"))
    }
    pub async fn prepare_set_gc_barrier(
        &self,
        id: u32,
        name: &str,
        ts: u64,
        ttl: Duration,
        now: BarrierTime,
        cancel: &CancellationToken,
    ) -> Result<PreparedGCBarrierChange<'_>, GCReadError> {
        if ttl.as_nanos() <= 0 {
            return Err(GCReadError::InvalidArgument {
                argument: "ttl",
                value: ttl.to_string(),
            });
        }
        let id = self.redirect(id, true).await?;
        let guard = tokio::select! {biased;_=cancel.cancelled()=>return Err(GCReadError::Cancelled),guard=self.mutation_lock()=>guard};
        let (transaction, barrier) = tokio::select! {biased;_=cancel.cancelled()=>return Err(GCReadError::Cancelled),result=self.stage_gc_barrier_set(id,name.as_bytes(),ts,ttl,now)=>result}?;

        Ok(PreparedGCBarrierChange {
            guard,
            transaction,
            barrier: Some(barrier),
            cancel: cancel.clone(),
        })
    }
    pub async fn delete_gc_barrier(
        &self,
        id: u32,
        name: &str,
    ) -> Result<Option<GCBarrier>, GCReadError> {
        self.prepare_delete_gc_barrier(id, name, &CancellationToken::new())
            .await?
            .commit()
            .await
    }
    pub async fn prepare_delete_gc_barrier(
        &self,
        id: u32,
        name: &str,
        cancel: &CancellationToken,
    ) -> Result<PreparedGCBarrierChange<'_>, GCReadError> {
        let id = self.redirect(id, true).await?;
        let guard = tokio::select! {biased;_=cancel.cancelled()=>return Err(GCReadError::Cancelled),guard=self.mutation_lock()=>guard};
        let (transaction, barrier) = tokio::select! {biased;_=cancel.cancelled()=>return Err(GCReadError::Cancelled),result=self.stage_gc_barrier_delete(id,name.as_bytes())=>result}?;

        Ok(PreparedGCBarrierChange {
            guard,
            transaction,
            barrier,
            cancel: cancel.clone(),
        })
    }

    pub(crate) async fn stage_gc_barrier_set(
        &self,
        id: u32,
        name: &[u8],
        ts: u64,
        ttl: Duration,
        now: BarrierTime,
    ) -> Result<(PreparedGCStateTxn<'_>, GCBarrier), GCReadError> {
        validate_id(name)?;
        let mut expiration = now.add_nanos(ttl.as_nanos());
        let barrier = GCBarrier::new(
            String::from_utf8_lossy(name).into_owned(),
            ts,
            if ttl.as_nanos() == i64::MAX {
                None
            } else {
                Some(&mut expiration)
            },
        );
        let mut transaction = self.storage.begin_gc_state_transaction(&self.paths).await?;
        let txn = self.storage.load_txn_safe_point(&self.paths, id).await?;
        if ts < txn {
            return Err(GCReadError::BarrierBehind { barrier: ts, txn });
        }
        transaction
            .batch
            .set_gc_barrier_with_service_id(id, name, &barrier)?;
        Ok((transaction, barrier))
    }
    pub(crate) async fn stage_gc_barrier_delete(
        &self,
        id: u32,
        name: &[u8],
    ) -> Result<(PreparedGCStateTxn<'_>, Option<GCBarrier>), GCReadError> {
        validate_id(name)?;
        let mut transaction = self.storage.begin_gc_state_transaction(&self.paths).await?;
        let barrier = self
            .storage
            .load_gc_barrier_with_service_id(&self.paths, id, name)
            .await?;
        transaction
            .batch
            .delete_gc_barrier_with_service_id(id, name);
        Ok((transaction, barrier))
    }
    /// The legacy HTTP force-delete bypasses ordinary reserved/empty ID and record-decode constraints.
    pub async fn force_delete_service_gc_safe_point(&self, name: &str) -> Result<(), GCReadError> {
        let _guard = self.mutation_lock().await;
        let mut transaction = self.storage.begin_gc_state_transaction(&self.paths).await?;
        transaction.batch.delete_gc_barrier(NULL_KEYSPACE_ID, name);
        transaction.commit().await?;
        Ok(())
    }
}
fn validate_id(name: &[u8]) -> Result<(), GCReadError> {
    if name == b"gc_worker" {
        return Err(GCReadError::ReservedBarrierId("gc_worker".into()));
    }
    if name.is_empty() {
        return Err(GCReadError::InvalidArgument {
            argument: "barrierID",
            value: String::new(),
        });
    }
    Ok(())
}
impl PreparedGCBarrierChange<'_> {
    pub fn barrier(&self) -> Option<&GCBarrier> {
        self.barrier.as_ref()
    }
    pub async fn commit(self) -> Result<Option<GCBarrier>, GCReadError> {
        let Self {
            guard,
            transaction,
            barrier,
            cancel,
        } = self;
        tokio::select! {biased;_=cancel.cancelled()=>return Err(GCReadError::Cancelled),response=transaction.commit()=>response?};
        drop(guard);
        Ok(barrier)
    }
}
