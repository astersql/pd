// Copyright 2026 AsterSQL.
//! Replica destinations and sources over host-owned configuration and store snapshots.
use astersql_pd_core::{ENGINE_KEY, RegionInfo, StoreInfo};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_filter::{
    DistinctScoreFilter, ExcludedFilter, Filter, IsolationFilter, LocationPolicy, SpecialUseFilter,
    StorageThresholdFilter, StoreCandidates, StoreStateFilter, isolation_comparer,
    region_score_comparer,
};
use astersql_pd_schedule_placement::{RegionFit, RuleFit, match_label_constraints};
use std::sync::Arc;

/// Borrow one coherent scheduling view. State budget queries use the host runtime.
pub struct ReplicaStrategy<'a, C: SharedConfigProvider> {
    pub checker_name: &'a str,
    pub conf: &'a C,
    pub stores: &'a [Arc<StoreInfo>],
    pub location_labels: &'a [String],
    pub isolation_level: &'a str,
    pub region: &'a RegionInfo,
    pub extra_filters: &'a [&'a dyn Filter<C::Version>],
    pub fast_failover: bool,
    pub now_unix_nanos: i128,
}
impl<C: SharedConfigProvider> ReplicaStrategy<'_, C> {
    fn state(&self, temporary: bool, fast: bool) -> StoreStateFilter {
        StoreStateFilter {
            action_scope: self.checker_name.into(),
            move_region: true,
            allow_temporary_states: temporary,
            allow_fast_failover: fast,
            operator_level: if self.fast_failover {
                PriorityLevel::URGENT
            } else {
                PriorityLevel::HIGH
            },
            now_unix_nanos: self.now_unix_nanos,
            ..Default::default()
        }
    }
    fn top_locations(
        &self,
        stores: Vec<Arc<StoreInfo>>,
        co: &[Arc<StoreInfo>],
        asc: bool,
    ) -> StoreCandidates {
        let labels: Vec<_> = self.location_labels.iter().map(String::as_str).collect();
        let refs: Vec<_> = co.iter().map(Arc::as_ref).collect();
        let mut candidates = StoreCandidates::new(stores);
        candidates.keep_the_top_stores(isolation_comparer(&labels, &refs), asc);
        candidates
    }
    fn pick(&self, candidates: &StoreCandidates, asc: bool) -> u64 {
        candidates
            .pick_the_top_store(region_score_comparer(self.conf), asc)
            .map_or(0, |s| s.id())
    }
    fn store(&self, id: u64) -> Option<&Arc<StoreInfo>> {
        self.stores.iter().find(|s| s.id() == id)
    }
    /// Rank permanent eligibility and diversity first, then temporary state and load.
    /// The boolean distinguishes temporary rejection from permanent lack of targets.
    pub async fn select_store_to_add(
        &self,
        co: &[Arc<StoreInfo>],
        extra: &[&dyn Filter<C::Version>],
    ) -> (u64, bool) {
        let excluded = ExcludedFilter::new(
            self.checker_name,
            [],
            self.region.meta().peers.iter().map(|p| p.store_id),
        );
        let storage = StorageThresholdFilter::new(self.checker_name);
        let special = SpecialUseFilter::new(self.checker_name, &[]);
        let refs: Vec<_> = co.iter().map(Arc::as_ref).collect();
        let isolation = (!self.location_labels.is_empty() && !self.isolation_level.is_empty())
            .then(|| {
                IsolationFilter::new(
                    self.checker_name,
                    self.isolation_level,
                    self.location_labels.to_vec(),
                    &refs,
                )
            });
        let relaxed = self.state(true, false);
        let strict = self.state(false, self.fast_failover);
        let mut eligible = Vec::new();
        for store in self.stores {
            if !excluded.target(self.conf, store).is_ok()
                || !storage.target(self.conf, store).is_ok()
                || !special.target(self.conf, store).is_ok()
                || !relaxed.target(self.conf, store).await.is_ok()
            {
                continue;
            }
            if isolation
                .as_ref()
                .is_some_and(|f| !f.target(self.conf, store).is_ok())
            {
                continue;
            }
            if extra
                .iter()
                .chain(self.extra_filters.iter())
                .any(|f| !f.target(self.conf, store).is_ok())
            {
                continue;
            }
            eligible.push(store.clone());
        }
        let top = self.top_locations(eligible, co, false);
        if top.is_empty() {
            return (0, false);
        }
        let mut healthy = Vec::new();
        for store in top.pick_all() {
            if strict.target(self.conf, store).await.is_ok() {
                healthy.push(store.clone());
            }
        }
        let id = self.pick(&StoreCandidates::new(healthy), true);
        (id, id == 0)
    }
    /// Repair may reduce diversity; a singleton remains in the comparison set.
    pub async fn select_store_to_fix(&self, co: &[Arc<StoreInfo>], old: u64) -> (u64, bool) {
        if co.is_empty() {
            return (0, false);
        }
        let mut co = co.to_vec();
        swap_store_to_first(&mut co, old);
        let remaining = if co.len() > 1 { &co[1..] } else { &co[..] };
        self.select_store_to_add(remaining, &[]).await
    }
    /// Replacement must strictly improve diversity and retain configured isolation.
    pub async fn select_store_to_improve(&self, co: &[Arc<StoreInfo>], old: u64) -> (u64, bool) {
        if co.is_empty() {
            return (0, false);
        }
        let Some(source) = self.store(old) else {
            return (0, false);
        };
        let mut co = co.to_vec();
        swap_store_to_first(&mut co, old);
        let improve = DistinctScoreFilter::new(
            self.checker_name,
            self.location_labels,
            &co,
            source,
            LocationPolicy::Improve,
        );
        self.select_store_to_add(&co[1..], &[&improve]).await
    }
    async fn sources(&self, co: &[Arc<StoreInfo>], temporary: bool) -> Vec<Arc<StoreInfo>> {
        let filter = self.state(temporary, false);
        let mut stores = Vec::new();
        for store in co {
            if filter.source(self.conf, store).await.is_ok() {
                stores.push(store.clone());
            }
        }
        stores
    }
    /// Remove the least diverse eligible source, breaking ties by greatest load.
    pub async fn select_store_to_remove(&self, co: &[Arc<StoreInfo>]) -> u64 {
        self.pick(
            &self.top_locations(self.sources(co, false).await, co, true),
            false,
        )
    }
    /// Retain a temporarily unavailable best source so callers can defer improvement.
    pub async fn select_store_to_remove_with_temp_state(
        &self,
        co: &[Arc<StoreInfo>],
    ) -> (u64, bool) {
        let candidates = self.top_locations(self.sources(co, true).await, co, true);
        if candidates.is_empty() {
            return (0, false);
        }
        let healthy = StoreCandidates::new(self.sources(candidates.pick_all(), false).await);
        let id = self.pick(&healthy, false);
        if id != 0 {
            (id, false)
        } else {
            (self.pick(&candidates, false), true)
        }
    }
    /// Compare only same-engine region stores matching a rule of the selected role.
    pub async fn get_better_location(
        &self,
        region: &RegionInfo,
        fit: &RegionFit,
        rf: &RuleFit,
    ) -> (u64, u64, bool) {
        let stores = get_rule_fit_stores(self.stores, rf);
        let (old, source_temp) = self.select_store_to_remove_with_temp_state(&stores).await;
        if old == 0 {
            return (0, 0, false);
        }
        let Some(source) = self.store(old) else {
            return (0, 0, false);
        };
        let co: Vec<_> = region
            .meta()
            .peers
            .iter()
            .filter_map(|p| self.store(p.store_id))
            .filter(|s| {
                s.label_value(ENGINE_KEY) == source.label_value(ENGINE_KEY)
                    && fit.rules().iter().any(|r| {
                        r.role == rf.rule.role
                            && match_label_constraints(Some(s), &r.label_constraints)
                    })
            })
            .cloned()
            .collect();
        let (new, temp) = self.select_store_to_improve(&co, old).await;
        if source_temp {
            (0, 0, new != 0 || temp)
        } else {
            (old, new, temp)
        }
    }
}
fn swap_store_to_first(stores: &mut [Arc<StoreInfo>], id: u64) {
    if let Some(index) = stores.iter().position(|s| s.id() == id) {
        stores.swap(0, index);
    }
}
/// Resolve peers against the current store snapshot, skipping missing stores.
pub fn get_rule_fit_stores(stores: &[Arc<StoreInfo>], rf: &RuleFit) -> Vec<Arc<StoreInfo>> {
    rf.peers
        .iter()
        .filter_map(|p| stores.iter().find(|s| s.id() == p.store_id).cloned())
        .collect()
}
/// Go's post-5.0 compatibility gate accepts every patch/prerelease within 6.6.
/// The host projects its own version into major/minor without a server dependency.
pub fn is_witness_enabled<C: SharedConfigProvider>(
    conf: &C,
    version_parts: impl FnOnce(&C::Version) -> (i64, i64),
) -> bool {
    let (major, minor) = version_parts(conf.get_cluster_version());
    (major, minor) >= (6, 6) && conf.is_witness_allowed()
}
