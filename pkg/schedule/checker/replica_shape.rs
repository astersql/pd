// Copyright 2026 AsterSQL.
//! Replica count and strictly improving location repairs over canonical snapshots.
use crate::{PauseController, ReplicaFaultContext, ReplicaFaultOutcome, ReplicaStrategy};
use astersql_pd_core::{Peer, RegionInfo};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::CheckerConfigProvider;
use astersql_pd_schedule_operator::{BuilderContext, BuilderError, OpKind, PeerOperatorFactory};

/// A coherent host view shared with the preceding fault-repair stage.
pub type ReplicaShapeContext<'a, C> = ReplicaFaultContext<'a, C>;
/// Operator, typed builder diagnostics and host retry-cache insertion intent.
pub type ReplicaShapeOutcome = ReplicaFaultOutcome;
/// Run after fault repair; add before removing excess voters, then improve diversity.
#[derive(Default, Debug)]
pub struct ReplicaShapeChecker {
    pub pause: PauseController,
}
impl ReplicaShapeChecker {
    /// Produce at most one real operation without mutating region/store snapshots.
    /// Failed factories fall through to later checks, matching Go's Check order.
    pub async fn check<C, F>(
        &self,
        context: ReplicaShapeContext<'_, C>,
        region: &RegionInfo,
        mut allocate: F,
    ) -> ReplicaShapeOutcome
    where
        C: CheckerConfigProvider,
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let mut out = ReplicaShapeOutcome::default();
        if self
            .pause
            .is_paused_at(context.now_unix_nanos.div_euclid(1_000_000_000) as i64)
        {
            return out;
        }
        let stores = context.stores.get_stores();
        let co = context.stores.get_region_stores(region);
        let strategy = ReplicaStrategy {
            checker_name: "replica-checker",
            conf: context.config,
            stores: &stores,
            location_labels: context.config.get_location_labels(),
            isolation_level: context.config.get_isolation_level(),
            region,
            extra_filters: &[],
            fast_failover: false,
            now_unix_nanos: context.now_unix_nanos,
        };
        let factory = |desc| {
            PeerOperatorFactory::new(
                desc,
                BuilderContext {
                    stores: context.stores,
                    config: context.config,
                    rules: context.rules,
                    now_unix_nanos: context.now_unix_nanos,
                },
                region,
                context.supports_conf_change_v2,
            )
        };
        if context.config.is_make_up_replica_enabled()
            && (region.meta().peers.len() as i64) < context.config.get_max_replicas()
        {
            let (target, temp) = strategy.select_store_to_add(&co, &[]).await;
            if target == 0 {
                out.pending |= temp;
            } else {
                match factory("make-up-replica")
                    .create_add_peer(
                        Some(Peer {
                            store_id: target,
                            ..Default::default()
                        }),
                        OpKind::REPLICA,
                        &mut allocate,
                    )
                    .await
                {
                    Ok(op) => {
                        op.set_priority_level(PriorityLevel::HIGH);
                        out.operator = Some(op);
                        return out;
                    }
                    Err(e) => out.errors.push(e),
                }
            }
        }
        if context.config.is_remove_extra_replica_enabled()
            && (region.voters().len() as i64) > context.config.get_max_replicas()
        {
            let old = strategy.select_store_to_remove(&co).await;
            if old == 0 {
                out.pending = true;
            } else {
                match factory("remove-extra-replica")
                    .create_remove_peer(old, OpKind::REPLICA, &mut allocate)
                    .await
                {
                    Ok(op) => {
                        out.operator = Some(op);
                        return out;
                    }
                    Err(e) => out.errors.push(e),
                }
            }
        }
        if context.config.is_location_replacement_enabled() {
            let old = strategy.select_store_to_remove(&co).await;
            if old != 0 {
                // Go intentionally ignores the temporary-state flag for improvement.
                let (target, _) = strategy.select_store_to_improve(&co, old).await;
                if target != 0 {
                    match factory("move-to-better-location")
                        .create_move_peer(
                            old,
                            Some(Peer {
                                store_id: target,
                                ..Default::default()
                            }),
                            OpKind::REPLICA,
                            &mut allocate,
                        )
                        .await
                    {
                        Ok(op) => out.operator = Some(op),
                        Err(e) => out.errors.push(e),
                    }
                }
            }
        }
        out
    }
}
