// Copyright 2026 AsterSQL.
//! Ordered placement-rule combination search over canonical core snapshots.
use crate::{PeerRoleType, Rule, match_label_constraints};
use astersql_pd_core::{Peer, RegionInfo, StoreInfo};
use std::{cmp::Ordering, sync::Arc};

/// Peers assigned to one rule, including roles that scheduling must change.
pub struct RuleFit {
    pub rule: Arc<Rule>,
    pub peers: Vec<Peer>,
    pub peers_with_different_role: Vec<Peer>,
    pub isolation_score: f64,
    pub witness_score: i64,
    pub stores: Vec<Arc<StoreInfo>>,
}
impl RuleFit {
    /// Satisfaction concerns peer count and roles; health only orders selection.
    pub fn is_satisfied(&self) -> bool {
        self.peers.len() as i64 == self.rule.count && self.peers_with_different_role.is_empty()
    }
    pub fn contains_store(&self, id: u64) -> bool {
        self.peers.iter().any(|p| p.store_id == id)
    }
    /// Compare count, role conformity, location diversity, then witness load.
    pub fn compare(&self, other: &Self) -> Ordering {
        self.peers
            .len()
            .cmp(&other.peers.len())
            .then_with(|| {
                other
                    .peers_with_different_role
                    .len()
                    .cmp(&self.peers_with_different_role.len())
            })
            .then_with(|| self.isolation_score.total_cmp(&other.isolation_score))
            .then_with(|| other.witness_score.cmp(&self.witness_score))
    }
}
fn strict_role(peer: &Peer, leader: u64, role: &PeerRoleType) -> bool {
    match role {
        PeerRoleType::Voter => !peer.is_learner(),
        PeerRoleType::Leader => peer.id == leader,
        PeerRoleType::Follower => !peer.is_learner() && peer.id != leader,
        PeerRoleType::Learner => peer.is_learner(),
        PeerRoleType::Unknown(_) => false,
    }
}
fn assemble(
    rule: Arc<Rule>,
    selected: &[(&Peer, &Arc<StoreInfo>)],
    leader: u64,
    support_witness: bool,
) -> RuleFit {
    let mut fit = RuleFit {
        rule,
        peers: vec![],
        peers_with_different_role: vec![],
        isolation_score: 0.0,
        witness_score: 0,
        stores: vec![],
    };
    let labels: Vec<_> = fit
        .rule
        .location_labels
        .iter()
        .map(String::as_str)
        .collect();
    for (i, (peer, store)) in selected.iter().enumerate() {
        fit.peers.push((*peer).clone());
        fit.stores.push(Arc::clone(store));
        if !strict_role(peer, leader, &fit.rule.role)
            || (support_witness && peer.is_witness != fit.rule.is_witness)
            || (!support_witness && peer.is_witness)
        {
            fit.peers_with_different_role.push((*peer).clone());
        }
        if support_witness && fit.rule.is_witness {
            fit.witness_score = fit.witness_score.wrapping_add(store.witness_count());
        }
        for (_, other) in &selected[i + 1..] {
            if let Some(level) = store.compare_location(other, &labels) {
                fit.isolation_score += 100f64.powf((labels.len() - level - 1) as f64);
            }
        }
    }
    fit
}
/// Fit one rule. Missing stores and exclusive labels reject candidates. Ties retain
/// the first numeric-bitmask combination of health-descending, ID-ascending peers.
/// This does not assign peers across multiple rules or validate rule configuration.
pub fn fit_rule(
    stores: &[Arc<StoreInfo>],
    region: &RegionInfo,
    rule: Arc<Rule>,
    support_witness: bool,
) -> RuleFit {
    let leader = region.leader().map_or(0, |p| p.id);
    let mut candidates: Vec<_> = region
        .meta()
        .peers
        .iter()
        .filter_map(|peer| {
            let store = stores.iter().find(|s| s.id() == peer.store_id)?;
            (match_label_constraints(Some(store), &rule.label_constraints)
                && (peer.id != leader || !support_witness || !rule.is_witness))
                .then_some((peer, store))
        })
        .collect();
    candidates.sort_by_key(|(p, _)| (std::cmp::Reverse(state_score(region, p.id)), p.id));
    let count = usize::try_from(rule.count)
        .unwrap_or(0)
        .min(candidates.len());
    let mut best = None;
    combinations(
        &candidates,
        candidates.len(),
        count,
        &mut vec![],
        &mut |selected| {
            let fit = assemble(Arc::clone(&rule), selected, leader, support_witness);
            if best.as_ref().is_none_or(|b| fit.compare(b).is_gt()) {
                best = Some(fit);
            }
            true
        },
    );
    best.expect("the empty selection is a combination")
}

/// Ordered assignment of peers to rules and the remaining orphan peers.
pub struct RegionFit {
    pub rule_fits: Vec<RuleFit>,
    pub orphan_peers: Vec<Peer>,
    pub(crate) region_stores: Vec<Arc<StoreInfo>>,
    pub(crate) rules: Vec<Arc<Rule>>,
}
impl RegionFit {
    pub fn is_satisfied(&self) -> bool {
        !self.rule_fits.is_empty()
            && self.rule_fits.iter().all(RuleFit::is_satisfied)
            && self.orphan_peers.is_empty()
    }
}
/// Fit an ordered rule list over canonical core snapshots.
pub fn fit_region(
    stores: &[Arc<StoreInfo>],
    region: &RegionInfo,
    rules: &[Arc<Rule>],
    support_witness: bool,
) -> RegionFit {
    let leader = region.leader().map_or(0, |p| p.id);
    let mut peers: Vec<_> = region.meta().peers.iter().collect();
    peers.sort_by_key(|p| (std::cmp::Reverse(state_score(region, p.id)), p.id));
    let mut worker = FitWorker {
        stores,
        peers,
        leader,
        rules,
        support_witness,
        selected: vec![false; region.meta().peers.len()],
        best: (0..rules.len()).map(|_| None).collect(),
        orphans: vec![],
        need_isolation: rules.iter().any(|r| !r.location_labels.is_empty()),
        exit: false,
    };
    worker.fit(0);
    if rules.is_empty() {
        worker.update_orphans();
    }
    RegionFit {
        rule_fits: worker
            .best
            .into_iter()
            .map(|f| f.expect("every rule has an empty combination"))
            .collect(),
        orphan_peers: worker.orphans,
        region_stores: region
            .meta()
            .peers
            .iter()
            .filter_map(|p| stores.iter().find(|s| s.id() == p.store_id).cloned())
            .collect(),
        rules: rules.to_vec(),
    }
}

// Enumerate fixed-size combinations in colex order, matching Go numeric masks,
// without machine-word shifts or an artificial peer-count limit.
fn combinations<T: Copy>(
    c: &[T],
    end: usize,
    remaining: usize,
    indices: &mut Vec<usize>,
    emit: &mut impl FnMut(&[T]) -> bool,
) -> bool {
    if remaining == 0 {
        let selected: Vec<_> = indices.iter().rev().map(|i| c[*i]).collect();
        return emit(&selected);
    }
    for last in remaining - 1..end {
        indices.push(last);
        let keep_searching = combinations(c, last, remaining - 1, indices, emit);
        indices.pop();
        if !keep_searching {
            return false;
        }
    }
    true
}

fn state_score(region: &RegionInfo, id: u64) -> u8 {
    if region.get_down_peer(id).is_some() {
        0
    } else if region.get_pending_peer(id).is_some() {
        1
    } else {
        2
    }
}

struct FitWorker<'a> {
    stores: &'a [Arc<StoreInfo>],
    peers: Vec<&'a Peer>,
    leader: u64,
    rules: &'a [Arc<Rule>],
    support_witness: bool,
    selected: Vec<bool>,
    best: Vec<Option<RuleFit>>,
    orphans: Vec<Peer>,
    need_isolation: bool,
    exit: bool,
}
impl FitWorker<'_> {
    fn update_orphans(&mut self) {
        self.orphans = self
            .peers
            .iter()
            .zip(&self.selected)
            .filter(|(_, selected)| !**selected)
            .map(|(p, _)| (*p).clone())
            .collect();
    }
    fn fit(&mut self, index: usize) -> bool {
        if self.exit {
            return false;
        }
        if index == self.rules.len() {
            if !self.need_isolation
                && !self.best.is_empty()
                && self.orphans.is_empty()
                && self
                    .best
                    .iter()
                    .all(|f| f.as_ref().is_some_and(RuleFit::is_satisfied))
            {
                self.exit = true;
            }
            return false;
        }
        let rule = Arc::clone(&self.rules[index]);
        let candidates: Vec<_> = self
            .peers
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                let store = self.stores.iter().find(|s| s.id() == p.store_id)?;
                (!self.selected[i]
                    && match_label_constraints(Some(store), &rule.label_constraints)
                    && (p.id != self.leader || !self.support_witness || !rule.is_witness))
                    .then_some((i, *p, store))
            })
            .collect();
        let count = usize::try_from(rule.count)
            .unwrap_or(0)
            .min(candidates.len());
        // Enumerate against a local immutable candidate snapshot; only the selection
        // flags and best suffix mutate while descending through subsequent rules.
        let mut better = false;
        combinations(
            &candidates,
            candidates.len(),
            count,
            &mut vec![],
            &mut |chosen| {
                for (i, _, _) in chosen {
                    self.selected[*i] = true;
                }
                let selected: Vec<_> = chosen.iter().map(|(_, p, s)| (*p, *s)).collect();
                let fit = assemble(
                    Arc::clone(&rule),
                    &selected,
                    self.leader,
                    self.support_witness,
                );
                let cmp = self.best[index]
                    .as_ref()
                    .map_or(Ordering::Greater, |best| fit.compare(best));
                if cmp.is_gt() {
                    self.best[index] = Some(fit);
                    for next in &mut self.best[index + 1..] {
                        *next = None;
                    }
                    self.fit(index + 1);
                    if index + 1 == self.rules.len() {
                        self.update_orphans();
                    }
                    better = true;
                } else if cmp.is_eq() && self.fit(index + 1) {
                    self.best[index] = Some(fit);
                    better = true;
                }
                for (i, _, _) in chosen {
                    self.selected[*i] = false;
                }
                !self.exit
            },
        );
        better
    }
}
