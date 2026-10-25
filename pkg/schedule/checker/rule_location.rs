// Copyright 2026 AsterSQL.
//! Orphan cleanup and rule-constrained diversity improvement over real fits.
use crate::{
    PauseController, ReplicaStrategy, RuleMissingContext, RuleMissingError, RuleMissingOutcome,
};
use astersql_pd_core::{Peer, PeerRole, RegionInfo};
use astersql_pd_schedule_config::CheckerConfigProvider;
use astersql_pd_schedule_filter::LabelConstraintFilter;
use astersql_pd_schedule_operator::{
    BuilderContext, BuilderError, OpKind, Operator, PeerOperatorFactory,
};
use astersql_pd_schedule_placement::{RegionFit, RuleFit};

/// Host composes orphan cleanup before rule repair and location improvement after
/// each rule's fault/role repair. Snapshots and retry caches remain host owned.
#[derive(Default, Debug)]
pub struct RuleLocationChecker {
    pub pause: PauseController,
}
impl RuleLocationChecker {
    fn stopped<C: CheckerConfigProvider>(
        &self,
        ctx: &RuleMissingContext<'_, C>,
        r: &RegionInfo,
    ) -> bool {
        self.pause
            .is_paused_at(ctx.replica.now_unix_nanos.div_euclid(1_000_000_000) as i64)
            || r.leader().is_none()
    }
    /// Remove a safe orphan, exchange it with a disconnected fitted peer, or
    /// preserve at least one healthy spare when the fit itself is unhealthy.
    pub async fn fix_orphan_peers<C, F>(
        &self,
        ctx: &RuleMissingContext<'_, C>,
        r: &RegionInfo,
        fit: &RegionFit,
        allocate: &mut F,
    ) -> Result<Option<Operator>, BuilderError>
    where
        C: CheckerConfigProvider,
        F: FnMut() -> Result<u64, BuilderError>,
    {
        if self.stopped(ctx, r) || fit.orphan_peers.is_empty() {
            return Ok(None);
        }
        let unhealthy =
            |p: &Peer| r.get_pending_peer(p.id).is_some() || r.get_down_peer(p.id).is_some();
        let disconnected = |p: &Peer| {
            ctx.replica
                .stores
                .get_store(p.store_id)
                .is_none_or(|s| s.is_disconnected(ctx.replica.now_unix_nanos))
        };
        let mut pin = None;
        let mut unhealthy_fit = false;
        for rf in &fit.rule_fits {
            if !rf.is_satisfied() {
                unhealthy_fit = true;
                break;
            }
            for p in &rf.peers {
                if disconnected(p) || r.get_down_peer(p.id).is_some() {
                    pin = Some(p);
                    unhealthy_fit = true;
                    break;
                }
                if r.get_pending_peer(p.id).is_some() {
                    unhealthy_fit = true;
                    break;
                }
            }
            if unhealthy_fit {
                break;
            }
        }
        if !unhealthy_fit {
            return factory(ctx, r, fit, "remove-orphan-peer")
                .create_remove_peer(fit.orphan_peers[0].store_id, OpKind(0), allocate)
                .await
                .map(Some);
        }
        for orphan in &fit.orphan_peers {
            let Some(pin) = pin else {
                break;
            };
            if pin.id == orphan.id
                || unhealthy(orphan)
                || disconnected(orphan)
                || pin.is_witness
                || orphan.is_witness
                || !disconnected(pin)
            {
                continue;
            }
            let dst = ctx.replica.stores.get_store(orphan.store_id);
            if !fit.replace(pin.store_id, dst.as_deref()) {
                continue;
            }
            let f = factory(ctx, r, fit, "replace-down-peer-with-orphan-peer");
            let op = match (orphan.role, pin.role) {
                (PeerRole::Learner, PeerRole::Voter) => {
                    f.create_promote_learner_and_remove_peer(
                        orphan.store_id,
                        pin.store_id,
                        allocate,
                    )
                    .await
                }
                (PeerRole::Voter, PeerRole::Learner) => {
                    f.create_demote_learner_and_remove_peer(orphan.store_id, pin.store_id, allocate)
                        .await
                }
                (a, b) if a == b => {
                    factory(ctx, r, fit, "remove-replaced-orphan-peer")
                        .create_remove_peer(pin.store_id, OpKind(0), allocate)
                        .await
                }
                _ => continue,
            };
            return op.map(Some);
        }
        if fit.orphan_peers.len() >= 2 {
            let disconnected_peer = fit.orphan_peers.iter().find(|p| disconnected(p));
            let mut healthy_seen = false;
            for orphan in &fit.orphan_peers {
                let (desc, remove) = if unhealthy(orphan) {
                    ("remove-unhealthy-orphan-peer", orphan)
                } else if healthy_seen && fit.extra_count() > 0 {
                    ("remove-orphan-peer", disconnected_peer.unwrap_or(orphan))
                } else {
                    healthy_seen = true;
                    continue;
                };
                return factory(ctx, r, fit, desc)
                    .create_remove_peer(remove.store_id, OpKind(0), allocate)
                    .await
                    .map(Some);
            }
        }
        Ok(None)
    }
    /// Strict diversity gain uses the existing engine/role-aware strategy. An
    /// unavailable target yields host retry intent, not a fabricated operation.
    pub async fn fix_better_location<C, F>(
        &self,
        ctx: &RuleMissingContext<'_, C>,
        r: &RegionInfo,
        fit: &RegionFit,
        rf: &RuleFit,
        allocate: &mut F,
    ) -> RuleMissingOutcome
    where
        C: CheckerConfigProvider,
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let mut out = RuleMissingOutcome::default();
        if self.stopped(ctx, r) || rf.rule.location_labels.is_empty() {
            return out;
        }
        let witness = rf.rule.is_witness && ctx.witness_enabled;
        let stores = ctx.replica.stores.get_stores();
        let filter = LabelConstraintFilter::new("rule-checker", rf.rule.label_constraints.clone());
        let filters: &[&dyn astersql_pd_schedule_filter::Filter<C::Version>] = &[&filter];
        let strategy = ReplicaStrategy {
            checker_name: "rule-checker",
            conf: ctx.replica.config,
            stores: &stores,
            location_labels: &rf.rule.location_labels,
            isolation_level: &rf.rule.isolation_level,
            region: r,
            extra_filters: filters,
            fast_failover: witness,
            now_unix_nanos: ctx.replica.now_unix_nanos,
        };
        let (old, new, temporary) = strategy.get_better_location(r, fit, rf).await;
        if new == 0 {
            out.pending = !temporary;
            out.pending_processed = temporary;
            return out;
        }
        match factory(ctx, r, fit, "move-to-better-location")
            .create_move_peer(
                old,
                Some(Peer {
                    store_id: new,
                    role: if rf.rule.role == astersql_pd_schedule_placement::PeerRoleType::Learner {
                        PeerRole::Learner
                    } else {
                        PeerRole::Voter
                    },
                    is_witness: witness,
                    ..Default::default()
                }),
                OpKind::REPLICA,
                allocate,
            )
            .await
        {
            Ok(op) => out.operator = Some(op),
            Err(e) => out.errors.push(RuleMissingError::Builder(e)),
        }
        out
    }
}
fn factory<'a, C: CheckerConfigProvider>(
    ctx: &'a RuleMissingContext<'_, C>,
    r: &RegionInfo,
    fit: &'a RegionFit,
    desc: &str,
) -> PeerOperatorFactory<'a, C::Version> {
    PeerOperatorFactory::new(
        desc,
        BuilderContext {
            stores: ctx.replica.stores,
            config: ctx.replica.config,
            rules: fit.rules(),
            now_unix_nanos: ctx.replica.now_unix_nanos,
        },
        r,
        ctx.replica.supports_conf_change_v2,
    )
}
