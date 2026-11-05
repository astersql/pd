// Copyright 2026 AsterSQL.
use crate::{GCReadError, GCStateManager};
use astersql_pd_storage_domain::{BarrierTime, GCBarrier, GlobalGCBarrier, PreparedGCStateTxn};
use tokio::sync::RwLockWriteGuard;
use tokio_util::sync::CancellationToken;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvanceTxnSafePointResult {
    pub old_txn_safe_point: u64,
    pub target: u64,
    pub new_txn_safe_point: u64,
    pub blocker_description: String,
    pub simulated_service_id: Vec<u8>,
}
/// A batch prepared under the manager write lock. The host can cancel before commit without writes.
pub struct PreparedTxnSafePointAdvance<'a> {
    manager: &'a GCStateManager,
    guard: RwLockWriteGuard<'a, ()>,
    transaction: PreparedGCStateTxn<'a>,
    id: u32,
    gc: u64,
    result: AdvanceTxnSafePointResult,
    observation: crate::gc_metrics::GCBarrierObservation,
    cancel: CancellationToken,
}
pub(crate) struct CacheOutcome<'a> {
    manager: &'a GCStateManager,
    id: u32,
    confirmed: bool,
}
impl Drop for CacheOutcome<'_> {
    fn drop(&mut self) {
        if !self.confirmed {
            self.manager.invalidate_points(self.id);
        }
    }
}
impl GCStateManager {
    pub async fn advance_txn_safe_point(
        &self,
        id: u32,
        target: u64,
        now: BarrierTime,
    ) -> Result<AdvanceTxnSafePointResult, GCReadError> {
        self.prepare_advance_txn_safe_point(id, target, now, &CancellationToken::new())
            .await?
            .commit()
            .await
    }
    pub async fn prepare_advance_txn_safe_point(
        &self,
        id: u32,
        target: u64,
        now: BarrierTime,
        cancel: &CancellationToken,
    ) -> Result<PreparedTxnSafePointAdvance<'_>, GCReadError> {
        let epoch = self.shared.barrier_metrics.epoch();
        let (id, name) = self.redirect_named(id, false).await?;
        let guard = tokio::select! {biased;_=cancel.cancelled()=>return Err(GCReadError::Cancelled),guard=self.mutation_lock()=>guard};
        let result = tokio::select! {biased;_=cancel.cancelled()=>Err(GCReadError::Cancelled),result=self.stage_txn_advance(id,target,now,epoch,name)=>result};
        match result {
            Ok((transaction, result, gc, observation)) => Ok(PreparedTxnSafePointAdvance {
                manager: self,
                guard,
                transaction,
                id,
                gc,
                result,
                observation,
                cancel: cancel.clone(),
            }),
            Err(error) => {
                self.invalidate_points(id);
                Err(error)
            }
        }
    }
    pub(crate) async fn stage_txn_advance(
        &self,
        id: u32,
        target: u64,
        now: BarrierTime,
        epoch: u64,
        name: String,
    ) -> Result<
        (
            PreparedGCStateTxn<'_>,
            AdvanceTxnSafePointResult,
            u64,
            crate::gc_metrics::GCBarrierObservation,
        ),
        GCReadError,
    > {
        let mut transaction = self.storage.begin_gc_state_transaction(&self.paths).await?;
        let old = self.storage.load_txn_safe_point(&self.paths, id).await?;
        let gc = self.storage.load_gc_safe_point(&self.paths, id).await?;
        if target < old {
            return Err(GCReadError::DecreasingTxnSafePoint {
                current: old,
                given: target,
            });
        }
        let mut minimum = target;
        let mut local: Option<GCBarrier> = None;
        let mut owner: Option<Vec<u8>> = None;
        let mut global: Option<GlobalGCBarrier> = None;
        let mut legacy = false;
        let mut observed_local = vec![];
        let mut observed_global = vec![];
        for barrier in self.storage.load_all_gc_barriers(&self.paths, id).await? {
            let barrier = barrier.ok_or(GCReadError::NullBarrier)?;
            if barrier.barrier_id == "gc_worker" {
                legacy = true;
                continue;
            }
            if barrier.is_expired(now) {
                transaction.batch.delete_gc_barrier(id, &barrier.barrier_id);
                continue;
            }
            if crate::gc_metrics::BarrierMetrics::candidate(
                crate::gc_metrics::BarrierMetricScope::Keyspace(id),
                &barrier,
                now,
            ) {
                observed_local.push(barrier.clone());
            }
            if barrier.barrier_ts < minimum {
                minimum = barrier.barrier_ts;
                local = Some(barrier);
            }
        }
        let (key, start) = self
            .storage
            .compatible_load_tidb_min_start_ts(&self.paths, id)
            .await?;
        if start != 0 && !key.is_empty() && start < minimum {
            minimum = start;
            local = None;
            owner = Some(key);
        }
        for barrier in self
            .storage
            .load_all_global_gc_barriers(&self.paths)
            .await?
        {
            let barrier = barrier.ok_or(GCReadError::NullBarrier)?;
            if barrier.is_expired(now) {
                transaction
                    .batch
                    .delete_global_gc_barrier(&barrier.barrier_id);
                continue;
            }
            let candidate = GCBarrier {
                barrier_id: String::new(),
                barrier_ts: barrier.barrier_ts,
                expiration_time: barrier.expiration_time,
            };
            if crate::gc_metrics::BarrierMetrics::candidate(
                crate::gc_metrics::BarrierMetricScope::Global,
                &candidate,
                now,
            ) {
                observed_global.push(barrier.clone());
            }
            if barrier.barrier_ts < minimum {
                minimum = barrier.barrier_ts;
                global = Some(barrier);
            }
        }
        let next = old.max(minimum);
        if legacy {
            transaction
                .batch
                .set_gc_barrier(id, &GCBarrier::new("gc_worker", next, None))?;
        }
        transaction.batch.set_txn_safe_point(id, next);
        let (description, service) = if let Some(b) = global {
            (
                self.barrier_description(
                    "GlobalGCBarrier",
                    &b.barrier_id,
                    b.barrier_ts,
                    b.expiration_time,
                ),
                b.barrier_id.into_bytes(),
            )
        } else if let Some(key) = owner {
            let description = format!(
                "TiDBMinStartTS {{ Key: {}, MinStartTS: {} }}",
                ascii_quote(&key),
                next
            );
            let mut service = b"tidb_min_start_ts_".to_vec();
            service.extend(key);
            (description, service)
        } else if let Some(b) = local {
            (
                self.barrier_description(
                    "GCBarrier",
                    &b.barrier_id,
                    b.barrier_ts,
                    b.expiration_time,
                ),
                b.barrier_id.into_bytes(),
            )
        } else {
            (String::new(), vec![])
        };
        Ok((
            transaction,
            AdvanceTxnSafePointResult {
                old_txn_safe_point: old,
                target,
                new_txn_safe_point: next,
                blocker_description: description,
                simulated_service_id: service,
            },
            gc,
            crate::gc_metrics::GCBarrierObservation {
                epoch,
                id,
                name,
                local: observed_local,
                global: observed_global,
                now,
            },
        ))
    }
    fn barrier_description(
        &self,
        kind: &str,
        id: &str,
        ts: u64,
        expiration: Option<BarrierTime>,
    ) -> String {
        let expiration =
            expiration.map_or_else(|| "<nil>".into(), |time| (self.diagnostic_time)(time));
        format!(
            "{kind} {{ BarrierID: {}, BarrierTS: {ts}, ExpirationTime: {} }}",
            ascii_quote(id.as_bytes()),
            ascii_quote(expiration.as_bytes())
        )
    }
}
impl PreparedTxnSafePointAdvance<'_> {
    pub fn result(&self) -> &AdvanceTxnSafePointResult {
        &self.result
    }
    pub async fn commit(self) -> Result<AdvanceTxnSafePointResult, GCReadError> {
        let Self {
            manager,
            guard,
            transaction,
            id,
            gc,
            result,
            observation,
            cancel,
        } = self;
        let mut outcome = CacheOutcome::new(manager, id);
        tokio::select! {biased;_=cancel.cancelled()=>return Err(GCReadError::Cancelled),response=transaction.commit()=>response?};
        let warnings = observation.publish_metrics(&manager.shared.barrier_metrics);
        outcome.publish(result.new_txn_safe_point, gc);
        drop(guard);
        for warning in warnings {
            warning.log();
        }
        Ok(result)
    }
}
/// Go fmt %+q renders printable non-ASCII runes as escapes and preserves invalid bytes.
fn ascii_quote(bytes: &[u8]) -> String {
    astersql_pd_typeutil::go_quote_bytes(bytes, true)
}
pub(crate) fn go_quote(bytes: &[u8]) -> String {
    astersql_pd_typeutil::go_quote_bytes(bytes, false)
}
/// Absolute persisted expiration values use whole seconds; host may supply a local-zone formatter.
pub(crate) fn utc_time(time: BarrierTime) -> String {
    let seconds = time.unix_seconds() as i128;
    let days = seconds.div_euclid(86400);
    let day_seconds = seconds.rem_euclid(86400);
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i128::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} +0000 UTC",
        day_seconds / 3600,
        day_seconds % 3600 / 60,
        day_seconds % 60
    )
}

impl<'a> CacheOutcome<'a> {
    pub(crate) fn new(manager: &'a GCStateManager, id: u32) -> Self {
        Self {
            manager,
            id,
            confirmed: false,
        }
    }
    pub(crate) fn publish(&mut self, txn: u64, gc: u64) {
        self.manager.publish_points(self.id, txn, gc);
        self.confirmed = true;
    }
}
