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
        let epoch = self.shared.barrier_metrics.epoch();
        let (id, name) = self.redirect_named(id, true).await?;
        let guard = self.mutation_lock().await;
        let mut warnings = vec![];
        let (minimum, updated) = if service == b"gc_worker" {
            if ttl_seconds != i64::MAX {
                return Err(GCReadError::LegacyWorkerTTL);
            }
            let mut outcome = CacheOutcome::new(self, id);
            let (transaction, result, gc, observation) =
                self.stage_txn_advance(id, target, now, epoch, name).await?;
            transaction.commit().await?;
            warnings = observation.publish_metrics(&self.shared.barrier_metrics);
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
                        Ok((transaction, barrier)) => transaction
                            .commit()
                            .await
                            .map(|_| {
                                self.shared.barrier_metrics.update_metrics(
                                    crate::gc_metrics::BarrierMetricScope::Global,
                                    &astersql_pd_storage_domain::GCBarrier {
                                        barrier_id: barrier.barrier_id,
                                        barrier_ts: barrier.barrier_ts,
                                        expiration_time: barrier.expiration_time,
                                    },
                                    now,
                                )
                            })
                            .map_err(GCReadError::from),
                        Err(error) => Err(error),
                    }
                } else {
                    let (transaction, _) = self.stage_global_barrier_delete("native_br").await?;
                    transaction
                        .commit()
                        .await
                        .map(|_| {
                            self.shared.barrier_metrics.delete_metrics(
                                crate::gc_metrics::BarrierMetricScope::Global,
                                "native_br",
                            )
                        })
                        .map_err(GCReadError::from)
                }
            } else if ttl_seconds > 0 {
                match self
                    .stage_gc_barrier_set(id, service, target, duration, now)
                    .await
                {
                    Ok((transaction, barrier)) => transaction
                        .commit()
                        .await
                        .map(|_| {
                            self.shared.barrier_metrics.update_metrics(
                                crate::gc_metrics::BarrierMetricScope::Keyspace(id),
                                &barrier,
                                now,
                            )
                        })
                        .map_err(GCReadError::from),
                    Err(error) => Err(error),
                }
            } else {
                let (transaction, _) = self.stage_gc_barrier_delete(id, service).await?;
                transaction
                    .commit()
                    .await
                    .map(|_| {
                        self.shared.barrier_metrics.delete_metrics(
                            crate::gc_metrics::BarrierMetricScope::Keyspace(id),
                            &String::from_utf8_lossy(service),
                        )
                    })
                    .map_err(GCReadError::from)
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
        drop(guard);
        for warning in warnings {
            warning.log();
        }
        Ok(LegacyServiceSafePointUpdate { minimum, updated })
    }
}

/// Legacy HTTP observation, including the synthetic GC worker lower bound.
pub struct LegacyServiceSafePointList {
    pub service_safe_points: Vec<astersql_pd_storage_domain::ServiceSafePoint>,
    pub minimum: u64,
    pub gc_safe_point: u64,
}
impl GCStateManager {
    /// Preserve the legacy list's minimum selection while using canonical state reads.
    pub async fn compatible_service_gc_safe_points(
        &self,
        now: BarrierTime,
    ) -> Result<LegacyServiceSafePointList, GCReadError> {
        let state = self.get_gc_state(NULL_KEYSPACE_ID, false).await?;
        let mut services: Vec<_> = state
            .gc_barriers
            .iter()
            .map(|b| b.to_service_safe_point(NULL_KEYSPACE_ID))
            .collect();
        services.push(astersql_pd_storage_domain::ServiceSafePoint {
            service_id: "gc_worker".into(),
            expired_at: i64::MAX,
            safe_point: state.txn_safe_point,
            keyspace_id: NULL_KEYSPACE_ID,
        });
        let minimum = services
            .iter()
            .filter(|s| s.expired_at > now.unix_seconds())
            .map(|s| s.safe_point)
            .fold(state.txn_safe_point, u64::min);
        Ok(LegacyServiceSafePointList {
            service_safe_points: services,
            minimum,
            gc_safe_point: state.gc_safe_point,
        })
    }
}
