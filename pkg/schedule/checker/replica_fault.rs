// Copyright 2026 AsterSQL.
//! Down/offline repair intent over host-owned scheduling snapshots.
use crate::{PauseController, ReplicaStrategy};
use astersql_pd_core::{Peer, RegionInfo, StoresInfo};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::CheckerConfigProvider;
use astersql_pd_schedule_operator::{
    BuilderContext, BuilderError, OpKind, Operator, PeerOperatorFactory,
};
use astersql_pd_schedule_placement::Rule;
use std::sync::Arc;

/// Configuration, rule result, stores and time supplied by the host for one check.
pub struct ReplicaFaultContext<'a, C: CheckerConfigProvider> {
    pub config: &'a C,
    pub stores: &'a StoresInfo,
    pub rules: &'a [Arc<Rule>],
    pub now_unix_nanos: i128,
    pub supports_conf_change_v2: bool,
}
/// Host applies pending membership to its existing retry cache. Builder errors
/// retain typed diagnostics while preserving Go's no-operation behavior.
#[derive(Default)]
pub struct ReplicaFaultOutcome {
    pub operator: Option<Operator>,
    pub pending: bool,
    pub errors: Vec<BuilderError>,
}
/// Fault repair only; count balancing and location improvement are separate checks.
#[derive(Default, Debug)]
pub struct ReplicaFaultChecker {
    pub pause: PauseController,
}
impl ReplicaFaultChecker {
    /// Try the first eligible down peer, then the first offline peer if no
    /// operation was built. Offline recovery skips every region with learners.
    pub async fn check<C, F>(
        &self,
        context: ReplicaFaultContext<'_, C>,
        region: &RegionInfo,
        mut allocate: F,
    ) -> ReplicaFaultOutcome
    where
        C: CheckerConfigProvider,
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let mut out = ReplicaFaultOutcome::default();
        if self
            .pause
            .is_paused_at(context.now_unix_nanos.div_euclid(1_000_000_000) as i64)
        {
            return out;
        }
        if context.config.is_remove_down_replica_enabled() {
            for stats in region.down_peers() {
                let id = stats.peer.store_id;
                let Some(store) = context.stores.get_store(id) else {
                    break;
                };
                if store.down_time_nanos(context.now_unix_nanos)
                    < context.config.get_max_store_down_time().as_nanos()
                {
                    continue;
                }
                Self::fix(&context, region, id, "down", &mut allocate, &mut out).await;
                break;
            }
        }
        if out.operator.is_none()
            && context.config.is_replace_offline_replica_enabled()
            && region.learners().is_empty()
        {
            for peer in &region.meta().peers {
                let Some(store) = context.stores.get_store(peer.store_id) else {
                    break;
                };
                if store.is_up() {
                    continue;
                }
                Self::fix(
                    &context,
                    region,
                    peer.store_id,
                    "offline",
                    &mut allocate,
                    &mut out,
                )
                .await;
                break;
            }
        }
        out
    }
    async fn fix<C, F>(
        context: &ReplicaFaultContext<'_, C>,
        region: &RegionInfo,
        old: u64,
        status: &str,
        allocate: &mut F,
        out: &mut ReplicaFaultOutcome,
    ) where
        C: CheckerConfigProvider,
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let extra = region.voters().len() as i64 > context.config.get_max_replicas();
        let target = if extra {
            0
        } else {
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
            let (target, temp) = strategy.select_store_to_fix(&co, old).await;
            if target == 0 {
                out.pending |= temp;
                return;
            }
            target
        };
        let description = if extra {
            format!("remove-extra-{status}-replica")
        } else {
            format!("replace-{status}-replica")
        };
        let factory = PeerOperatorFactory::new(
            &description,
            BuilderContext {
                stores: context.stores,
                config: context.config,
                rules: context.rules,
                now_unix_nanos: context.now_unix_nanos,
            },
            region,
            context.supports_conf_change_v2,
        );
        let result = if extra {
            factory
                .create_remove_peer(old, OpKind::REPLICA, allocate)
                .await
        } else {
            factory
                .create_move_peer(
                    old,
                    Some(Peer {
                        store_id: target,
                        ..Default::default()
                    }),
                    OpKind::REPLICA,
                    allocate,
                )
                .await
        };
        match result {
            Ok(op) => {
                op.set_priority_level(PriorityLevel::HIGH);
                out.operator = Some(op);
            }
            Err(e) => out.errors.push(e),
        }
    }
}
