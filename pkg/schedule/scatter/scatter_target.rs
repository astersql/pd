// Copyright 2026 AsterSQL.
//! Request-local target planning; selection never commits group accounting.
use astersql_pd_core::{
    ENGINE_KEY, ENGINE_TIFLASH, Peer, PeerRole, RegionInfo, RegionOption, StoreInfo,
};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_filter::{
    DistinctScoreFilter, ExcludedFilter, Filter, LocationPolicy, RuleFitFilter, StoreStateFilter,
};
use astersql_pd_schedule_placement::{PeerRoleType, RegionFit, Rule, fit_region};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

/// Host-owned group selection history, separated by engine and peer/leader use.
/// The host seeds/commits counts; planning borrows them without recording a pick.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScatterCounts {
    counts: BTreeMap<(String, String, u64, bool), u64>,
}
impl ScatterCounts {
    pub fn set(&mut self, group: &str, engine: &str, store: u64, leader: bool, count: u64) {
        self.counts
            .insert((group.into(), engine.into(), store, leader), count);
    }
    pub fn get(&self, group: &str, engine: &str, store: u64, leader: bool) -> u64 {
        self.counts
            .get(&(group.into(), engine.into(), store, leader))
            .copied()
            .unwrap_or(0)
    }
}
/// Peer IDs are retained for peers staying in place and zero for new allocations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScatterTarget {
    pub peers: BTreeMap<u64, Peer>,
    pub leader: u64,
    pub leader_picked_count: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScatterTargetError {
    NoLeader,
    MissingSourceStore(u64),
    NoTargetLeader,
    InvalidPeerLayout,
}
impl std::fmt::Display for ScatterTargetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "scatter target error: {self:?}")
    }
}
impl std::error::Error for ScatterTargetError {}

/// One immutable scheduling snapshot. Rules are the canonical manager's ordered
/// apply-range rules, including default rules even when placement is disabled.
/// Store and peer input order explicitly realizes Go's map iteration order.
pub struct ScatterTargetSelector<'a, C: SharedConfigProvider> {
    pub config: &'a C,
    pub stores: &'a [Arc<StoreInfo>],
    pub rules: &'a [Arc<Rule>],
    pub counts: &'a ScatterCounts,
    pub now_unix_nanos: i128,
}
fn engine(store: &StoreInfo) -> &str {
    if store.label_value(ENGINE_KEY) == ENGINE_TIFLASH {
        ENGINE_TIFLASH
    } else {
        "tikv"
    }
}
fn allow_leader(fit: &RegionFit, peer: &Peer) -> bool {
    !matches!(peer.role, PeerRole::Learner | PeerRole::DemotingVoter)
        && !peer.is_witness
        && fit.get_rule_fit(peer.id).is_some_and(|rf| {
            !rf.rule.is_witness
                && matches!(rf.rule.role, PeerRoleType::Voter | PeerRoleType::Leader)
        })
}
impl<C: SharedConfigProvider> ScatterTargetSelector<'_, C> {
    fn store(&self, id: u64) -> Option<&Arc<StoreInfo>> {
        self.stores.iter().find(|s| s.id() == id)
    }
    /// Plan admin single-region scatter. Callers perform admission/hot/affinity
    /// checks before planning and create/commit an operator after it succeeds.
    /// Avoided stores cannot receive a move; retaining an origin remains a safe
    /// fallback when every candidate is rejected, matching selectNewPeer.
    pub async fn select(
        &self,
        region: &RegionInfo,
        group: &str,
        avoid: &BTreeSet<u64>,
    ) -> Result<ScatterTarget, ScatterTargetError> {
        let leader = region.leader().ok_or(ScatterTargetError::NoLeader)?;
        let mut groups: BTreeMap<&str, Vec<&Peer>> = BTreeMap::new();
        let mut originals = BTreeSet::new();
        for peer in &region.meta().peers {
            if peer.store_id == 0 || !originals.insert(peer.store_id) {
                return Err(ScatterTargetError::InvalidPeerLayout);
            }
            let store = self
                .store(peer.store_id)
                .ok_or(ScatterTargetError::MissingSourceStore(peer.store_id))?;
            groups.entry(engine(store)).or_default().push(peer);
        }
        if !originals.contains(&leader.store_id) {
            return Err(ScatterTargetError::InvalidPeerLayout);
        }
        let old_fit = fit_region(self.stores, region, self.rules, true);
        let mut planned = region.clone();
        let mut selected = BTreeSet::new();
        let mut targets = BTreeMap::new();
        let mut leaders = vec![];
        let state = StoreStateFilter {
            action_scope: "scatter-region".into(),
            move_region: true,
            scatter_region: true,
            operator_level: PriorityLevel::HIGH,
            now_unix_nanos: self.now_unix_nanos,
            ..Default::default()
        };
        // Ordinary peers must run first; special-engine peers never lead.
        let mut order = vec!["tikv"];
        order.extend(groups.keys().copied().filter(|e| *e != "tikv"));
        for eng in order {
            let Some(peers) = groups.get(eng) else {
                continue;
            };
            for peer in peers {
                if selected.contains(&peer.store_id) {
                    if eng == "tikv" && allow_leader(&old_fit, peer) {
                        leaders.push(peer.store_id);
                    }
                    continue;
                }
                loop {
                    let source = self.store(peer.store_id).unwrap();
                    let fit = Arc::new(fit_region(self.stores, &planned, self.rules, true));
                    let guard: Box<dyn Filter<C::Version>> =
                        if self.config.is_placement_rules_enabled() {
                            Box::new(RuleFitFilter::new("scatter-region", fit, source.id()))
                        } else {
                            let co: Vec<_> = planned
                                .meta()
                                .peers
                                .iter()
                                .filter_map(|p| self.store(p.store_id).cloned())
                                .collect();
                            Box::new(DistinctScoreFilter::new(
                                "scatter-region",
                                self.config.get_location_labels(),
                                &co,
                                source,
                                LocationPolicy::Safeguard,
                            ))
                        };
                    let excluded = ExcludedFilter::new(
                        "scatter-region",
                        [],
                        selected.iter().chain(avoid.iter()).copied(),
                    );
                    let max = self
                        .stores
                        .iter()
                        .map(|s| self.counts.get(group, eng, s.id(), false))
                        .max()
                        .unwrap_or(0);
                    let min = self
                        .stores
                        .iter()
                        .map(|s| self.counts.get(group, eng, s.id(), false))
                        .min()
                        .unwrap_or(u64::MAX);
                    let mut best = None;
                    let mut best_count = u64::MAX;
                    for store in self.stores {
                        let count = self.counts.get(group, eng, store.id(), false);
                        if count >= max && max != min {
                            continue;
                        }
                        if engine(store) != eng
                            || !excluded.target(self.config, store).is_ok()
                            || !guard.target(self.config, store).is_ok()
                            || !state.target(self.config, store).await.is_ok()
                        {
                            continue;
                        }
                        if count < best_count {
                            best = Some(store.id());
                            best_count = count;
                        }
                    }
                    let next = if self.counts.get(group, eng, peer.store_id, false) <= best_count {
                        peer.store_id
                    } else {
                        best.unwrap_or(peer.store_id)
                    };
                    selected.insert(next);
                    if let Some(other) = peers
                        .iter()
                        .find(|p| p.store_id == next && next != peer.store_id)
                    {
                        targets.insert(next, (*other).clone());
                        continue;
                    }
                    let target = if next == peer.store_id {
                        (*peer).clone()
                    } else {
                        Peer {
                            id: 0,
                            store_id: next,
                            ..(*peer).clone()
                        }
                    };
                    targets.insert(next, target);
                    selected.insert(peer.store_id);
                    if next != peer.store_id {
                        // Replacement and leader must move together before fitting the next peer.
                        let mut moved = planned.meta().peers.clone();
                        for p in &mut moved {
                            if p.id == peer.id {
                                p.store_id = next;
                            }
                        }
                        let mut new_leader = planned.leader().cloned();
                        if let Some(l) = &mut new_leader {
                            if l.id == peer.id {
                                l.store_id = next;
                            }
                        }
                        planned = planned.clone_with([
                            RegionOption::Peers(moved),
                            RegionOption::Leader(new_leader),
                        ]);
                    }
                    if eng == "tikv" && allow_leader(&old_fit, peer) {
                        leaders.push(next);
                    }
                    break;
                }
            }
        }
        let target_leader = leaders
            .into_iter()
            .min_by_key(|id| self.counts.get(group, "tikv", *id, true))
            .ok_or(ScatterTargetError::NoTargetLeader)?;
        // Membership roles/witnesses are preserved, including reserved peers.
        let mut remaining: Vec<_> = region
            .meta()
            .peers
            .iter()
            .map(|p| (p.role, p.is_witness))
            .collect();
        for (id, p) in &targets {
            if *id == 0 || *id != p.store_id {
                return Err(ScatterTargetError::InvalidPeerLayout);
            }
            let Some(i) = remaining
                .iter()
                .position(|kind| *kind == (p.role, p.is_witness))
            else {
                return Err(ScatterTargetError::InvalidPeerLayout);
            };
            remaining.swap_remove(i);
        }
        if !remaining.is_empty() || targets.len() != region.meta().peers.len() {
            return Err(ScatterTargetError::InvalidPeerLayout);
        }
        Ok(ScatterTarget {
            peers: targets,
            leader: target_leader,
            leader_picked_count: self.counts.get(group, "tikv", target_leader, true),
        })
    }
}
