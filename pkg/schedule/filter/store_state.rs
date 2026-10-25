// Copyright 2026 AsterSQL.
//! Ordered store safety predicates. The host supplies one clock observation;
//! real store-budget checks yield to the host runtime and never reserve tokens.
use std::{cell::Cell, future::Future, pin::Pin, sync::Arc};

use astersql_pd_core::StoreInfo;
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::{REJECT_LEADER, SharedConfigProvider};
use astersql_pd_schedule_plan::{Collector, PlanOption, Resource, Status, StatusCode};
use astersql_pd_storelimit::Type;

use crate::{Filter, FilterType};

/// Source/target safety options, matching Go StoreStateFilter.
/// Each instance is local to its scheduling evaluation, like Go's mutable Reason.
#[derive(Debug)]
pub struct StoreStateFilter {
    pub action_scope: String,
    pub transfer_leader: bool,
    pub move_region: bool,
    pub scatter_region: bool,
    pub allow_fast_failover: bool,
    pub allow_temporary_states: bool,
    pub operator_level: PriorityLevel,
    /// Explicit host clock observation, in Unix nanoseconds.
    pub now_unix_nanos: i128,
    pub reason: Cell<FilterType>,
}
impl Default for StoreStateFilter {
    fn default() -> Self {
        Self {
            action_scope: String::new(),
            transfer_leader: false,
            move_region: false,
            scatter_region: false,
            allow_fast_failover: false,
            allow_temporary_states: false,
            operator_level: PriorityLevel::LOW,
            now_unix_nanos: 0,
            reason: Cell::new(FilterType(0)),
        }
    }
}

#[derive(Clone, Copy)]
enum Condition {
    Removed,
    Down,
    Removing,
    PauseIn,
    PauseOut,
    Slow,
    Stopping,
    Trend,
    Disconnected,
    Busy,
    RemoveLimit,
    AddLimit,
    LeaderLimit,
    Snapshots,
    Pending,
    RejectLeader,
}
use Condition::*;
const LEADER_SOURCE: &[Condition] = &[Removed, Down, PauseOut, Disconnected];
const REGION_SOURCE: &[Condition] = &[Busy, RemoveLimit, Snapshots];
const LEADER_TARGET: &[Condition] = &[
    Removed,
    Removing,
    Down,
    PauseIn,
    Slow,
    Stopping,
    Trend,
    Disconnected,
    Busy,
    RejectLeader,
    LeaderLimit,
];
const REGION_TARGET: &[Condition] = &[
    Removed,
    Removing,
    Down,
    Disconnected,
    Busy,
    AddLimit,
    Snapshots,
    Pending,
];
const FAST_TARGET: &[Condition] = &[Removed, Removing, Down, Disconnected, Busy];

impl StoreStateFilter {
    pub fn scope(&self) -> &str {
        &self.action_scope
    }
    /// Last evaluated condition identity. With no active conditions it is retained.
    pub fn reason(&self) -> FilterType {
        self.reason.get()
    }
    pub fn filter_type(&self) -> FilterType {
        self.reason()
    }

    async fn check<V>(
        &self,
        conditions: &[Condition],
        conf: &dyn SharedConfigProvider<Version = V>,
        store: &StoreInfo,
    ) -> Status {
        for condition in conditions {
            let temporary = self.allow_temporary_states;
            let (failed, reason, code) = match condition {
                Removed => (store.is_removed(), 11, StatusCode::STORE_REMOVED),
                Down => (
                    store.down_time_nanos(self.now_unix_nanos)
                        > conf.get_max_store_down_time().as_nanos(),
                    12,
                    StatusCode::STORE_DOWN,
                ),
                Removing => (store.is_removing(), 13, StatusCode::STORE_REMOVING),
                PauseIn => (
                    !store.allow_leader_transfer_in(),
                    14,
                    StatusCode::STORE_REJECT_LEADER,
                ),
                PauseOut => (
                    !store.allow_leader_transfer_out(),
                    14,
                    StatusCode::STORE_REJECT_LEADER,
                ),
                Slow => (
                    store.evicted_as_slow_store(),
                    15,
                    StatusCode::STORE_REJECT_LEADER,
                ),
                Stopping => (
                    store.evicted_as_stopping_store(),
                    16,
                    StatusCode::STORE_REJECT_LEADER,
                ),
                Trend => (
                    store.is_evicted_as_slow_trend(),
                    25,
                    StatusCode::STORE_REJECT_LEADER,
                ),
                Disconnected => (
                    !temporary && store.is_disconnected(self.now_unix_nanos),
                    17,
                    StatusCode::STORE_DISCONNECTED,
                ),
                Busy => (!temporary && store.is_busy(), 18, StatusCode::STORE_BUSY),
                RemoveLimit => (
                    !temporary
                        && !store
                            .is_available(Type::REMOVE_PEER, self.operator_level)
                            .await,
                    19,
                    StatusCode::STORE_REMOVE_LIMIT_THROTTLED,
                ),
                AddLimit => (
                    !temporary
                        && !store
                            .is_available(Type::ADD_PEER, self.operator_level)
                            .await,
                    20,
                    StatusCode::STORE_ADD_LIMIT_THROTTLED,
                ),
                LeaderLimit => (
                    !temporary
                        && !store
                            .is_available(Type::TRANSFER_LEADER_IN, self.operator_level)
                            .await,
                    21,
                    StatusCode::STORE_TRANSFER_LEADER_IN_LIMIT_THROTTLED,
                ),
                Snapshots => (
                    !temporary
                        && (u64::from(store.sending_snap_count()) > conf.get_max_snapshot_count()
                            || u64::from(store.receiving_snap_count())
                                > conf.get_max_snapshot_count()),
                    22,
                    StatusCode::STORE_SNAPSHOT_THROTTLED,
                ),
                Pending => (
                    !temporary
                        && conf.get_max_pending_peer_count() > 0
                        && store.pending_peer_count() > conf.get_max_pending_peer_count() as i64,
                    23,
                    StatusCode::STORE_PENDING_PEER_THROTTLED,
                ),
                RejectLeader => (
                    conf.check_label_property(REJECT_LEADER, store.labels()),
                    24,
                    StatusCode::STORE_REJECT_LEADER,
                ),
            };
            self.reason
                .set(FilterType(if failed { reason } else { 10 }));
            if failed {
                return Status::new(code, &[]);
            }
        }
        Status::default()
    }
    /// Check leader predicates before peer-removal predicates, stopping on rejection.
    pub async fn source<V>(
        &self,
        conf: &dyn SharedConfigProvider<Version = V>,
        store: &StoreInfo,
    ) -> Status {
        if self.transfer_leader {
            let status = self.check(LEADER_SOURCE, conf, store).await;
            if !status.is_ok() {
                return status;
            }
        }
        if self.move_region {
            return self.check(REGION_SOURCE, conf, store).await;
        }
        Status::default()
    }
    /// Check leader predicates first, then fast-failover/scatter/ordinary peer predicates.
    pub async fn target<V>(
        &self,
        conf: &dyn SharedConfigProvider<Version = V>,
        store: &StoreInfo,
    ) -> Status {
        if self.transfer_leader {
            let status = self.check(LEADER_TARGET, conf, store).await;
            if !status.is_ok() {
                return status;
            }
        }
        if self.move_region {
            return self
                .check(
                    if self.allow_fast_failover || self.scatter_region {
                        FAST_TARGET
                    } else {
                        REGION_TARGET
                    },
                    conf,
                    store,
                )
                .await;
        }
        Status::default()
    }
}

/// Object-safe async companion for real budget queries. Futures use the caller's
/// runtime; synchronous filters are adapted without blocking or changing their API.
pub trait AsyncFilter<V> {
    fn source_async<'a>(
        &'a self,
        conf: &'a dyn SharedConfigProvider<Version = V>,
        store: &'a StoreInfo,
    ) -> Pin<Box<dyn Future<Output = Status> + 'a>>;
    fn target_async<'a>(
        &'a self,
        conf: &'a dyn SharedConfigProvider<Version = V>,
        store: &'a StoreInfo,
    ) -> Pin<Box<dyn Future<Output = Status> + 'a>>;
}
/// Borrow a synchronous filter for inclusion in an asynchronous chain.
pub struct SynchronousFilter<'a, V>(pub &'a dyn Filter<V>);
impl<V> AsyncFilter<V> for SynchronousFilter<'_, V> {
    fn source_async<'a>(
        &'a self,
        conf: &'a dyn SharedConfigProvider<Version = V>,
        store: &'a StoreInfo,
    ) -> Pin<Box<dyn Future<Output = Status> + 'a>> {
        Box::pin(async move { self.0.source(conf, store) })
    }
    fn target_async<'a>(
        &'a self,
        conf: &'a dyn SharedConfigProvider<Version = V>,
        store: &'a StoreInfo,
    ) -> Pin<Box<dyn Future<Output = Status> + 'a>> {
        Box::pin(async move { self.0.target(conf, store) })
    }
}
impl<V> AsyncFilter<V> for StoreStateFilter {
    fn source_async<'a>(
        &'a self,
        conf: &'a dyn SharedConfigProvider<Version = V>,
        store: &'a StoreInfo,
    ) -> Pin<Box<dyn Future<Output = Status> + 'a>> {
        Box::pin(self.source(conf, store))
    }
    fn target_async<'a>(
        &'a self,
        conf: &'a dyn SharedConfigProvider<Version = V>,
        store: &'a StoreInfo,
    ) -> Pin<Box<dyn Future<Output = Status> + 'a>> {
        Box::pin(self.target(conf, store))
    }
}
/// Direction and diagnostic step of an asynchronous candidate selection.
#[derive(Clone, Copy)]
pub enum StoreSelection {
    Source,
    Target,
    UnavailableTarget,
}
/// Preserve ordered filtering, store identity and first-failure diagnostics while
/// mixing synchronous filters with real asynchronous store-state budget checks.
pub async fn select_stores_async<V>(
    stores: &[Arc<StoreInfo>],
    filters: &[&dyn AsyncFilter<V>],
    conf: &dyn SharedConfigProvider<Version = V>,
    mut collector: Option<&mut Collector>,
    direction: StoreSelection,
) -> Vec<Arc<StoreInfo>> {
    let unavailable = matches!(direction, StoreSelection::UnavailableTarget);
    let mut selected = Vec::new();
    for store in stores {
        let mut rejected = false;
        for filter in filters {
            let status = if matches!(direction, StoreSelection::Source) {
                filter.source_async(conf, store).await
            } else {
                filter.target_async(conf, store).await
            };
            if !status.is_ok() {
                rejected = true;
                if let Some(collector) = collector.as_deref_mut() {
                    let resource = Resource::Store(Some(store.clone()));
                    let option = if unavailable {
                        PlanOption::SetResourceWithStep(resource, 2)
                    } else {
                        PlanOption::SetResource(resource)
                    };
                    collector.collect(&[option, PlanOption::SetStatus(status)]);
                }
                break;
            }
        }
        if rejected == unavailable {
            selected.push(store.clone());
        }
    }
    selected
}
