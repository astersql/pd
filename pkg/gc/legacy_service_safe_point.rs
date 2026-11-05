// Copyright 2026 AsterSQL.
use crate::{GCReadError, GCStateManager, txn_safe_point::CacheOutcome};
use astersql_pd_keypath::NULL_KEYSPACE_ID;
use astersql_pd_storage_domain::BarrierTime;
use astersql_pd_typeutil::{Duration, saturating_std_duration_from_seconds};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyServiceSafePoint {
    pub service_id: Vec<u8>,
    pub expired_at: i64,
    pub safe_point: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyServiceSafePointUpdate {
    pub minimum: LegacyServiceSafePoint,
    pub updated: bool,
}
impl GCStateManager {
    /// Maps the legacy byte-valued service API through the same serialized GC state machine.
    pub async fn compatible_update_service_gc_safe_point(
        &self,
        id: u32,
        service: &[u8],
        target: u64,
        ttl_seconds: i64,
        now: BarrierTime,
    ) -> Result<LegacyServiceSafePointUpdate, GCReadError> {
        let id = self.redirect(id, true).await?;
        let _guard = self.mutation_lock().await;
        let (minimum, updated) = if service == b"gc_worker" {
            if ttl_seconds != i64::MAX {
                return Err(GCReadError::LegacyWorkerTTL);
            }
            let mut outcome = CacheOutcome::new(self, id);
            let (transaction, result, gc) = self.stage_txn_advance(id, target, now).await?;
            transaction.commit().await?;
            outcome.publish(result.new_txn_safe_point, gc);
            let service = if result.new_txn_safe_point == target {
                b"gc_worker".to_vec()
            } else {
                result.simulated_service_id
            };
            (
                LegacyServiceSafePoint {
                    service_id: service,
                    expired_at: i64::MAX,
                    safe_point: result.new_txn_safe_point,
                },
                result.old_txn_safe_point != result.new_txn_safe_point,
            )
        } else {
            let duration = Duration::from_nanos(
                saturating_std_duration_from_seconds(ttl_seconds).as_nanos() as i64,
            );
            let global = service == b"native_br" && id == NULL_KEYSPACE_ID;
            let write = if global {
                if ttl_seconds > 0 {
                    match self
                        .stage_global_barrier_set("native_br", target, duration, now)
                        .await
                    {
                        Ok((transaction, _)) => {
                            transaction.commit().await.map_err(GCReadError::from)
                        }
                        Err(error) => Err(error),
                    }
                } else {
                    let (transaction, _) = self.stage_global_barrier_delete("native_br").await?;
                    transaction.commit().await.map_err(GCReadError::from)
                }
            } else if ttl_seconds > 0 {
                match self
                    .stage_gc_barrier_set(id, service, target, duration, now)
                    .await
                {
                    Ok((transaction, _)) => transaction.commit().await.map_err(GCReadError::from),
                    Err(error) => Err(error),
                }
            } else {
                let (transaction, _) = self.stage_gc_barrier_delete(id, service).await?;
                transaction.commit().await.map_err(GCReadError::from)
            };
            if let Err(error) = write {
                let ignored = if global {
                    matches!(error, GCReadError::GlobalBarrierBehind { .. })
                } else {
                    matches!(error, GCReadError::BarrierBehind { .. })
                };
                if !ignored {
                    return Err(error);
                }
            }
            // The follow-up observation belongs after the committed change; it is a separate storage read/transaction.
            let point = if global {
                let transaction = self.storage.begin_gc_state_transaction(&self.paths).await?;
                let maximum = self.max_txn_in_transaction(&transaction).await?;
                transaction.commit().await?;
                maximum.safe_point
            } else {
                self.storage.load_txn_safe_point(&self.paths, id).await?
            };
            (
                LegacyServiceSafePoint {
                    service_id: b"gc_worker".to_vec(),
                    expired_at: i64::MAX,
                    safe_point: point,
                },
                ttl_seconds > 0 && point <= target,
            )
        };
        Ok(LegacyServiceSafePointUpdate { minimum, updated })
    }
}
