// Copyright 2026 AsterSQL.
//! Rule-scoped fault and loose-role repairs over canonical fits and operators.
use crate::{
    PauseController, ReplicaStrategy, RuleMissingChecker, RuleMissingContext, RuleMissingError,
    RuleMissingOutcome, get_rule_fit_stores,
};
use astersql_pd_core::{Peer, PeerRole, RegionInfo, StoresInfo};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::CheckerConfigProvider;
use astersql_pd_schedule_filter::{LabelConstraintFilter, StoreStateFilter};
use astersql_pd_schedule_operator::{
    BuilderContext, BuilderError, OpKind, Operator, PeerOperatorFactory, RangeOperatorFactory,
};
use astersql_pd_schedule_placement::{PeerRoleType, RegionFit, RuleFit, match_label_constraints};
use std::collections::HashMap;

/// Rule diagnostics preserve domain rejection separately from factory failures.
#[derive(Debug, PartialEq)]
pub enum RuleFaultError {
    NoStoreToReplace,
    PeerCannotBeLeader,
    NoNewLeader,
    PeerCannotBeWitness,
    Missing(RuleMissingError),
    Builder(BuilderError),
}
/// Retry membership is applied by the host, just as in the missing-rule stage.
#[derive(Default)]
pub struct RuleFaultOutcome {
    pub operator: Option<Operator>,
    pub pending: bool,
    pub pending_processed: bool,
    pub errors: Vec<RuleFaultError>,
}
/// Host-owned mutable state; no private timer, runtime or persistent backend.
#[derive(Default, Debug)]
pub struct RuleFaultChecker {
    pub pause: PauseController,
    promoted_until: HashMap<u64, i128>,
    offline_leaders: HashMap<u64, u64>,
    last_leader_update: i128,
}
impl RuleFaultChecker {
    /// Record the completion callback, rather than assuming a planned promotion succeeded.
    pub fn record_region_promote_to_non_witness(
        &mut self,
        region: u64,
        now: i128,
        interval_nanos: i128,
    ) {
        self.promoted_until
            .insert(region, now.saturating_add(interval_nanos));
    }
    /// Count includes the outgoing leader when it won Go's strict first-minimum tie.
    pub fn offline_leader_count(&self, store: u64) -> u64 {
        self.offline_leaders.get(&store).copied().unwrap_or(0)
    }
    /// Refresh looks up map keys (store IDs), never the counter values.
    pub fn refresh(&mut self, stores: &StoresInfo, now: i128) {
        if now - self.last_leader_update > 300_000_000_000
            && self
                .offline_leaders
                .keys()
                .any(|id| stores.get_store(*id).is_none_or(|s| s.is_removed()))
        {
            self.offline_leaders.clear();
        }
        self.promoted_until.retain(|_, until| *until > now);
    }
    /// Process rules in fit order: missing peers, faults, then loose role repair.
    /// Orphan cleanup and better-location optimization remain separate stages.
    pub async fn check_with_fit<C, F>(
        &mut self,
        ctx: &RuleMissingContext<'_, C>,
        region: &RegionInfo,
        fit: &RegionFit,
        allocate: &mut F,
    ) -> RuleFaultOutcome
    where
        C: CheckerConfigProvider,
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let mut out = RuleFaultOutcome::default();
        if self
            .pause
            .is_paused_at(ctx.replica.now_unix_nanos.div_euclid(1_000_000_000) as i64)
            || region.leader().is_none()
        {
            return out;
        }
        self.refresh(ctx.replica.stores, ctx.replica.now_unix_nanos);
        for rf in &fit.rule_fits {
            if (rf.peers.len() as i64) < rf.rule.count {
                let mut missing = RuleMissingOutcome::default();
                RuleMissingChecker::default()
                    .add(ctx, region, fit, rf, true, allocate, &mut missing)
                    .await;
                append_missing(&mut out, missing);
            } else {
                match self
                    .fix_rule(ctx, region, fit, rf, allocate, &mut out)
                    .await
                {
                    Ok(op) => out.operator = op,
                    Err(e) => out.errors.push(e),
                }
            }
            if out.operator.is_some() {
                out.pending = false;
                return out;
            }
        }
        out
    }
    pub(crate) async fn fix_rule<C, F>(
        &mut self,
        ctx: &RuleMissingContext<'_, C>,
        r: &RegionInfo,
        fit: &RegionFit,
        rf: &RuleFit,
        allocate: &mut F,
        out: &mut RuleFaultOutcome,
    ) -> Result<Option<Operator>, RuleFaultError>
    where
        C: CheckerConfigProvider,
        F: FnMut() -> Result<u64, BuilderError>,
    {
        for p in &rf.peers {
            let Some(store) = ctx.replica.stores.get_store(p.store_id) else {
                continue;
            };
            if r.get_down_peer(p.id).is_some() {
                if store.down_time_nanos(ctx.replica.now_unix_nanos)
                    >= ctx.replica.config.get_max_store_down_time().as_nanos()
                {
                    return self
                        .replace(ctx, r, fit, rf, p, true, allocate, out)
                        .await
                        .map(Some);
                }
                if ctx.witness_enabled && p.is_voter() {
                    if let Some(w) = r.witnesses().iter().find(|w| {
                        w.id != p.id
                            && r.get_down_peer(w.id).is_none()
                            && ctx.replica.stores.get_store(w.store_id).is_some()
                    }) {
                        return witness(
                            ctx,
                            r,
                            fit,
                            "promote-witness-for-down",
                            w,
                            false,
                            allocate,
                        )
                        .await
                        .map(Some)
                        .map_err(RuleFaultError::Builder);
                    }
                }
            }
            if !store.is_preparing() && !store.is_serving() {
                let pre = RuleMissingChecker::default()
                    .fix_missing_tiflash_learner_peer(ctx, r, fit, allocate)
                    .await;
                // Go logs preflight errors and proceeds with offline replacement.
                if pre.operator.is_some() {
                    return Ok(pre.operator);
                }
                return self
                    .replace(ctx, r, fit, rf, p, false, allocate, out)
                    .await
                    .map(Some);
            }
        }
        for p in &rf.peers_with_different_role {
            let op = self.fix_loose(ctx, r, fit, rf, p, allocate).await?;
            if op.is_some() {
                return Ok(op);
            }
        }
        Ok(None)
    }
    async fn replace<C, F>(
        &mut self,
        ctx: &RuleMissingContext<'_, C>,
        r: &RegionInfo,
        fit: &RegionFit,
        rf: &RuleFit,
        p: &Peer,
        down: bool,
        allocate: &mut F,
        out: &mut RuleFaultOutcome,
    ) -> Result<Operator, RuleFaultError>
    where
        C: CheckerConfigProvider,
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let store = ctx
            .replica
            .stores
            .get_store(p.store_id)
            .ok_or(RuleFaultError::NoStoreToReplace)?;
        let fast = ctx.witness_enabled && store.is_tikv() && (down || rf.rule.is_witness);
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
            fast_failover: fast,
            now_unix_nanos: ctx.replica.now_unix_nanos,
        };
        let (target, temp) = strategy
            .select_store_to_fix(&get_rule_fit_stores(&stores, rf), p.store_id)
            .await;
        if target == 0 {
            out.pending = !temp;
            out.pending_processed |= temp;
            return Err(RuleFaultError::NoStoreToReplace);
        }
        let mut leader = None;
        if r.leader().is_some_and(|l| l.id == p.id) {
            let mut min = u64::MAX;
            for candidate in &r.meta().peers {
                let count = self.offline_leader_count(candidate.store_id);
                if min > count
                    && (candidate.id == p.id
                        || (r.get_down_peer(candidate.id).is_none()
                            && r.get_pending_peer(candidate.id).is_none()
                            && self.allow_leader(ctx, fit, candidate).await))
                {
                    min = count;
                    leader = Some(candidate.store_id);
                }
            }
        }
        let status = if down { "down" } else { "offline" };
        let replace_leader = leader.is_some_and(|id| id != p.store_id);
        let desc = if replace_leader {
            format!("replace-rule-{status}-leader-peer")
        } else if fast {
            format!("fast-replace-rule-{status}-peer")
        } else {
            format!("replace-rule-{status}-peer")
        };
        let new = Some(Peer {
            store_id: target,
            role: if rf.rule.role == PeerRoleType::Learner {
                PeerRole::Learner
            } else {
                PeerRole::Voter
            },
            is_witness: fast,
            ..Default::default()
        });
        let f = factory(ctx, r, fit, &desc);
        let result = if replace_leader {
            f.create_replace_leader_peer(
                p.store_id,
                new,
                leader.unwrap(),
                OpKind::REPLICA,
                allocate,
            )
            .await
        } else {
            f.create_move_peer(p.store_id, new, OpKind::REPLICA, allocate)
                .await
        };
        let op = result.map_err(RuleFaultError::Builder)?;
        if let Some(id) = leader {
            *self.offline_leaders.entry(id).or_default() += 1;
            self.last_leader_update = ctx.replica.now_unix_nanos;
        }
        op.set_priority_level(if fast {
            PriorityLevel::URGENT
        } else {
            PriorityLevel::HIGH
        });
        Ok(op)
    }
    async fn fix_loose<C, F>(
        &self,
        ctx: &RuleMissingContext<'_, C>,
        r: &RegionInfo,
        fit: &RegionFit,
        rf: &RuleFit,
        p: &Peer,
        allocate: &mut F,
    ) -> Result<Option<Operator>, RuleFaultError>
    where
        C: CheckerConfigProvider,
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let leader = r.leader().is_some_and(|l| l.id == p.id);
        let result = if p.is_learner() && rf.rule.role != PeerRoleType::Learner {
            factory(ctx, r, fit, "fix-peer-role")
                .create_promote_learner(p.store_id, allocate)
                .await
        } else if !leader && rf.rule.role == PeerRoleType::Leader {
            if !self.allow_leader(ctx, fit, p).await {
                return Err(RuleFaultError::PeerCannotBeLeader);
            }
            factory(ctx, r, fit, "fix-leader-role")
                .create_transfer_leader(p.store_id, vec![], OpKind(0), allocate)
                .await
        } else if leader && rf.rule.role == PeerRoleType::Follower {
            let mut target = None;
            for p in &r.meta().peers {
                if self.allow_leader(ctx, fit, p).await {
                    target = Some(p.store_id);
                    break;
                }
            }
            let target = target.ok_or(RuleFaultError::NoNewLeader)?;
            factory(ctx, r, fit, "fix-follower-role")
                .create_transfer_leader(target, vec![], OpKind(0), allocate)
                .await
        } else if p.is_voter() && rf.rule.role == PeerRoleType::Learner {
            factory(ctx, r, fit, "fix-demote-voter")
                .create_demote_voter(p.store_id, allocate)
                .await
        } else {
            if leader && rf.rule.is_witness {
                return Err(RuleFaultError::PeerCannotBeWitness);
            }
            if !p.is_witness && rf.rule.is_witness && ctx.witness_enabled {
                if self
                    .promoted_until
                    .get(&r.meta().id)
                    .is_some_and(|t| *t > ctx.replica.now_unix_nanos)
                    || !r.pending_peers().is_empty()
                {
                    return Ok(None);
                }
                witness(ctx, r, fit, "fix-witness-peer", p, true, allocate).await
            } else if p.is_witness && (!rf.rule.is_witness || !ctx.witness_enabled) {
                witness(ctx, r, fit, "fix-non-witness-peer", p, false, allocate).await
            } else {
                return Ok(None);
            }
        };
        result.map(Some).map_err(RuleFaultError::Builder)
    }
    /// Leader safety uses actual store state/budget and a matching leader/voter rule.
    pub async fn allow_leader<C: CheckerConfigProvider>(
        &self,
        ctx: &RuleMissingContext<'_, C>,
        fit: &RegionFit,
        p: &Peer,
    ) -> bool {
        if p.is_learner() || p.is_witness {
            return false;
        }
        let Some(s) = ctx.replica.stores.get_store(p.store_id) else {
            return false;
        };
        let state = StoreStateFilter {
            action_scope: "rule-checker".into(),
            transfer_leader: true,
            now_unix_nanos: ctx.replica.now_unix_nanos,
            ..Default::default()
        };
        state.target(ctx.replica.config, &s).await.is_ok()
            && fit.rule_fits.iter().any(|rf| {
                matches!(rf.rule.role, PeerRoleType::Leader | PeerRoleType::Voter)
                    && match_label_constraints(Some(&s), &rf.rule.label_constraints)
            })
    }
}
fn append_missing(out: &mut RuleFaultOutcome, missing: RuleMissingOutcome) {
    out.operator = missing.operator;
    out.pending = missing.pending;
    out.pending_processed |= missing.pending_processed;
    out.errors
        .extend(missing.errors.into_iter().map(RuleFaultError::Missing));
}
fn context<'a, C: CheckerConfigProvider>(
    ctx: &'a RuleMissingContext<'_, C>,
    fit: &'a RegionFit,
) -> BuilderContext<'a, C::Version> {
    BuilderContext {
        stores: ctx.replica.stores,
        config: ctx.replica.config,
        rules: fit.rules(),
        now_unix_nanos: ctx.replica.now_unix_nanos,
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
        context(ctx, fit),
        r,
        ctx.replica.supports_conf_change_v2,
    )
}
async fn witness<C, F>(
    ctx: &RuleMissingContext<'_, C>,
    r: &RegionInfo,
    fit: &RegionFit,
    desc: &str,
    p: &Peer,
    witness: bool,
    allocate: &mut F,
) -> Result<Operator, BuilderError>
where
    C: CheckerConfigProvider,
    F: FnMut() -> Result<u64, BuilderError>,
{
    RangeOperatorFactory::new(
        desc,
        context(ctx, fit),
        r,
        ctx.replica.supports_conf_change_v2,
    )
    .create_witness(p.store_id, witness, allocate)
    .await
}
