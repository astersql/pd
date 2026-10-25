// Copyright 2026 AsterSQL.
//! Action-specific region predicates over canonical core and placement state.
use astersql_pd_core::{RegionInfo, StoreInfo};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_affinity::manager::Manager;
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_schedule_placement::{RegionFit, RuleManager};
use astersql_pd_schedule_plan::{Collector, PlanOption, Resource, Status, StatusCode};
use astersql_pd_storelimit::Type;
use std::{cell::RefCell, collections::BTreeSet, sync::Arc};

fn status(allowed: bool, rejection: StatusCode) -> Status {
    Status::new(if allowed { StatusCode::OK } else { rejection }, &[])
}
/// Pending peers prevent actions which require complete health.
pub fn is_region_healthy(region: &RegionInfo) -> bool {
    is_region_healthy_allow_pending(region) && region.pending_peers().is_empty()
}
/// Pending peers are allowed; down peers always prevent healthy scheduling.
pub fn is_region_healthy_allow_pending(region: &RegionInfo) -> bool {
    region.down_peers().is_empty()
}
/// Host-owned rule manager, configuration and store generation for one evaluation.
/// Interior borrowing supports the synchronous RegionFilter contract without
/// copying manager state or installing a private runtime.
pub struct RegionReplicationContext<'a, C: SharedConfigProvider> {
    pub conf: &'a C,
    pub stores: &'a [Arc<StoreInfo>],
    pub manager: RefCell<&'a mut RuleManager>,
    /// Unix nanoseconds, supplied by the host for placement-cache health checks.
    pub now: i128,
}
fn replicas_satisfied<C: SharedConfigProvider>(conf: &C, region: &RegionInfo) -> bool {
    region.learners().is_empty() && region.meta().peers.len() as i64 == conf.get_max_replicas()
}
/// Rules use actual canonical fitting; legacy mode requires an exact replica count.
pub fn is_region_replicated<C: SharedConfigProvider>(
    context: &RegionReplicationContext<'_, C>,
    region: &RegionInfo,
) -> bool {
    if context.conf.is_placement_rules_enabled() {
        context
            .manager
            .borrow_mut()
            .fit_region(context.stores, region, context.conf, context.now)
            .is_satisfied()
    } else {
        replicas_satisfied(context.conf, region)
    }
}
/// Region selection returns the full Go diagnostic code and detailed reason.
pub trait RegionFilter {
    fn select(&self, region: &Arc<RegionInfo>) -> Status;
}
/// Reject any pending peer independently of down peers or replication.
pub struct RegionPendingFilter;
impl RegionFilter for RegionPendingFilter {
    fn select(&self, region: &Arc<RegionInfo>) -> Status {
        status(
            region.pending_peers().is_empty(),
            StatusCode::REGION_UNHEALTHY,
        )
    }
}
/// Reject any reported down peer, including zero down duration.
pub struct RegionDownFilter;
impl RegionFilter for RegionDownFilter {
    fn select(&self, region: &Arc<RegionInfo>) -> Status {
        status(region.down_peers().is_empty(), StatusCode::REGION_UNHEALTHY)
    }
}
/// Replication filter retaining the last successful placement fit.
pub struct RegionReplicatedFilter<'a, C: SharedConfigProvider> {
    context: RegionReplicationContext<'a, C>,
    fit: RefCell<Option<Arc<RegionFit>>>,
}
impl<'a, C: SharedConfigProvider> RegionReplicatedFilter<'a, C> {
    pub fn new(context: RegionReplicationContext<'a, C>) -> Self {
        Self {
            context,
            fit: RefCell::new(None),
        }
    }
    /// Rejections and legacy-mode evaluations leave this successful fit intact.
    pub fn get_fit(&self) -> Option<Arc<RegionFit>> {
        self.fit.borrow().clone()
    }
}
impl<C: SharedConfigProvider> RegionFilter for RegionReplicatedFilter<'_, C> {
    fn select(&self, region: &Arc<RegionInfo>) -> Status {
        let context = &self.context;
        if context.conf.is_placement_rules_enabled() {
            let fit = context.manager.borrow_mut().fit_region(
                context.stores,
                region,
                context.conf,
                context.now,
            );
            if !fit.is_satisfied() {
                return Status::new(StatusCode::REGION_NOT_MATCH_RULE, &[]);
            }
            *self.fit.borrow_mut() = Some(fit);
            Status::new(StatusCode::OK, &[])
        } else {
            status(
                replicas_satisfied(context.conf, region),
                StatusCode::REGION_NOT_REPLICATED,
            )
        }
    }
}
/// Balance by size alone; tiny regions are allowed only below 100 total regions.
pub struct RegionEmptyFilter {
    pub total_region_count: i64,
}
impl RegionFilter for RegionEmptyFilter {
    fn select(&self, region: &Arc<RegionInfo>) -> Status {
        status(
            region.stats().approximate_size > 1 || self.total_region_count < 100,
            StatusCode::REGION_EMPTY,
        )
    }
}
/// Reject a witness on this store, without rejecting witnesses on other stores.
pub struct RegionWitnessFilter {
    pub store_id: u64,
}
impl RegionFilter for RegionWitnessFilter {
    fn select(&self, region: &Arc<RegionInfo>) -> Status {
        status(
            region.get_store_witness(self.store_id).is_none(),
            StatusCode::REGION_NOT_MATCH_RULE,
        )
    }
}
/// Capture snapshot-sender admission once at construction, like the Go filter.
pub struct SnapshotSenderFilter {
    senders: BTreeSet<u64>,
}
impl SnapshotSenderFilter {
    /// Real store limiters run on the caller's runtime. Admission reserves no tokens.
    pub async fn new(stores: &[Arc<StoreInfo>], level: PriorityLevel) -> Self {
        let mut senders = BTreeSet::new();
        for store in stores {
            if store.is_available(Type::SEND_SNAPSHOT, level).await && !store.is_busy() {
                senders.insert(store.id());
            }
        }
        Self { senders }
    }
}
impl RegionFilter for SnapshotSenderFilter {
    fn select(&self, region: &Arc<RegionInfo>) -> Status {
        status(
            self.senders
                .contains(&region.leader().map_or(0, |p| p.store_id)),
            StatusCode::REGION_SEND_SNAPSHOT_THROTTLED,
        )
    }
}
/// Reuse affinity's region/label lookup and its regular-scheduling policy.
pub struct AffinityFilter<'a> {
    pub manager: Option<&'a Manager>,
    pub labels: &'a RegionLabeler,
    /// Unix seconds, matching the affinity manager's clock contract.
    pub now: u64,
}
impl RegionFilter for AffinityFilter<'_> {
    fn select(&self, region: &Arc<RegionInfo>) -> Status {
        let allowed = self.manager.is_none_or(|manager| {
            manager
                .get_region_affinity_group_state(self.labels, Some(region), self.now)
                .0
                .is_none_or(|group| group.regular_scheduling_allowed)
        });
        status(allowed, StatusCode::REGION_AFFINITY)
    }
}
/// Keep all passing region handles, preserving order and duplicates.
pub fn select_regions(
    regions: &[Arc<RegionInfo>],
    filters: &[&dyn RegionFilter],
) -> Vec<Arc<RegionInfo>> {
    regions
        .iter()
        .filter(|r| filters.iter().all(|f| f.select(r).is_ok()))
        .cloned()
        .collect()
}
/// Return the first passing region. Record only each region's first rejection.
pub fn select_one_region(
    regions: &[Arc<RegionInfo>],
    mut collector: Option<&mut Collector>,
    filters: &[&dyn RegionFilter],
) -> Option<Arc<RegionInfo>> {
    for region in regions {
        let rejected = filters.iter().find_map(|f| {
            let status = f.select(region);
            (!status.is_ok()).then_some(status)
        });
        if let Some(status) = rejected {
            if let Some(collector) = collector.as_deref_mut() {
                collector.collect(&[
                    PlanOption::SetResource(Resource::Region(Some(region.clone()))),
                    PlanOption::SetStatus(status),
                ]);
            }
        } else {
            return Some(region.clone());
        }
    }
    None
}
