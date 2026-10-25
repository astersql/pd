// Copyright 2026 AsterSQL.
//! Static store constraints over canonical core and placement contracts.
use crate::{Filter, FilterType};
use astersql_pd_core::{ENGINE_KEY, ENGINE_TIFLASH, StoreInfo, equal_fold};
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_placement::{LabelConstraint, LabelConstraintOp, match_label_constraints};
use astersql_pd_schedule_plan::{Status, StatusCode};

fn status(allowed: bool, rejection: StatusCode) -> Status {
    Status::new(if allowed { StatusCode::OK } else { rejection }, &[])
}

/// Reject low-space destinations while permitting evacuation from every source.
#[derive(Clone, Debug)]
pub struct StorageThresholdFilter {
    scope: String,
}
impl StorageThresholdFilter {
    pub fn new(scope: impl Into<String>) -> Self {
        Self {
            scope: scope.into(),
        }
    }
}
impl<V> Filter<V> for StorageThresholdFilter {
    fn scope(&self) -> &str {
        &self.scope
    }
    fn filter_type(&self) -> FilterType {
        FilterType(1)
    }
    fn source(&self, _: &dyn SharedConfigProvider<Version = V>, _: &StoreInfo) -> Status {
        Status::new(StatusCode::OK, &[])
    }
    fn target(&self, conf: &dyn SharedConfigProvider<Version = V>, store: &StoreInfo) -> Status {
        status(
            !store.is_low_space(conf.get_low_space_ratio()),
            StatusCode::STORE_LOW_SPACE,
        )
    }
}

/// Apply the placement conjunction, including its exclusive-label gate, both ways.
#[derive(Clone, Debug)]
pub struct LabelConstraintFilter {
    scope: String,
    constraints: Vec<LabelConstraint>,
}
impl LabelConstraintFilter {
    pub fn new(scope: impl Into<String>, constraints: Vec<LabelConstraint>) -> Self {
        Self {
            scope: scope.into(),
            constraints,
        }
    }
}
impl<V> Filter<V> for LabelConstraintFilter {
    fn scope(&self) -> &str {
        &self.scope
    }
    fn filter_type(&self) -> FilterType {
        FilterType(3)
    }
    fn source(&self, _: &dyn SharedConfigProvider<Version = V>, store: &StoreInfo) -> Status {
        status(
            match_label_constraints(Some(store), &self.constraints),
            StatusCode::STORE_NOT_MATCH_RULE,
        )
    }
    fn target(&self, conf: &dyn SharedConfigProvider<Version = V>, store: &StoreInfo) -> Status {
        self.source(conf, store)
    }
}

/// The Go special-engine input selects TiFlash, using case-sensitive label values.
pub fn special_engines() -> LabelConstraint {
    LabelConstraint {
        key: ENGINE_KEY.into(),
        op: LabelConstraintOp::In,
        values: vec![ENGINE_TIFLASH.into()],
    }
}
/// The Go normal-engine input accepts missing engines and excludes only TiFlash.
pub fn not_special_engines() -> LabelConstraint {
    LabelConstraint {
        op: LabelConstraintOp::NotIn,
        ..special_engines()
    }
}

/// Match a single engine constraint without the placement exclusive-label gate.
#[derive(Clone, Debug)]
pub struct EngineFilter {
    scope: String,
    constraint: LabelConstraint,
}
impl EngineFilter {
    pub fn new(scope: impl Into<String>, constraint: LabelConstraint) -> Self {
        Self {
            scope: scope.into(),
            constraint,
        }
    }
}
impl<V> Filter<V> for EngineFilter {
    fn scope(&self) -> &str {
        &self.scope
    }
    fn filter_type(&self) -> FilterType {
        FilterType(6)
    }
    fn source(&self, _: &dyn SharedConfigProvider<Version = V>, store: &StoreInfo) -> Status {
        status(
            self.constraint.match_store(store),
            StatusCode::STORE_NOT_MATCH_RULE,
        )
    }
    fn target(&self, conf: &dyn SharedConfigProvider<Version = V>, store: &StoreInfo) -> Status {
        self.source(conf, store)
    }
}

/// Store label key for designated scheduling uses.
pub const SPECIAL_USE_KEY: &str = "specialUse";
/// Designated hot-region capacity.
pub const SPECIAL_USE_HOT_REGION: &str = "hotRegion";
/// Designated reserved capacity.
pub const SPECIAL_USE_RESERVED: &str = "reserved";

/// Reject disallowed known special uses; low-space sources can always evacuate.
#[derive(Clone, Debug)]
pub struct SpecialUseFilter {
    scope: String,
    constraint: LabelConstraint,
}
impl SpecialUseFilter {
    /// Unknown allow values have no effect; input order and duplicates are harmless.
    pub fn new(scope: impl Into<String>, allow_uses: &[String]) -> Self {
        Self {
            scope: scope.into(),
            constraint: LabelConstraint {
                key: SPECIAL_USE_KEY.into(),
                op: LabelConstraintOp::In,
                values: [SPECIAL_USE_HOT_REGION, SPECIAL_USE_RESERVED]
                    .into_iter()
                    .filter(|v| !allow_uses.iter().any(|allowed| allowed == v))
                    .map(str::to_owned)
                    .collect(),
            },
        }
    }
}
impl<V> Filter<V> for SpecialUseFilter {
    fn scope(&self) -> &str {
        &self.scope
    }
    fn filter_type(&self) -> FilterType {
        FilterType(7)
    }
    fn source(&self, conf: &dyn SharedConfigProvider<Version = V>, store: &StoreInfo) -> Status {
        status(
            store.is_low_space(conf.get_low_space_ratio()) || !self.constraint.match_store(store),
            StatusCode::STORE_NOT_MATCH_RULE,
        )
    }
    fn target(&self, _: &dyn SharedConfigProvider<Version = V>, store: &StoreInfo) -> Status {
        status(
            !self.constraint.match_store(store),
            StatusCode::STORE_NOT_MATCH_RULE,
        )
    }
}

/// Reject targets sharing an occupied location prefix through the isolation level.
#[derive(Clone, Debug)]
pub struct IsolationFilter {
    scope: String,
    location_labels: Vec<String>,
    constraints: Vec<Vec<String>>,
}
impl IsolationFilter {
    /// Snapshot region locations. An unknown level defaults to the first key in Go.
    ///
    /// Panics, like Go, when stores exist but location_labels is empty.
    pub fn new(
        scope: impl Into<String>,
        isolation_level: &str,
        location_labels: Vec<String>,
        region_stores: &[&StoreInfo],
    ) -> Self {
        let level = location_labels
            .iter()
            .position(|label| label == isolation_level)
            .unwrap_or(0);
        let constraints = region_stores
            .iter()
            .map(|store| {
                (0..=level)
                    .map(|i| store.label_value(&location_labels[i]).to_owned())
                    .collect()
            })
            .collect();
        Self {
            scope: scope.into(),
            location_labels,
            constraints,
        }
    }
}
impl<V> Filter<V> for IsolationFilter {
    fn scope(&self) -> &str {
        &self.scope
    }
    fn filter_type(&self) -> FilterType {
        FilterType(8)
    }
    fn source(&self, _: &dyn SharedConfigProvider<Version = V>, _: &StoreInfo) -> Status {
        Status::new(StatusCode::OK, &[])
    }
    fn target(&self, _: &dyn SharedConfigProvider<Version = V>, store: &StoreInfo) -> Status {
        let matches = self.constraints.iter().any(|prefix| {
            !prefix.is_empty()
                && prefix.iter().enumerate().all(|(i, value)| {
                    equal_fold(store.label_value(&self.location_labels[i]), value)
                })
        });
        status(
            !self.constraints.is_empty() && !matches,
            StatusCode::STORE_NOT_MATCH_ISOLATION,
        )
    }
}

/// Permit hot-region sources, rejecting any slow/stopping/trend-evicted target.
#[derive(Clone, Debug)]
pub struct HotRegionEvictedTargetFilter {
    scope: String,
}
impl HotRegionEvictedTargetFilter {
    pub fn new(scope: impl Into<String>) -> Self {
        Self {
            scope: scope.into(),
        }
    }
}
impl<V> Filter<V> for HotRegionEvictedTargetFilter {
    fn scope(&self) -> &str {
        &self.scope
    }
    fn filter_type(&self) -> FilterType {
        FilterType(9)
    }
    fn source(&self, _: &dyn SharedConfigProvider<Version = V>, _: &StoreInfo) -> Status {
        Status::new(StatusCode::OK, &[])
    }
    fn target(&self, _: &dyn SharedConfigProvider<Version = V>, store: &StoreInfo) -> Status {
        status(
            !(store.evicted_as_slow_store()
                || store.evicted_as_stopping_store()
                || store.is_evicted_as_slow_trend()),
            StatusCode::STORE_REJECT_LEADER,
        )
    }
}
