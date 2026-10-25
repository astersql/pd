// Copyright 2026 AsterSQL.
//! Placement guards retain immutable core and rule-fit snapshots.
use crate::{Filter, FilterType};
use astersql_pd_core::{Peer, RegionInfo, RegionMeta, RegionOption, StoreInfo, distinct_score};
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_placement::{RegionFit, RuleManager};
use astersql_pd_schedule_plan::{Status, StatusCode};
use std::sync::Arc;

fn status(allowed: bool, rejection: StatusCode) -> Status {
    Status::new(if allowed { StatusCode::OK } else { rejection }, &[])
}
/// Whether replacing a replica must improve diversity or merely preserve it.
#[derive(Clone, Copy, Debug)]
pub enum LocationPolicy {
    Safeguard,
    Improve,
}
/// Compare hierarchical diversity with a fixed replica set, excluding the source.
pub struct DistinctScoreFilter {
    scope: String,
    labels: Vec<String>,
    stores: Vec<Arc<StoreInfo>>,
    safe_score: f64,
    policy: LocationPolicy,
}
impl DistinctScoreFilter {
    pub fn new(
        scope: impl Into<String>,
        labels: &[String],
        stores: &[Arc<StoreInfo>],
        source: &StoreInfo,
        policy: LocationPolicy,
    ) -> Self {
        // Go allocates len(stores)-1 capacity before removing the source.
        assert!(
            !stores.is_empty(),
            "location filter requires a region store"
        );
        let stores: Vec<_> = stores
            .iter()
            .filter(|s| s.id() != source.id())
            .cloned()
            .collect();
        let refs: Vec<_> = stores.iter().map(Arc::as_ref).collect();
        let names: Vec<_> = labels.iter().map(String::as_str).collect();
        let safe_score = distinct_score(&names, &refs, source);
        Self {
            scope: scope.into(),
            labels: labels.to_vec(),
            stores,
            safe_score,
            policy,
        }
    }
}
impl<V> Filter<V> for DistinctScoreFilter {
    fn scope(&self) -> &str {
        &self.scope
    }
    fn filter_type(&self) -> FilterType {
        FilterType(2)
    }
    fn source(&self, _: &dyn SharedConfigProvider<Version = V>, _: &StoreInfo) -> Status {
        Status::new(StatusCode::OK, &[])
    }
    fn target(&self, _: &dyn SharedConfigProvider<Version = V>, store: &StoreInfo) -> Status {
        let names: Vec<_> = self.labels.iter().map(String::as_str).collect();
        let refs: Vec<_> = self.stores.iter().map(Arc::as_ref).collect();
        let score = distinct_score(&names, &refs, store);
        status(
            match self.policy {
                LocationPolicy::Safeguard => score >= self.safe_score,
                LocationPolicy::Improve => score > self.safe_score,
            },
            StatusCode::STORE_NOT_MATCH_ISOLATION,
        )
    }
}
/// Protect the source rule's label constraints and diversity during replacement.
pub struct RuleFitFilter {
    scope: String,
    fit: Arc<RegionFit>,
    source: u64,
}
impl RuleFitFilter {
    pub fn new(scope: impl Into<String>, fit: Arc<RegionFit>, source: u64) -> Self {
        Self {
            scope: scope.into(),
            fit,
            source,
        }
    }
}
impl<V> Filter<V> for RuleFitFilter {
    fn scope(&self) -> &str {
        &self.scope
    }
    fn filter_type(&self) -> FilterType {
        FilterType(4)
    }
    fn source(&self, _: &dyn SharedConfigProvider<Version = V>, _: &StoreInfo) -> Status {
        Status::new(StatusCode::OK, &[])
    }
    fn target(&self, _: &dyn SharedConfigProvider<Version = V>, store: &StoreInfo) -> Status {
        status(
            self.fit.replace(self.source, Some(store)),
            StatusCode::STORE_NOT_MATCH_RULE,
        )
    }
}
/// Protect leader placement, requiring a non-witness peer unless moving is allowed.
pub struct RuleLeaderFitFilter {
    base: RuleFitFilter,
    region: Arc<RegionInfo>,
    allow_move: bool,
}
impl RuleLeaderFitFilter {
    pub fn new(
        scope: impl Into<String>,
        region: Arc<RegionInfo>,
        fit: Arc<RegionFit>,
        source: u64,
        allow_move: bool,
    ) -> Self {
        Self {
            base: RuleFitFilter::new(scope, fit, source),
            region,
            allow_move,
        }
    }
}
impl<V> Filter<V> for RuleLeaderFitFilter {
    fn scope(&self) -> &str {
        &self.base.scope
    }
    fn filter_type(&self) -> FilterType {
        FilterType(5)
    }
    fn source(&self, _: &dyn SharedConfigProvider<Version = V>, _: &StoreInfo) -> Status {
        Status::new(StatusCode::OK, &[])
    }
    fn target(&self, _: &dyn SharedConfigProvider<Version = V>, store: &StoreInfo) -> Status {
        let eligible = self
            .region
            .get_store_peer(store.id())
            .map_or(self.allow_move, |p| !p.is_witness);
        status(
            eligible && self.base.fit.replace(self.base.source, Some(store)),
            StatusCode::STORE_NOT_MATCH_RULE,
        )
    }
}
/// Witness transfers require an existing peer other than the current leader.
pub struct RuleWitnessFitFilter {
    base: RuleFitFilter,
    region: Arc<RegionInfo>,
}
impl RuleWitnessFitFilter {
    pub fn new(
        scope: impl Into<String>,
        region: Arc<RegionInfo>,
        fit: Arc<RegionFit>,
        source: u64,
    ) -> Self {
        Self {
            base: RuleFitFilter::new(scope, fit, source),
            region,
        }
    }
}
impl<V> Filter<V> for RuleWitnessFitFilter {
    fn scope(&self) -> &str {
        &self.base.scope
    }
    fn filter_type(&self) -> FilterType {
        FilterType(4)
    }
    fn source(&self, _: &dyn SharedConfigProvider<Version = V>, _: &StoreInfo) -> Status {
        Status::new(StatusCode::OK, &[])
    }
    fn target(&self, _: &dyn SharedConfigProvider<Version = V>, store: &StoreInfo) -> Status {
        let eligible = self
            .region
            .get_store_peer(store.id())
            .is_some_and(|p| p.id != self.region.leader().map_or(0, |l| l.id));
        status(
            eligible && self.base.fit.replace(self.base.source, Some(store)),
            StatusCode::STORE_NOT_MATCH_RULE,
        )
    }
}
/// Host-owned inputs needed only while constructing a placement guard.
/// A supplied fit is reused; otherwise the canonical manager fits the real region.
pub struct PlacementGuardContext<'a, C: SharedConfigProvider> {
    pub conf: &'a C,
    pub stores: &'a [Arc<StoreInfo>],
    pub manager: &'a mut RuleManager,
    pub region: Arc<RegionInfo>,
    /// Explicit clock for the manager's existing cache health checks.
    pub now: i128,
}
impl<C: SharedConfigProvider> PlacementGuardContext<'_, C> {
    fn fit(&mut self, fit: Option<Arc<RegionFit>>) -> Arc<RegionFit> {
        fit.unwrap_or_else(|| {
            self.manager
                .fit_region(self.stores, &self.region, self.conf, self.now)
        })
    }
    /// Peer safeguard falls back to location diversity when rules are disabled.
    pub fn safeguard(
        &mut self,
        scope: &str,
        source: &StoreInfo,
        fit: Option<Arc<RegionFit>>,
    ) -> Box<dyn Filter<C::Version>> {
        if self.conf.is_placement_rules_enabled() {
            Box::new(RuleFitFilter::new(scope, self.fit(fit), source.id()))
        } else {
            let stores: Vec<_> = self
                .region
                .meta()
                .peers
                .iter()
                .filter_map(|p| self.stores.iter().find(|s| s.id() == p.store_id).cloned())
                .collect();
            Box::new(DistinctScoreFilter::new(
                scope,
                self.conf.get_location_labels(),
                &stores,
                source,
                LocationPolicy::Safeguard,
            ))
        }
    }
    /// Disabled rules return no leader guard, as in Go's nil filter.
    pub fn leader_safeguard(
        &mut self,
        scope: &str,
        source: u64,
        fit: Option<Arc<RegionFit>>,
        allow_move: bool,
    ) -> Option<RuleLeaderFitFilter> {
        if !self.conf.is_placement_rules_enabled() {
            return None;
        }
        Some(RuleLeaderFitFilter::new(
            scope,
            self.region.clone(),
            self.fit(fit),
            source,
            allow_move,
        ))
    }
    /// Disabled rules return no witness guard.
    pub fn witness_safeguard(
        &mut self,
        scope: &str,
        source: u64,
        fit: Option<Arc<RegionFit>>,
    ) -> Option<RuleWitnessFitFilter> {
        if !self.conf.is_placement_rules_enabled() {
            return None;
        }
        Some(RuleWitnessFitFilter::new(
            scope,
            self.region.clone(),
            self.fit(fit),
            source,
        ))
    }
}
/// Build a temporary fitting region with independently owned peers and leader.
/// Go copies only peer ID/store/role, clearing witness flags; options run afterward.
/// Region ID, epoch, health and statistics start at their constructor defaults.
pub fn create_region_for_rule_fit(
    start_key: &[u8],
    end_key: &[u8],
    peers: &[Peer],
    leader: Option<&Peer>,
    options: impl IntoIterator<Item = RegionOption>,
) -> RegionInfo {
    let peers = peers
        .iter()
        .map(|p| Peer {
            id: p.id,
            store_id: p.store_id,
            role: p.role,
            is_witness: false,
        })
        .collect();
    RegionInfo::new(
        RegionMeta {
            key_range: astersql_pd_core::KeyRange {
                start_key: start_key.to_vec(),
                end_key: end_key.to_vec(),
            },
            peers,
            ..Default::default()
        },
        leader.cloned(),
        vec![],
        vec![],
    )
    .clone_with(options)
}
