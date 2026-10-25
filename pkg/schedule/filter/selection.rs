// Copyright 2026 AsterSQL.
//! Store selection preserves caller order and the first diagnostic rejection.
use astersql_pd_core::StoreInfo;
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_plan::{Collector, PlanOption, Resource, Status, StatusCode};
use std::{collections::BTreeSet, sync::Arc};

/// Stable Go filter identity, shared by subsequent filtering/metrics consumers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FilterType(pub i64);
impl FilterType {
    /// Go `excluded` (zero).
    pub const EXCLUDED: Self = Self(0);
}

/// Read-only source and target predicates over one host configuration generation.
/// V is the host's version type; no protobuf or server configuration leaks here.
pub trait Filter<V> {
    /// Scheduler or checker owning this filter.
    fn scope(&self) -> &str;
    /// Stable identity independent of the human-readable scope.
    fn filter_type(&self) -> FilterType;
    /// Evaluate a source store and retain the complete diagnostic status.
    fn source(&self, conf: &dyn SharedConfigProvider<Version = V>, store: &StoreInfo) -> Status;
    /// Evaluate a target store and retain the complete diagnostic status.
    fn target(&self, conf: &dyn SharedConfigProvider<Version = V>, store: &StoreInfo) -> Status;
}

/// Independent source and target exclusions, matched by the full u64 store ID.
#[derive(Clone, Debug)]
pub struct ExcludedFilter {
    scope: String,
    sources: BTreeSet<u64>,
    targets: BTreeSet<u64>,
}
impl ExcludedFilter {
    /// Capture exclusions; empty iterators represent Go nil/empty maps.
    pub fn new(
        scope: impl Into<String>,
        sources: impl IntoIterator<Item = u64>,
        targets: impl IntoIterator<Item = u64>,
    ) -> Self {
        Self {
            scope: scope.into(),
            sources: sources.into_iter().collect(),
            targets: targets.into_iter().collect(),
        }
    }
    fn status(excluded: bool) -> Status {
        Status::new(
            if excluded {
                StatusCode::STORE_ALREADY_HAS_PEER
            } else {
                StatusCode::OK
            },
            &[],
        )
    }
}
impl<V> Filter<V> for ExcludedFilter {
    fn scope(&self) -> &str {
        &self.scope
    }
    fn filter_type(&self) -> FilterType {
        FilterType::EXCLUDED
    }
    fn source(&self, _: &dyn SharedConfigProvider<Version = V>, store: &StoreInfo) -> Status {
        Self::status(self.sources.contains(&store.id()))
    }
    fn target(&self, _: &dyn SharedConfigProvider<Version = V>, store: &StoreInfo) -> Status {
        Self::status(self.targets.contains(&store.id()))
    }
}

pub(crate) fn first_rejection<V>(
    store: &StoreInfo,
    filters: &[&dyn Filter<V>],
    conf: &dyn SharedConfigProvider<Version = V>,
    source: bool,
) -> Option<(usize, Status)> {
    filters.iter().enumerate().find_map(|(i, f)| {
        let status = if source {
            f.source(conf, store)
        } else {
            f.target(conf, store)
        };
        (!status.is_ok()).then_some((i, status))
    })
}

#[derive(Clone, Copy)]
enum Selection {
    Source,
    Target,
    UnavailableTarget,
}
fn select<V>(
    stores: &[Arc<StoreInfo>],
    filters: &[&dyn Filter<V>],
    conf: &dyn SharedConfigProvider<Version = V>,
    mut collector: Option<&mut Collector>,
    selection: Selection,
) -> Vec<Arc<StoreInfo>> {
    let unavailable = matches!(selection, Selection::UnavailableTarget);
    let mut selected = Vec::new();
    for store in stores {
        let rejection =
            first_rejection(store, filters, conf, matches!(selection, Selection::Source));
        let rejected = rejection.is_some();
        if let Some((_, status)) = rejection {
            if let Some(collector) = collector.as_deref_mut() {
                let resource = Resource::Store(Some(store.clone()));
                let option = if unavailable {
                    PlanOption::SetResourceWithStep(resource, 2)
                } else {
                    PlanOption::SetResource(resource)
                };
                collector.collect(&[option, PlanOption::SetStatus(status)]);
            }
        }
        if rejected == unavailable {
            selected.push(store.clone());
        }
    }
    selected
}

/// Select sources passing every filter. Empty filters retain all input handles.
pub fn select_source_stores<V>(
    stores: &[Arc<StoreInfo>],
    filters: &[&dyn Filter<V>],
    conf: &dyn SharedConfigProvider<Version = V>,
    collector: Option<&mut Collector>,
) -> Vec<Arc<StoreInfo>> {
    select(stores, filters, conf, collector, Selection::Source)
}
/// Select targets passing every filter; diagnostic resources use the base step.
pub fn select_target_stores<V>(
    stores: &[Arc<StoreInfo>],
    filters: &[&dyn Filter<V>],
    conf: &dyn SharedConfigProvider<Version = V>,
    collector: Option<&mut Collector>,
) -> Vec<Arc<StoreInfo>> {
    select(stores, filters, conf, collector, Selection::Target)
}
/// Select rejected targets; collect the first rejection with resource step 2.
pub fn select_unavailable_target_stores<V>(
    stores: &[Arc<StoreInfo>],
    filters: &[&dyn Filter<V>],
    conf: &dyn SharedConfigProvider<Version = V>,
    collector: Option<&mut Collector>,
) -> Vec<Arc<StoreInfo>> {
    select(
        stores,
        filters,
        conf,
        collector,
        Selection::UnavailableTarget,
    )
}
