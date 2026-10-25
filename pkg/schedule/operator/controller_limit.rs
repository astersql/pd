// Copyright 2026 AsterSQL.
//! Store budgets and influence queries over canonical controller/cluster state.
use crate::*;
use astersql_pd_core::{BasicCluster, KeyRange, RegionInfo};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_storelimit::{TYPE_NAME_VALUE, VERSION_V1};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// Go compares raw bytes for *every* range, including empty end keys.
/// An empty list accepts every present region; absent regions never match.
pub fn with_range_option(ranges: &[KeyRange], region: Option<&RegionInfo>) -> bool {
    region.is_some_and(|region| {
        ranges.iter().all(|r| {
            let keys = &region.meta().key_range;
            keys.start_key >= r.start_key && r.end_key >= keys.end_key
        })
    })
}
/// All non-nil predicates must accept a region for it to contribute.
pub type OpInfluenceOption<'a> = Option<&'a dyn Fn(&RegionInfo) -> bool>;

/// Total influence caches the first region observation inside each operator.
pub fn new_total_op_influence(operators: &[Arc<Operator>], cluster: &BasicCluster) -> OpInfluence {
    let influence = OpInfluence::new();
    for op in operators {
        op.total_influence(
            &influence,
            cluster.regions.get_region(op.region_id()).as_deref(),
        );
    }
    influence
}
/// Non-consuming check of aggregate costs, with the first member's priority.
/// Missing stores/limiters bypass the remaining check as in Go. Config rates are
/// per minute; only v1 limiters are reset. Store handles remain canonical.
pub async fn exceed_store_limit<C: SharedConfigProvider>(
    operators: &[Arc<Operator>],
    cluster: &BasicCluster,
    config: &C,
) -> bool {
    let Some(first) = operators.first() else {
        return false;
    };
    if first.priority_level() == PriorityLevel::URGENT {
        return false;
    }
    for (id, inf) in new_total_op_influence(operators, cluster).snapshot() {
        for (_, typ) in TYPE_NAME_VALUE {
            let cost = inf.get_step_cost(typ);
            if cost == 0 {
                continue;
            }
            let Some(store) = cluster.stores.get_store(id) else {
                return false;
            };
            let Some(handle) = store.limiter() else {
                return false;
            };
            let mut limit = handle.write().expect("store limiter poisoned");
            if limit.version() == VERSION_V1 {
                limit.reset(config.get_store_limit_by_type(id, typ) / 60.0, typ)
            }
            if !limit.available(cost, typ, first.priority_level()).await {
                return true;
            }
        }
    }
    false
}
/// Charge admitted operators individually, including urgent operators. A failed
/// Take is deliberately ignored just as Go does after its admission check.
async fn take_operator_cost(op: &Arc<Operator>, cluster: &BasicCluster) {
    for (id, inf) in new_total_op_influence(&[op.clone()], cluster).snapshot() {
        let Some(store) = cluster.stores.get_store(id) else {
            continue;
        };
        let Some(handle) = store.limiter() else {
            continue;
        };
        let mut limit = handle.write().expect("store limiter poisoned");
        for (_, typ) in TYPE_NAME_VALUE {
            let cost = inf.get_step_cost(typ);
            if cost != 0 {
                let _ = limit.take(cost, typ, op.priority_level()).await;
            }
        }
    }
}
/// Acknowledge detached/replaced operators against the canonical store limiters.
/// V1 retains token debt; V2 returns snapshot window usage. Never acknowledge a
/// stale removal request: use only the removed members returned by admission.
pub fn ack_removed_operators<C: SharedConfigProvider>(
    operators: &[Arc<Operator>],
    cluster: &BasicCluster,
    config: &C,
) {
    for op in operators {
        for (id, inf) in new_total_op_influence(&[op.clone()], cluster).snapshot() {
            let Some(store) = cluster.stores.get_store(id) else {
                break;
            };
            let Some(handle) = store.limiter() else { break };
            let mut limit = handle.write().expect("store limiter poisoned");
            for (_, typ) in TYPE_NAME_VALUE {
                if limit.version() == VERSION_V1 {
                    limit.reset(config.get_store_limit_by_type(id, typ) / 60.0, typ);
                }
                limit.ack(inf.get_step_cost(typ), typ);
            }
        }
    }
}
impl ControllerAdmission {
    /// Detach by identity, then return outstanding snapshot costs for all members.
    pub fn remove_limited_operator<C: SharedConfigProvider>(
        &self,
        cluster: &BasicCluster,
        config: &C,
        op: &Arc<Operator>,
        reason: CancelReason,
    ) -> Vec<Arc<Operator>> {
        let removed = self.remove_operator(op, reason);
        ack_removed_operators(&removed, cluster, config);
        removed
    }
    pub fn remove_limited_operators<C: SharedConfigProvider>(
        &self,
        cluster: &BasicCluster,
        config: &C,
        reason: CancelReason,
    ) -> Vec<Arc<Operator>> {
        let removed = self.remove_operators(reason);
        ack_removed_operators(&removed, cluster, config);
        removed
    }

    /// Atomically serialize checks, admission and token consumption against other
    /// controller mutations. Like core limiter futures, this future is !Send:
    /// run it on the host task/LocalSet, never hold a store guard while calling it.
    pub async fn add_limited_operators<C: SharedConfigProvider>(
        &self,
        cluster: &BasicCluster,
        config: &C,
        ops: &[Arc<Operator>],
    ) -> Result<AdmissionOutcome, AdmissionError> {
        let mut state = self.state.lock().expect("admission poisoned");
        if exceed_store_limit(ops, cluster, config).await {
            return Err(Self::reject(ops, CancelReason::ExceedStoreLimit));
        }
        let out = self.admit(&mut state, cluster, ops, false)?;
        ack_removed_operators(&out.removed, cluster, config);
        for op in &out.admitted {
            take_operator_cost(op, cluster).await;
        }
        Ok(out)
    }
    /// Drop over-budget waiting batches and continue, as Go promotion does.
    pub async fn promote_limited_waiting<C: SharedConfigProvider>(
        &self,
        cluster: &BasicCluster,
        config: &C,
        random: f64,
    ) -> Result<Option<AdmissionOutcome>, AdmissionError> {
        let mut state = self.state.lock().expect("admission poisoned");
        while let Some(ops) = state.waiting.get_operator(random)? {
            state.waiting_counts.dec_count(ops[0].desc());
            if exceed_store_limit(&ops, cluster, config).await {
                Self::reject(&ops, CancelReason::ExceedStoreLimit);
                continue;
            }
            match self.admit(&mut state, cluster, &ops, true) {
                Ok(out) => {
                    ack_removed_operators(&out.removed, cluster, config);
                    for op in &out.admitted {
                        take_operator_cost(op, cluster).await;
                    }
                    return Ok(Some(out));
                }
                Err(AdmissionError::Rejected(_)) => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    }
    /// Current unfinished deltas, excluding success/timeout/missing regions.
    /// The timeout check retains its source-defined status side effect.
    pub fn get_op_influence(
        &self,
        cluster: Option<&BasicCluster>,
        options: &[OpInfluenceOption<'_>],
    ) -> OpInfluence {
        let influence = OpInfluence::new();
        let Some(cluster) = cluster else {
            return influence;
        };
        for op in self.operators() {
            if !op.check_timeout()
                && !op.check_success()
                && let Some(region) = cluster.regions.get_region(op.region_id())
                && options.iter().flatten().all(|opt| opt(&region))
            {
                op.unfinished_influence(&influence, &region);
            }
        }
        influence
    }
}
/// The dispatch host inserts fast successes here after removal. TTL is ten
/// seconds from insertion; a new operation for the same region replaces it.
/// Explicit monotonic time keeps this library free of background GC/runtime.
#[derive(Default)]
pub struct FastOperatorInfluence {
    operators: Mutex<HashMap<u64, (Arc<Operator>, Instant)>>,
}
impl FastOperatorInfluence {
    pub fn put_at(&self, op: Arc<Operator>, now: Instant) {
        self.operators
            .lock()
            .expect("fast cache poisoned")
            .insert(op.region_id(), (op, now + Duration::from_secs(10)));
    }
    pub fn add_to_at(&self, cluster: &BasicCluster, influence: &OpInfluence, now: Instant) {
        let ops = {
            let mut cache = self.operators.lock().expect("fast cache poisoned");
            cache.retain(|_, (_, expires)| *expires > now);
            cache.values().map(|(op, _)| op.clone()).collect::<Vec<_>>()
        };
        influence.add(&new_total_op_influence(&ops, cluster));
    }
}
