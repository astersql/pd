// Copyright 2026 AsterSQL.
//! Affinity voter/leader repair over canonical placement and persisted group state.
use crate::{PauseController, ReplicaStrategy, RuleMissingContext};
use astersql_pd_core::{PeerRole, RegionInfo};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_affinity::{group::GroupState, manager::Manager};
use astersql_pd_schedule_config::{CheckerConfigProvider, REJECT_LEADER};
use astersql_pd_schedule_filter::{LabelConstraintFilter, is_region_healthy};
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_schedule_operator::{
    BuilderContext, BuilderError, OpKind, Operator, PeerOperatorFactory,
};
use astersql_pd_schedule_placement::{PeerRoleType, RuleManager};
use astersql_pd_statistics::is_region_label_isolation_satisfied;
use astersql_pd_storage_endpoint::StorageEndpoint;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

/// Host supplies current existence after its region-tree lookup, canonical state,
/// storage and explicit time; this checker owns no runtime or cluster mirror.
pub struct AffinityContext<'a, C: CheckerConfigProvider> {
    pub rules: RuleMissingContext<'a, C>,
    pub labels: &'a RegionLabeler,
    pub storage: &'a StorageEndpoint,
    pub paths: &'a ClusterPaths,
    pub region_exists: bool,
}
/// Repairs affinity peers. Already-affine regions are left for the merge stage.
#[derive(Default)]
pub struct AffinityChecker {
    pub pause: PauseController,
    pub(crate) start_time: i128,
    recent_merges: BTreeMap<u64, u64>,
}
impl AffinityChecker {
    pub fn get_type(&self) -> &'static str {
        "affinity-checker"
    }
    /// Non-merge Check, retaining factory failures as typed host diagnostics.
    /// Failed observation transactions are intentionally swallowed by the canonical
    /// manager, preserving Go's unchanged expired/degraded group and no-op result.
    pub async fn check<C, F>(
        &self,
        ctx: &AffinityContext<'_, C>,
        rules: &mut RuleManager,
        manager: &mut Manager,
        region: &Arc<RegionInfo>,
        allocate: &mut F,
    ) -> Result<Option<Operator>, BuilderError>
    where
        C: CheckerConfigProvider,
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let Some((group, affinity)) = self.prepare_group(ctx, rules, manager, region).await else {
            return Ok(None);
        };
        if affinity {
            return Ok(None);
        }
        self.create_affinity_operator(ctx, rules, region, &group, allocate)
            .await
    }
    pub(crate) async fn prepare_group<C: CheckerConfigProvider>(
        &self,
        ctx: &AffinityContext<'_, C>,
        rules: &mut RuleManager,
        manager: &mut Manager,
        region: &Arc<RegionInfo>,
    ) -> Option<(GroupState, bool)> {
        let now = ctx.rules.replica.now_unix_nanos.div_euclid(1_000_000_000) as u64;
        if self.pause.is_paused_at(now as i64)
            || !ctx.rules.replica.config.is_placement_rules_enabled()
            || region.leader().is_none()
            || !is_region_healthy(region)
            || manager.groups.is_empty()
        {
            return None;
        }
        let (group, mut affinity) =
            manager.get_and_cache_region_affinity_group_state(ctx.labels, Some(region), now);
        let Some(mut group) = group else {
            return None;
        };
        if !best_location(ctx, rules, region, true).await {
            return None;
        }
        let mut refetch = !group.affinity_scheduling_allowed;
        if group.affinity_scheduling_allowed && !affinity {
            let valid = if let Some(target) = clone_region_with_replace_peer_stores(
                region,
                group.group.leader_store_id,
                &group.group.voter_store_ids,
            ) {
                best_location(ctx, rules, &target, false).await
            } else {
                false
            };
            if !valid {
                manager.expire_affinity_group(&group.group.id, now);
                group = manager
                    .group_observation(&group.group.id, now)
                    .expect("exclusive manager preserves group")
                    .state;
                refetch = true;
            }
        }
        if refetch {
            let observation = manager.group_observation(&group.group.id, now);
            let stores = ctx
                .rules
                .replica
                .stores
                .get_stores()
                .iter()
                .map(|s| (**s).clone())
                .collect::<Vec<_>>();
            manager
                .observe_available_region(
                    ctx.storage,
                    ctx.paths,
                    &stores,
                    region,
                    observation.as_ref(),
                    now,
                )
                .await;
            let (updated, matched) =
                manager.get_and_cache_region_affinity_group_state(ctx.labels, Some(region), now);
            let Some(updated) = updated else {
                return None;
            };
            group = updated;
            affinity = matched;
        }
        if !ctx.region_exists {
            manager.invalid_cache(region.meta().id);
            return None;
        }
        if !group.affinity_scheduling_allowed {
            return None;
        }
        Some((group, affinity))
    }
    /// Build a single combo operation; learner/voter conflicts abort instead of
    /// silently promoting a learner (including TiFlash replicas).
    pub async fn create_affinity_operator<C, F>(
        &self,
        ctx: &AffinityContext<'_, C>,
        rules: &RuleManager,
        region: &RegionInfo,
        group: &GroupState,
        allocate: &mut F,
    ) -> Result<Option<Operator>, BuilderError>
    where
        C: CheckerConfigProvider,
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let Some(leader) = region.leader() else {
            return Ok(None);
        };
        let target = &group.group;
        let current: BTreeSet<_> = region.voters().iter().map(|p| p.store_id).collect();
        if leader.store_id == target.leader_store_id
            && current == target.voter_store_ids.iter().copied().collect()
        {
            return Ok(None);
        }
        let mut roles: BTreeMap<_, _> = target
            .voter_store_ids
            .iter()
            .map(|id| {
                (
                    *id,
                    if *id == target.leader_store_id {
                        PeerRoleType::Leader
                    } else {
                        PeerRoleType::Voter
                    },
                )
            })
            .collect();
        for learner in region.learners() {
            if roles.contains_key(&learner.store_id) {
                return Ok(None);
            }
            roles.insert(learner.store_id, PeerRoleType::Learner);
        }
        let replica = &ctx.rules.replica;
        let Some(store) = replica.stores.get_store(target.leader_store_id) else {
            return Ok(None);
        };
        if !store.allow_leader_transfer_in()
            || replica
                .config
                .check_label_property(REJECT_LEADER, store.labels())
        {
            return Ok(None);
        }
        let kind = if leader.store_id != target.leader_store_id {
            OpKind::AFFINITY | OpKind::LEADER
        } else {
            OpKind::AFFINITY
        };
        let apply = rules.get_rules_for_apply_region(region);
        PeerOperatorFactory::new(
            "affinity-move-region",
            BuilderContext {
                stores: replica.stores,
                config: replica.config,
                rules: &apply,
                now_unix_nanos: replica.now_unix_nanos,
            },
            region,
            replica.supports_conf_change_v2,
        )
        .create_move_region(roles.into_iter().collect(), kind, allocate)
        .await
        .map(Some)
    }
    /// Cache both IDs for one minute only when affinity scheduling is enabled.
    /// The host calls this on successful completion, as in Go RecordOpSuccess.
    pub fn record_op_success(&mut self, op: &Operator, schedule_limit: u64, now: u64) {
        self.recent_merges.retain(|_, expires| *expires >= now);
        let related = op.additional_info().related_merge_region();
        if schedule_limit == 0 || !op.additional_info().has_related_merge_region() || related == 0 {
            return;
        }
        self.recent_merges
            .insert(op.region_id(), now.saturating_add(60));
        self.recent_merges.insert(related, now.saturating_add(60));
    }
    /// Like Go TTL.Get, equality is still valid; only times after expiry reject.
    pub fn recently_merged(&self, region: u64, now: u64) -> bool {
        self.recent_merges
            .get(&region)
            .is_some_and(|expiry| now <= *expiry)
    }
}
pub(crate) async fn best_location<C: CheckerConfigProvider>(
    ctx: &AffinityContext<'_, C>,
    manager: &mut RuleManager,
    region: &RegionInfo,
    existing: bool,
) -> bool {
    let replica = &ctx.rules.replica;
    let stores = replica.stores.get_stores();
    let fit = if existing {
        manager.fit_region(&stores, region, replica.config, replica.now_unix_nanos)
    } else {
        manager.fit_region_without_cache(&stores, region, replica.config)
    };
    if !fit.is_satisfied() {
        return false;
    }
    for rf in &fit.rule_fits {
        if rf.rule.location_labels.is_empty() {
            continue;
        }
        let constraint =
            LabelConstraintFilter::new("affinity-checker", rf.rule.label_constraints.clone());
        let filters: &[&dyn astersql_pd_schedule_filter::Filter<C::Version>] = &[&constraint];
        let strategy = ReplicaStrategy {
            checker_name: "affinity-checker",
            conf: replica.config,
            stores: &stores,
            location_labels: &rf.rule.location_labels,
            isolation_level: &rf.rule.isolation_level,
            region,
            extra_filters: filters,
            fast_failover: rf.rule.is_witness && ctx.rules.witness_enabled,
            now_unix_nanos: replica.now_unix_nanos,
        };
        let (_, new, temporary) = strategy.get_better_location(region, &fit, rf).await;
        let refs = rf.stores.iter().map(AsRef::as_ref).collect::<Vec<_>>();
        let labels = rf
            .rule
            .location_labels
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        if new != 0
            || temporary
            || !is_region_label_isolation_satisfied(&refs, &labels, &rf.rule.isolation_level)
        {
            return false;
        }
    }
    true
}
/// Virtual placement: retain shared stores and peer IDs, replace only displaced
/// voters, keep learners, and reject count/leader/duplicate mismatches.
pub fn clone_region_with_replace_peer_stores(
    region: &RegionInfo,
    leader: u64,
    voters: &[u64],
) -> Option<RegionInfo> {
    let source: BTreeSet<_> = region.voters().iter().map(|p| p.store_id).collect();
    let target: BTreeSet<_> = voters.iter().copied().collect();
    if source.len() != region.voters().len()
        || target.len() != voters.len()
        || source.len() != target.len()
        || !target.contains(&leader)
    {
        return None;
    }
    let remove = source.difference(&target).copied();
    let add = target.difference(&source).copied();
    let replace: BTreeMap<_, _> = remove.zip(add).collect();
    let mut meta = region.meta().clone();
    for p in &mut meta.peers {
        if p.role != PeerRole::Learner {
            p.store_id = *replace.get(&p.store_id).unwrap_or(&p.store_id)
        }
    }
    let leader = meta
        .peers
        .iter()
        .find(|p| p.store_id == leader && p.role != PeerRole::Learner)?
        .clone();
    Some(RegionInfo::new(
        meta,
        Some(leader),
        region.down_peers().to_vec(),
        region.pending_peers().to_vec(),
    ))
}
