// Copyright 2026 AsterSQL.
//! Placement populations and local expectations for one immutable solver snapshot.
use crate::{HotDestinationScope, HotSourceOperation};
use astersql_pd_core::RegionInfo;
use astersql_pd_schedule_placement::{RegionFit, Rule, match_label_constraints};
use astersql_pd_statistics::{StoreLoad, StoreLoadDetail, StoreLoadSummary};
use std::{collections::HashMap, sync::Arc};

/// Value-derived constraints and engine. Range selection is owned by RegionFit.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PlacementScopeKey {
    pub rule_scope: String,
    pub is_tikv: bool,
}
impl PlacementScopeKey {
    pub fn new(rules: &[&Rule], is_tikv: bool) -> Self {
        fn part(out: &mut String, value: &str) {
            out.push_str(&format!("{}:{value}", value.len()));
        }
        let mut key = format!("{}:", rules.len());
        for rule in rules {
            key.push_str(&format!("{}:", rule.label_constraints.len()));
            for c in &rule.label_constraints {
                part(&mut key, &c.key);
                part(&mut key, &String::from(c.op.clone()));
                key.push_str(&format!("{}:", c.values.len()));
                for value in &c.values {
                    part(&mut key, value);
                }
            }
        }
        Self {
            rule_scope: key,
            is_tikv,
        }
    }
}
/// Local statistics and stable membership, independent of pointer/index identity.
pub struct HotPlacementScope {
    pub expect: StoreLoad,
    pub stddev: StoreLoad,
    store_ids: Vec<u64>,
    population: Vec<u64>,
}
impl HotPlacementScope {
    pub fn store_ids(&self) -> &[u64] {
        &self.store_ids
    }
    pub fn contains(&self, id: u64) -> bool {
        self.store_ids.binary_search(&id).is_ok()
    }
    pub fn destination_scope(&self) -> HotDestinationScope<'_> {
        HotDestinationScope {
            store_ids: &self.store_ids,
            expect: &self.expect,
        }
    }
    /// Bit representation is local to the immutable round; IDs permit safe cross-round comparison.
    pub fn population_words(&self) -> &[u64] {
        &self.population
    }
}
/// Nil represents the complete engine population, as in Go.
pub fn same_hot_placement_population(
    a: Option<&HotPlacementScope>,
    a_tikv: bool,
    b: Option<&HotPlacementScope>,
    b_tikv: bool,
) -> bool {
    a_tikv == b_tikv
        && match (a, b) {
            (None, None) => true,
            (Some(a), Some(b)) => a.store_ids == b.store_ids,
            _ => false,
        }
}
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum HotPlacementError {
    #[error("duplicate store in placement load snapshot: {0}")]
    DuplicateStore(u64),
}
/// A solver round owns its load snapshot and cache. Construct a new round after
/// store labels/loads change; rule edits automatically use new value-derived keys.
/// Deriving the population index from this snapshot prevents stale external indices.
pub struct HotPlacementLoads {
    details: Vec<StoreLoadDetail>,
    cache: HashMap<PlacementScopeKey, Option<Arc<HotPlacementScope>>>,
}
impl HotPlacementLoads {
    pub fn new(mut details: Vec<StoreLoadDetail>) -> Result<Self, HotPlacementError> {
        details.sort_by_key(|d| d.store_summary_info.store_info.id());
        for pair in details.windows(2) {
            let id = pair[0].store_summary_info.store_info.id();
            if id == pair[1].store_summary_info.store_info.id() {
                return Err(HotPlacementError::DuplicateStore(id));
            }
        }
        Ok(Self {
            details,
            cache: HashMap::new(),
        })
    }
    pub fn cached_scopes(&self) -> usize {
        self.cache.len()
    }
    pub fn scope(&mut self, rules: &[&Rule], is_tikv: bool) -> Option<Arc<HotPlacementScope>> {
        if rules.is_empty() {
            return None;
        }
        let key = PlacementScopeKey::new(rules, is_tikv);
        if let Some(scope) = self.cache.get(&key) {
            return scope.clone();
        }
        let mut total = 0;
        let mut selected = Vec::new();
        let mut summary = StoreLoadSummary::default();
        let mut population = vec![0u64; self.details.len().div_ceil(64)];
        for (position, detail) in self.details.iter().enumerate() {
            let store = &detail.store_summary_info.store_info;
            if store.is_tikv() != is_tikv {
                continue;
            }
            total += 1;
            if !rules
                .iter()
                .any(|r| match_label_constraints(Some(store), &r.label_constraints))
            {
                continue;
            }
            population[position / 64] |= 1 << (position % 64);
            summary.add(&detail.load_pred.current);
            selected.push(detail.clone());
        }
        let scope = if !selected.is_empty() && selected.len() < total {
            let (expect, stddev) = summary.result(&selected);
            Some(Arc::new(HotPlacementScope {
                expect,
                stddev,
                population,
                store_ids: selected
                    .iter()
                    .map(|d| d.store_summary_info.store_info.id())
                    .collect(),
            }))
        } else {
            None
        };
        self.cache.insert(key, scope.clone());
        scope
    }
    /// Fit must be computed for this region's active key range by RuleManager.
    /// A missing fit/peer/rule, witness source or stale source label falls back to global statistics.
    pub fn prepare_for_region(
        &mut self,
        region: &RegionInfo,
        fit: Option<&RegionFit>,
        source: &StoreLoadDetail,
        operation: HotSourceOperation,
    ) -> Option<Arc<HotPlacementScope>> {
        let fit = fit?;
        let store = &source.store_summary_info.store_info;
        let peer = region.get_store_peer(store.id())?;
        let source_fit = fit
            .rule_fits
            .iter()
            .find(|f| f.peers.iter().any(|p| p.id == peer.id))?;
        if source_fit.rule.is_witness {
            return None;
        }
        let rules: Vec<&Rule> = if operation == HotSourceOperation::TransferLeader {
            fit.rule_fits
                .iter()
                .filter(|f| f.rule.role == source_fit.rule.role && !f.rule.is_witness)
                .map(|f| f.rule.as_ref())
                .collect()
        } else {
            vec![source_fit.rule.as_ref()]
        };
        if !rules
            .iter()
            .any(|r| match_label_constraints(Some(store), &r.label_constraints))
        {
            return None;
        }
        self.scope(&rules, store.is_tikv())
    }
}
