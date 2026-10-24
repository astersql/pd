// Copyright 2026 AsterSQL.
//! Replacement decisions over an immutable, real region fit.
use crate::{RegionFit, Rule, RuleFit, match_label_constraints};
use astersql_pd_core::StoreInfo;
use std::sync::Arc;

/// Pairwise diversity after replacing every occurrence of the source store.
/// Missing labels follow canonical core location comparison semantics.
pub fn isolation_store_score(
    source: u64,
    destination: &StoreInfo,
    stores: &[Arc<StoreInfo>],
    labels: &[String],
) -> f64 {
    let labels: Vec<_> = labels.iter().map(String::as_str).collect();
    let mut score = 0.0;
    for (i, store) in stores.iter().enumerate() {
        let a = if store.id() == source {
            destination
        } else {
            store
        };
        for store in &stores[i + 1..] {
            let b = if store.id() == source {
                destination
            } else {
                store
            };
            if let Some(level) = a.compare_location(b, &labels) {
                score += 100f64.powf((labels.len() - level - 1) as f64);
            }
        }
    }
    score
}
impl RegionFit {
    fn rule_fit_by_store(&self, id: u64) -> Option<&RuleFit> {
        self.rule_fits.iter().find(|fit| fit.contains_store(id))
    }
    /// Allow a role exchange, or a constrained replacement preserving diversity.
    /// Existing membership and same-role exchanges precede label checks, as in Go.
    pub fn replace(&self, source: u64, destination: Option<&StoreInfo>) -> bool {
        let (Some(destination), Some(fit)) = (destination, self.rule_fit_by_store(source)) else {
            return false;
        };
        if fit.contains_store(destination.id())
            || self
                .rule_fit_by_store(destination.id())
                .is_some_and(|target| target.rule.role == fit.rule.role)
        {
            return true;
        }
        match_label_constraints(Some(destination), &fit.rule.label_constraints)
            && fit.isolation_score
                <= isolation_store_score(
                    source,
                    destination,
                    &fit.stores,
                    &fit.rule.location_labels,
                )
    }
    /// Known region-store occurrences minus desired peers; shortages stay negative.
    pub fn extra_count(&self) -> i64 {
        self.region_stores.len() as i64
            - self.rule_fits.iter().map(|fit| fit.rule.count).sum::<i64>()
    }
    /// Find a selected peer by peer ID, excluding orphan peers.
    pub fn get_rule_fit(&self, id: u64) -> Option<&RuleFit> {
        self.rule_fits
            .iter()
            .find(|fit| fit.peers.iter().any(|peer| peer.id == id))
    }
    /// Known stores in region peer order, including repeated store occurrences.
    pub fn region_stores(&self) -> &[Arc<StoreInfo>] {
        &self.region_stores
    }
    /// Ordered rules used to produce this fit.
    pub fn rules(&self) -> &[Arc<Rule>] {
        &self.rules
    }
}
