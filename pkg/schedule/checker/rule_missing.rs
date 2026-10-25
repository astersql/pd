// Copyright 2026 AsterSQL.
//! Missing placement peers, with host-owned fit cache and retry membership.
use crate::{PauseController, ReplicaFaultContext, ReplicaStrategy, get_rule_fit_stores};
use astersql_pd_core::{ENGINE_KEY, Peer, PeerRole, RegionInfo};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::CheckerConfigProvider;
use astersql_pd_schedule_filter::LabelConstraintFilter;
use astersql_pd_schedule_operator::{
    BuilderContext, BuilderError, OpKind, Operator, PeerOperatorFactory,
};
use astersql_pd_schedule_placement::{
    LabelConstraintOp, PeerRoleType, RegionFit, Rule, RuleFit, RuleManager,
    match_label_constraints,
    region_rule_cache::{validate_fit, validate_region, validate_stores},
};
use std::sync::Arc;

/// The host supplies a version/config-derived witness capability, never a runtime.
pub struct RuleMissingContext<'a, C: CheckerConfigProvider> {
    pub replica: ReplicaFaultContext<'a, C>,
    pub witness_enabled: bool,
}
/// Per-rule errors remain available even when a later rule produces an operation.
#[derive(Debug, PartialEq)]
pub enum RuleMissingError {
    NoStoreToAdd,
    NoStoreToReplace,
    Builder(BuilderError),
}
/// Retry intents map to Go's pendingList and pendingProcessedRegions respectively.
/// A returned operation requests removal from pendingList, including after an error.
#[derive(Default)]
pub struct RuleMissingOutcome {
    pub operator: Option<Operator>,
    pub pending: bool,
    pub pending_processed: bool,
    pub errors: Vec<RuleMissingError>,
}
/// This stage repairs missing peers only. Orphan, fault and role repair compose
/// around it in the host; they are not silently approximated here.
#[derive(Default, Debug)]
pub struct RuleMissingChecker {
    pub pause: PauseController,
}
impl RuleMissingChecker {
    /// Calculate a real placement fit through the canonical manager.
    pub async fn check<C, F>(
        &self,
        context: &RuleMissingContext<'_, C>,
        manager: &mut RuleManager,
        region: &RegionInfo,
        allocate: &mut F,
    ) -> RuleMissingOutcome
    where
        C: CheckerConfigProvider,
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let fit = manager.fit_region(
            &context.replica.stores.get_stores(),
            region,
            context.replica.config,
            context.replica.now_unix_nanos,
        );
        self.check_with_fit(context, manager, region, Some(fit), allocate)
            .await
    }
    /// Invalidate supplied fit caches before considering each missing rule in order.
    /// A nil fit denotes disabled rules. Snapshots are never modified.
    pub async fn check_with_fit<C, F>(
        &self,
        context: &RuleMissingContext<'_, C>,
        manager: &mut RuleManager,
        region: &RegionInfo,
        fit: Option<Arc<RegionFit>>,
        allocate: &mut F,
    ) -> RuleMissingOutcome
    where
        C: CheckerConfigProvider,
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let mut out = RuleMissingOutcome::default();
        if self
            .pause
            .is_paused_at((context.replica.now_unix_nanos / 1_000_000_000) as i64)
            || region.leader().is_none()
        {
            return out;
        }
        let Some(fit) = fit else {
            return out;
        };
        manager.invalid_cache(region.meta().id);
        for rf in &fit.rule_fits {
            if (rf.peers.len() as i64) < rf.rule.count {
                self.add(context, region, &fit, rf, true, allocate, &mut out)
                    .await;
                if out.operator.is_some() {
                    out.pending = false;
                    return out;
                }
            }
        }
        if context.replica.config.is_placement_rules_cache_enabled()
            && validate_fit(&fit)
            && validate_region(region)
            && validate_stores(fit.region_stores(), context.replica.now_unix_nanos)
        {
            manager.set_region_fit_cache(
                &context.replica.stores.get_stores(),
                region,
                fit,
                context.replica.now_unix_nanos,
            );
        }
        out
    }
    /// Offline-repair preflight: try all unavailable TiFlash rules, without swap-fit
    /// replacement or retry-cache changes. The caller owns the offline decision.
    pub async fn fix_missing_tiflash_learner_peer<C, F>(
        &self,
        context: &RuleMissingContext<'_, C>,
        region: &RegionInfo,
        fit: &RegionFit,
        allocate: &mut F,
    ) -> RuleMissingOutcome
    where
        C: CheckerConfigProvider,
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let mut out = RuleMissingOutcome::default();
        for rf in &fit.rule_fits {
            if (rf.peers.len() as i64) < rf.rule.count && is_tiflash_learner_rule(&rf.rule) {
                let mut attempt = RuleMissingOutcome::default();
                self.add(context, region, fit, rf, false, allocate, &mut attempt)
                    .await;
                if attempt.errors == [RuleMissingError::NoStoreToAdd] {
                    out = attempt;
                    continue;
                }
                return attempt;
            }
        }
        out
    }
    async fn add<C, F>(
        &self,
        context: &RuleMissingContext<'_, C>,
        region: &RegionInfo,
        fit: &RegionFit,
        rf: &RuleFit,
        regular: bool,
        allocate: &mut F,
        out: &mut RuleMissingOutcome,
    ) where
        C: CheckerConfigProvider,
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let (target, temp) = select(context, region, rf, None).await;
        let result = if target != 0 {
            factory(context, region, fit, "add-rule-peer")
                .create_add_peer(
                    Some(peer(&rf.rule, target, context.witness_enabled)),
                    OpKind::REPLICA,
                    allocate,
                )
                .await
        } else {
            if regular {
                out.pending_processed |= temp;
                // A permanent rejection inserts pendingList; a temporary rejection removes it.
                out.pending = !temp;
                // Free a constrained store belonging to another satisfied rule first.
                for p in &region.meta().peers {
                    let store = context.replica.stores.get_store(p.store_id);
                    if !match_label_constraints(store.as_deref(), &rf.rule.label_constraints) {
                        continue;
                    }
                    let Some(old) = fit
                        .rule_fits
                        .iter()
                        .find(|old| old.peers.iter().any(|x| x.id == p.id))
                    else {
                        continue;
                    };
                    if std::ptr::eq(old, rf) || !old.is_satisfied() {
                        continue;
                    }
                    let (destination, temporary) =
                        select(context, region, old, Some(p.store_id)).await;
                    if destination == 0 {
                        out.pending_processed |= temporary;
                        out.pending = !temporary;
                        out.errors.push(RuleMissingError::NoStoreToReplace);
                        return;
                    }
                    match factory(context, region, fit, "replace-rule-swap-fit-peer")
                        .create_move_peer(
                            p.store_id,
                            Some(peer(&old.rule, destination, context.witness_enabled)),
                            OpKind::REPLICA,
                            allocate,
                        )
                        .await
                    {
                        Ok(op) => {
                            op.set_priority_level(
                                if old.rule.is_witness && context.witness_enabled {
                                    PriorityLevel::URGENT
                                } else {
                                    PriorityLevel::HIGH
                                },
                            );
                            out.operator = Some(op);
                        }
                        Err(e) => out.errors.push(RuleMissingError::Builder(e)),
                    }
                    return;
                }
            }
            out.errors.push(RuleMissingError::NoStoreToAdd);
            return;
        };
        match result {
            Ok(op) => {
                op.set_priority_level(PriorityLevel::HIGH);
                out.operator = Some(op);
            }
            Err(e) => out.errors.push(RuleMissingError::Builder(e)),
        }
    }
}
/// Only a singleton engine=in[tiflash] learner rule qualifies for offline preflight.
pub fn is_tiflash_learner_rule(rule: &Rule) -> bool {
    rule.role == PeerRoleType::Learner
        && rule.label_constraints.iter().any(|c| {
            c.key == ENGINE_KEY && c.op == LabelConstraintOp::In && c.values == ["tiflash"]
        })
}
fn peer(rule: &Rule, store_id: u64, enabled: bool) -> Peer {
    Peer {
        store_id,
        role: if rule.role == PeerRoleType::Learner {
            PeerRole::Learner
        } else {
            PeerRole::Voter
        },
        is_witness: rule.is_witness && enabled,
        ..Default::default()
    }
}
fn factory<'a, C: CheckerConfigProvider>(
    context: &'a RuleMissingContext<'_, C>,
    region: &'a RegionInfo,
    fit: &'a RegionFit,
    desc: &str,
) -> PeerOperatorFactory<'a, C::Version> {
    PeerOperatorFactory::new(
        desc,
        BuilderContext {
            stores: context.replica.stores,
            config: context.replica.config,
            rules: fit.rules(),
            now_unix_nanos: context.replica.now_unix_nanos,
        },
        region,
        context.replica.supports_conf_change_v2,
    )
}
async fn select<C: CheckerConfigProvider>(
    context: &RuleMissingContext<'_, C>,
    region: &RegionInfo,
    rf: &RuleFit,
    old: Option<u64>,
) -> (u64, bool) {
    let stores = context.replica.stores.get_stores();
    let filter = LabelConstraintFilter::new("rule-checker", rf.rule.label_constraints.clone());
    let filters: &[&dyn astersql_pd_schedule_filter::Filter<C::Version>] = &[&filter];
    let strategy = ReplicaStrategy {
        checker_name: "rule-checker",
        conf: context.replica.config,
        stores: &stores,
        location_labels: &rf.rule.location_labels,
        isolation_level: &rf.rule.isolation_level,
        region,
        extra_filters: filters,
        fast_failover: rf.rule.is_witness && context.witness_enabled,
        now_unix_nanos: context.replica.now_unix_nanos,
    };
    let co = get_rule_fit_stores(&stores, rf);
    match old {
        Some(id) => strategy.select_store_to_fix(&co, id).await,
        None => strategy.select_store_to_add(&co, &[]).await,
    }
}
