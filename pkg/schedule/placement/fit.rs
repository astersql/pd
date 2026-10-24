// Copyright 2026 AsterSQL.
//! Single-rule peer selection over canonical core snapshots.
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
    let state = |p: &Peer| {
        if region.get_down_peer(p.id).is_some() {
            0
        } else if region.get_pending_peer(p.id).is_some() {
            1
        } else {
            2
        }
    };
    candidates.sort_by_key(|(p, _)| (std::cmp::Reverse(state(p)), p.id));
    let count = usize::try_from(rule.count)
        .unwrap_or(0)
        .min(candidates.len());
    let mut best = None;
    // Enumerate fixed-size combinations in colex order, matching Go numeric masks,
    // without machine-word shifts or an artificial peer-count limit.
    fn visit<'a>(
        c: &[(&'a Peer, &'a Arc<StoreInfo>)],
        end: usize,
        remaining: usize,
        indices: &mut Vec<usize>,
        emit: &mut impl FnMut(&[(&'a Peer, &'a Arc<StoreInfo>)]),
    ) {
        if remaining == 0 {
            let selected: Vec<_> = indices.iter().rev().map(|i| c[*i]).collect();
            emit(&selected);
            return;
        }
        for last in remaining - 1..end {
            indices.push(last);
            visit(c, last, remaining - 1, indices, emit);
            indices.pop();
        }
    }
    visit(
        &candidates,
        candidates.len(),
        count,
        &mut vec![],
        &mut |selected| {
            let fit = assemble(Arc::clone(&rule), selected, leader, support_witness);
            if best.as_ref().is_none_or(|b| fit.compare(b).is_gt()) {
                best = Some(fit);
            }
        },
    );
    best.expect("the empty selection is a combination")
}
