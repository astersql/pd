// Copyright 2026 AsterSQL.
//! Two-stage cache over immutable domain snapshots. The host serializes mutation.
use crate::{RegionFit, Rule};
use astersql_pd_core::{RegionEpoch, RegionInfo, StoreInfo};
use std::{collections::HashMap, sync::Arc};

#[derive(Default)]
pub struct RegionRuleFitCacheManager {
    regions: HashMap<u64, Entry>,
}
struct Entry {
    epoch: RegionEpoch,
    leader: u64,
    stores: Vec<Arc<StoreInfo>>,
    rules: Vec<Arc<Rule>>,
    hits: u32,
    fit: Option<Arc<RegionFit>>,
}
/// Match Go's region eligibility, including absent epoch and down peers.
pub fn validate_region(region: &RegionInfo) -> bool {
    region.leader().is_some() && region.meta().epoch.is_some() && region.down_peers().is_empty()
}
/// Use an explicit Unix-nanosecond clock for core's disconnected predicate.
pub fn validate_stores(stores: &[Arc<StoreInfo>], now: i128) -> bool {
    stores
        .iter()
        .all(|s| !s.is_removing() && !s.is_disconnected(now))
}
pub fn validate_fit(fit: &RegionFit) -> bool {
    !fit.rules.is_empty() && !fit.region_stores.is_empty() && fit.is_satisfied()
}
fn rules_equal(a: &[Arc<Rule>], b: &[Arc<Rule>]) -> bool {
    a.len() == b.len()
        && a.iter().all(|a| {
            b.iter().any(|b| {
                a.id == b.id
                    && a.group_id == b.group_id
                    && a.version == b.version
                    && a.create_timestamp == b.create_timestamp
            })
        })
}
fn stores_equal(a: &[Arc<StoreInfo>], b: &[Arc<StoreInfo>]) -> bool {
    a.len() == b.len()
        && a.iter().all(|a| {
            b.iter().any(|b| {
                let al = a.meta().map(|m| m.labels.as_slice()).unwrap_or_default();
                let bl = b.meta().map(|m| m.labels.as_slice()).unwrap_or_default();
                a.id() == b.id()
                    && a.state() == b.state()
                    && al.len() == bl.len()
                    && bl.iter().all(|l| {
                        al.iter()
                            .rev()
                            .find(|x| x.key == l.key)
                            .is_some_and(|x| x.value == l.value)
                    })
            })
        })
}
impl RegionRuleFitCacheManager {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn invalid(&mut self, region_id: u64) {
        self.regions.remove(&region_id);
    }
    /// Evict every candidate or promoted result referencing the removed store.
    pub fn remove_store_cache(&mut self, store_id: u64) {
        self.regions
            .retain(|_, e| !e.stores.iter().any(|s| s.id() == store_id));
    }
    /// The boolean denotes an unchanged candidate even before its fit is promoted.
    pub fn check_and_get_cache(
        &self,
        region: &RegionInfo,
        rules: &[Arc<Rule>],
        stores: &[Arc<StoreInfo>],
        now: i128,
    ) -> (bool, Option<Arc<RegionFit>>) {
        if !validate_region(region) || !validate_stores(stores, now) {
            return (false, None);
        }
        let Some(e) = self.regions.get(&region.meta().id) else {
            return (false, None);
        };
        if Some(e.epoch) == region.meta().epoch
            && e.leader == region.leader().unwrap().store_id
            && rules_equal(&e.rules, rules)
            && stores_equal(&e.stores, stores)
        {
            (true, e.fit.clone())
        } else {
            (false, None)
        }
    }
    /// Current stores must be a fresh host snapshot, independently of the fit's stores.
    pub fn set_cache(
        &mut self,
        current: &[Arc<StoreInfo>],
        region: &RegionInfo,
        fit: Arc<RegionFit>,
        now: i128,
    ) {
        if !validate_region(region)
            || !validate_fit(&fit)
            || !validate_stores(&fit.region_stores, now)
        {
            return;
        }
        let live = || {
            fit.region_stores
                .iter()
                .all(|s| current.iter().any(|c| c.id() == s.id() && !c.is_removed()))
        };
        if let Some(e) = self.regions.get_mut(&region.meta().id) {
            e.hits = e.hits.wrapping_add(1);
            if e.hits >= 10 {
                if !live() {
                    self.invalid(region.meta().id);
                    return;
                }
                e.fit = Some(fit);
            }
            return;
        }
        if live() {
            self.regions.insert(
                region.meta().id,
                Entry {
                    epoch: region.meta().epoch.unwrap(),
                    leader: region.leader().unwrap().store_id,
                    stores: fit.region_stores.clone(),
                    rules: fit.rules.clone(),
                    hits: 0,
                    fit: None,
                },
            );
        }
    }
}

impl crate::RuleManager {
    pub fn fit_region_without_cache<
        C: astersql_pd_schedule_config::SharedConfigProvider + ?Sized,
    >(
        &self,
        stores: &[Arc<StoreInfo>],
        region: &RegionInfo,
        conf: &C,
    ) -> Arc<RegionFit> {
        Arc::new(crate::fit_region(
            stores,
            region,
            &self.get_rules_for_apply_region(region),
            conf.is_witness_allowed(),
        ))
    }
    pub fn is_region_fit_cached(
        &self,
        stores: &[Arc<StoreInfo>],
        region: &RegionInfo,
        now: i128,
    ) -> bool {
        let selected: Vec<_> = region
            .meta()
            .peers
            .iter()
            .filter_map(|p| stores.iter().find(|s| s.id() == p.store_id).cloned())
            .collect();
        self.cache
            .check_and_get_cache(
                region,
                &self.get_rules_for_apply_region(region),
                &selected,
                now,
            )
            .0
    }
    /// Like Go, first insertion is explicit; unchanged candidates advance on fit.
    pub fn fit_region<C: astersql_pd_schedule_config::SharedConfigProvider + ?Sized>(
        &mut self,
        stores: &[Arc<StoreInfo>],
        region: &RegionInfo,
        conf: &C,
        now: i128,
    ) -> Arc<RegionFit> {
        let rules = self.get_rules_for_apply_region(region);
        let selected: Vec<_> = region
            .meta()
            .peers
            .iter()
            .filter_map(|p| stores.iter().find(|s| s.id() == p.store_id).cloned())
            .collect();
        let (unchanged, cached) = if conf.is_placement_rules_cache_enabled() {
            self.cache
                .check_and_get_cache(region, &rules, &selected, now)
        } else {
            (false, None)
        };
        if let Some(fit) = cached {
            return fit;
        }
        let fit = Arc::new(crate::fit_region(
            &selected,
            region,
            &rules,
            conf.is_witness_allowed(),
        ));
        if unchanged {
            self.cache.set_cache(stores, region, fit.clone(), now);
        }
        fit
    }
    pub fn set_region_fit_cache(
        &mut self,
        stores: &[Arc<StoreInfo>],
        region: &RegionInfo,
        fit: Arc<RegionFit>,
        now: i128,
    ) {
        self.cache.set_cache(stores, region, fit, now);
    }
    pub fn invalid_cache(&mut self, region_id: u64) {
        self.cache.invalid(region_id);
    }
    pub fn remove_store_cache(&mut self, store_id: u64) {
        self.cache.remove_store_cache(store_id);
    }
}
