// Copyright 2026 AsterSQL.
use crate::{GCReadError, GCStateManager, txn_safe_point::CacheOutcome};
use astersql_pd_storage_domain::PreparedGCStateTxn;
use tokio::sync::RwLockWriteGuard;
use tokio_util::sync::CancellationToken;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GCSafePointUpdate {
    pub old_gc_safe_point: u64,
    pub new_gc_safe_point: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GCSafePointMode {
    Strict,
    Compatible,
}
pub struct PreparedGCSafePointAdvance<'a> {
    manager: &'a GCStateManager,
    guard: RwLockWriteGuard<'a, ()>,
    transaction: PreparedGCStateTxn<'a>,
    id: u32,
    txn: u64,
    result: GCSafePointUpdate,
    target: u64,
    cancel: CancellationToken,
}
impl GCStateManager {
    pub async fn advance_gc_safe_point(
        &self,
        id: u32,
        target: u64,
    ) -> Result<GCSafePointUpdate, GCReadError> {
        self.prepare_gc_safe_point(
            id,
            target,
            GCSafePointMode::Strict,
            &CancellationToken::new(),
        )
        .await?
        .commit()
        .await
    }
    pub async fn compatible_update_gc_safe_point(
        &self,
        id: u32,
        target: u64,
    ) -> Result<GCSafePointUpdate, GCReadError> {
        self.prepare_gc_safe_point(
            id,
            target,
            GCSafePointMode::Compatible,
            &CancellationToken::new(),
        )
        .await?
        .commit()
        .await
    }
    pub async fn prepare_gc_safe_point(
        &self,
        id: u32,
        target: u64,
        mode: GCSafePointMode,
        cancel: &CancellationToken,
    ) -> Result<PreparedGCSafePointAdvance<'_>, GCReadError> {
        let id = self.redirect(id, false).await?;
        let guard = tokio::select! {biased;_=cancel.cancelled()=>return Err(GCReadError::Cancelled),guard=self.mutation_lock()=>guard};
        let result = tokio::select! {biased;_=cancel.cancelled()=>Err(GCReadError::Cancelled),result=async {
            let mut transaction=self.storage.begin_gc_state_transaction(&self.paths).await?;
            let old=self.storage.load_gc_safe_point(&self.paths,id).await?;let txn=self.storage.load_txn_safe_point(&self.paths,id).await?;
            let next=if target<old {
                if mode==GCSafePointMode::Strict{return Err(GCReadError::DecreasingGCSafePoint{current:old,given:target});}
                old
            }else{
                if target>txn{return Err(GCReadError::GCExceedsTxn{current:old,given:target,txn});}
                transaction.batch.set_gc_safe_point(id,target);target
            };
            Ok::<_,GCReadError>((transaction,txn,GCSafePointUpdate{old_gc_safe_point:old,new_gc_safe_point:next}))
        }=>result};
        match result {
            Ok((transaction, txn, result)) => Ok(PreparedGCSafePointAdvance {
                manager: self,
                guard,
                transaction,
                id,
                txn,
                result,
                target,
                cancel: cancel.clone(),
            }),
            Err(error) => {
                self.invalidate_points(id);
                Err(error)
            }
        }
    }
}
impl PreparedGCSafePointAdvance<'_> {
    pub fn result(&self) -> GCSafePointUpdate {
        self.result
    }
    pub async fn commit(self) -> Result<GCSafePointUpdate, GCReadError> {
        let Self {
            manager,
            guard,
            transaction,
            id,
            txn,
            result,
            target,
            cancel,
        } = self;
        let mut outcome = CacheOutcome::new(manager, id);
        tokio::select! {biased;_=cancel.cancelled()=>return Err(GCReadError::Cancelled),response=transaction.commit()=>response?};
        outcome.publish(txn, result.new_gc_safe_point);
        if id == astersql_pd_keypath::NULL_KEYSPACE_ID {
            manager.shared.gc_point_metrics(target);
        }
        drop(guard);
        Ok(result)
    }
}
