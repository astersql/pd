// Copyright 2026 AsterSQL.
use crate::{GCReadError, GCState, GCStateManager};
use astersql_pd_keypath::NULL_KEYSPACE_ID;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, atomic::Ordering},
};
use tokio_util::sync::CancellationToken;
impl GCStateManager {
    /// Read enabled metadata scopes and Null; unified scopes retain their zero state identity.
    pub async fn get_all_keyspaces_gc_states(
        self: &Arc<Self>,
        exclude_barriers: bool,
    ) -> Result<BTreeMap<u32, GCState>, GCReadError> {
        self.get_all_keyspaces_gc_states_with_cancel(exclude_barriers, &CancellationToken::new())
            .await
    }
    pub async fn get_all_keyspaces_gc_states_with_cancel(
        self: &Arc<Self>,
        exclude: bool,
        cancel: &CancellationToken,
    ) -> Result<BTreeMap<u32, GCState>, GCReadError> {
        let manager = self.clone();
        let call = self.all_states[usize::from(exclude)].call_cancellable(move |execution| async move {
            let cancel = CancellationToken::new();
            let generation = manager.generation();
            let tracker = generation.as_ref().map(|g| g.work.clone()).unwrap_or_default();
            let tracked = if let Some(g) = &generation {
                let _claims = g.flights.lock().unwrap();
                if g.cancel.is_cancelled() { return Err(GCReadError::NotLeader); }
                tracker.track_future(manager.enumerate_gc_states(exclude, &cancel))
            } else {
                tracker.track_future(manager.enumerate_gc_states(exclude, &cancel))
            };
            let read = tracked;
            tokio::select! {biased; _=execution.cancelled()=>Err(GCReadError::Cancelled), result=read=>result}
        });
        tokio::select! {biased; _=cancel.cancelled()=>Err(GCReadError::Cancelled), result=call=>result}
    }
    async fn enumerate_gc_states(
        &self,
        exclude: bool,
        cancel: &CancellationToken,
    ) -> Result<BTreeMap<u32, GCState>, GCReadError> {
        let generation = self.generation();
        let expected = generation.as_ref().map_or(0, |g| g.id);
        let retired = generation
            .as_ref()
            .map(|g| g.cancel.clone())
            .unwrap_or_default();
        let mut states = if exclude && generation.is_some() {
            let states = self
                .shared
                .cache
                .read()
                .unwrap()
                .iter()
                .map(|(id, (txn, gc))| (*id, GCState::pair(*id, *txn, *gc)))
                .collect();
            if self.shared.active.load(Ordering::Acquire) == expected {
                states
            } else {
                BTreeMap::new()
            }
        } else {
            BTreeMap::new()
        };
        let mut actual = BTreeSet::from([NULL_KEYSPACE_ID]);
        if !states.contains_key(&NULL_KEYSPACE_ID) {
            states.insert(
                NULL_KEYSPACE_ID,
                self.read_gc_state_scope(NULL_KEYSPACE_ID, exclude, cancel)
                    .await?,
            );
        }
        let mut start = 0;
        loop {
            let metas = tokio::select! {biased;_=cancel.cancelled()=>return Err(GCReadError::Cancelled),_=retired.cancelled()=>return Err(GCReadError::NotLeader),result=self.keyspaces.load_range(start,100)=>result.map_err(|e|GCReadError::Keyspace(Arc::new(e)))}?;
            if metas.is_empty() {
                break;
            }
            for meta in &metas {
                if cancel.is_cancelled() {
                    return Err(GCReadError::Cancelled);
                }
                if retired.is_cancelled() {
                    return Err(GCReadError::NotLeader);
                }
                if meta.state != 0 {
                    continue;
                }
                let id = meta.id();
                actual.insert(id);
                if meta
                    .config
                    .get("gc_management_type")
                    .is_none_or(|value| value != "keyspace_level")
                {
                    states.insert(
                        id,
                        GCState {
                            keyspace_id: id,
                            is_keyspace_level: false,
                            gc_safe_point: 0,
                            txn_safe_point: 0,
                            gc_barriers: vec![],
                        },
                    );
                } else if !states.contains_key(&id) {
                    states.insert(id, self.read_gc_state_scope(id, exclude, cancel).await?);
                }
            }
            let next = metas
                .last()
                .unwrap()
                .id()
                .checked_add(1)
                .ok_or(GCReadError::NonAdvancingKeyspaceScan)?;
            if next <= start {
                return Err(GCReadError::NonAdvancingKeyspaceScan);
            }
            start = next;
        }
        let _guard = tokio::select! {biased;_=cancel.cancelled()=>return Err(GCReadError::Cancelled),_=retired.cancelled()=>return Err(GCReadError::NotLeader),guard=self.shared.gate.read()=>guard};
        if self.shared.active.load(Ordering::Acquire) != expected {
            return Err(GCReadError::NotLeader);
        }
        if exclude {
            let stale: Vec<_> = states
                .keys()
                .filter(|id| !actual.contains(id))
                .copied()
                .collect();
            let mut cache = self.shared.cache.write().unwrap();
            for id in stale {
                states.remove(&id);
                cache.remove(&id);
            }
        }
        Ok(states)
    }
}

use astersql_pd_storage_domain::{BarrierTime, GCBarrier, GlobalGCBarrier};
use prometheus::{
    Gauge, GaugeVec, IntCounter, IntCounterVec, Opts, Registry,
    core::{Collector, Desc},
    proto::{Gauge as GaugeValue, LabelPair, Metric, MetricFamily, MetricType},
};
use std::{
    collections::HashMap,
    sync::atomic::AtomicBool,
    sync::{Mutex, Weak},
    time::{SystemTime, UNIX_EPOCH},
};
const BARRIER_NAME: &str = "pd_gc_barrier_timestamp_seconds";
const BARRIER_HELP: &str = "Physical Unix timestamp of a valid GC barrier more than 24 hours old, observed by successful transaction safe point advancement.";
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum BarrierMetricScope {
    Keyspace(u32),
    Global,
}
#[derive(Clone)]
struct BarrierMetricEntry {
    name: String,
    barrier: GCBarrier,
    last_warning: Option<BarrierTime>,
}
#[derive(Default)]
struct BarrierMetricData {
    epoch: u64,
    entries: BTreeMap<(BarrierMetricScope, String), BarrierMetricEntry>,
}
#[derive(Default)]
pub(crate) struct BarrierMetrics {
    data: Mutex<BarrierMetricData>,
}
#[derive(Clone, Debug)]
pub(crate) struct BarrierWarning {
    scope: BarrierMetricScope,
    barrier: GCBarrier,
}
impl BarrierWarning {
    pub(crate) fn log(self) {
        tracing::warn!(scope=?self.scope,barrier_id=%self.barrier.barrier_id,barrier_ts=self.barrier.barrier_ts,expiration_time=?self.barrier.expiration_time,"GC barrier timestamp is too old");
    }
}
impl BarrierMetrics {
    pub(crate) fn epoch(&self) -> u64 {
        self.data.lock().unwrap().epoch
    }
    pub(crate) fn clear_metrics(&self) {
        let mut state = self.data.lock().unwrap();
        state.epoch = state.epoch.wrapping_add(1);
        state.entries.clear();
    }
    pub(crate) fn invalidate_keyspace_metrics(&self, id: u32) {
        let mut state = self.data.lock().unwrap();
        state.epoch = state.epoch.wrapping_add(1);
        state
            .entries
            .retain(|(scope, _), _| *scope != BarrierMetricScope::Keyspace(id));
    }
    pub(crate) fn candidate(
        scope: BarrierMetricScope,
        barrier: &GCBarrier,
        now: BarrierTime,
    ) -> bool {
        if matches!(scope, BarrierMetricScope::Keyspace(_)) && barrier.barrier_id == "gc_worker" {
            return false;
        }
        let physical = (barrier.barrier_ts >> 18) as i64;
        let threshold = BarrierTime::from_unix(physical / 1000, (physical % 1000) * 1_000_000)
            .add_nanos(86_400_000_000_000);
        !barrier.is_expired(now) && now > threshold
    }
    pub(crate) fn observe_metrics(
        &self,
        epoch: u64,
        id: u32,
        name: &str,
        local: &[GCBarrier],
        global: &[GlobalGCBarrier],
        now: BarrierTime,
    ) -> Vec<BarrierWarning> {
        let mut state = self.data.lock().unwrap();
        if state.epoch != epoch {
            return vec![];
        }
        let mut warnings = vec![];
        for (scope, barriers) in [
            (BarrierMetricScope::Keyspace(id), local.to_vec()),
            (
                BarrierMetricScope::Global,
                global
                    .iter()
                    .map(|b| GCBarrier {
                        barrier_id: b.barrier_id.clone(),
                        barrier_ts: b.barrier_ts,
                        expiration_time: b.expiration_time,
                    })
                    .collect(),
            ),
        ] {
            let previous: BTreeMap<_, _> = state
                .entries
                .iter()
                .filter(|((s, _), _)| *s == scope)
                .map(|((_, id), entry)| (id.clone(), entry.clone()))
                .collect();
            state.entries.retain(|(s, _), _| *s != scope);
            for barrier in barriers {
                if !Self::candidate(scope, &barrier, now) {
                    continue;
                }
                let old = previous.get(&barrier.barrier_id);
                let mut last = old
                    .filter(|e| !e.barrier.is_expired(now))
                    .and_then(|e| e.last_warning);
                if last.is_none_or(|last| now >= last.add_nanos(600_000_000_000)) {
                    last = Some(now);
                    warnings.push(BarrierWarning {
                        scope,
                        barrier: barrier.clone(),
                    });
                }
                let name = if scope == BarrierMetricScope::Global
                    || scope == BarrierMetricScope::Keyspace(NULL_KEYSPACE_ID)
                {
                    String::new()
                } else {
                    name.to_owned()
                };
                state.entries.insert(
                    (scope, barrier.barrier_id.clone()),
                    BarrierMetricEntry {
                        name,
                        barrier,
                        last_warning: last,
                    },
                );
            }
        }
        warnings
    }
    pub(crate) fn update_metrics(
        &self,
        scope: BarrierMetricScope,
        barrier: &GCBarrier,
        now: BarrierTime,
    ) {
        let mut state = self.data.lock().unwrap();
        let key = (scope, barrier.barrier_id.clone());
        let Some(old) = state.entries.get(&key) else {
            return;
        };
        if old.barrier.is_expired(now) || !Self::candidate(scope, barrier, now) {
            state.entries.remove(&key);
        } else {
            state.entries.get_mut(&key).unwrap().barrier = barrier.clone();
        }
    }
    pub(crate) fn delete_metrics(&self, scope: BarrierMetricScope, id: &str) {
        self.data
            .lock()
            .unwrap()
            .entries
            .remove(&(scope, id.to_owned()));
    }
    pub(crate) fn snapshot_metrics(&self, now: BarrierTime) -> Vec<Metric> {
        let mut state = self.data.lock().unwrap();
        state
            .entries
            .retain(|_, entry| !entry.barrier.is_expired(now));
        state
            .entries
            .iter()
            .map(|((scope, id), entry)| {
                let (scope, keyspace_id) = match scope {
                    BarrierMetricScope::Keyspace(id) => ("keyspace", id.to_string()),
                    BarrierMetricScope::Global => ("global", String::new()),
                };
                let mut metric = Metric::default();
                let labels = [
                    ("barrier_id", id.clone()),
                    ("keyspace_id", keyspace_id),
                    ("keyspace_name", entry.name.clone()),
                    ("scope", scope.into()),
                ]
                .into_iter()
                .map(|(name, value)| {
                    let mut pair = LabelPair::default();
                    pair.set_name(name.into());
                    pair.set_value(value);
                    pair
                })
                .collect();
                metric.set_label(labels);
                let mut gauge = GaugeValue::default();
                gauge.set_value((entry.barrier.barrier_ts >> 18) as f64 / 1000.0);
                metric.set_gauge(gauge);
                metric
            })
            .collect()
    }
}
struct GCMetricsInner {
    barrier_desc: Desc,
    accesses: IntCounterVec,
    children: [IntCounter; 3],
    gauge: GaugeVec,
    gauge_child: Gauge,
    gauge_set: AtomicBool,
    active: Mutex<Weak<BarrierMetrics>>,
    clock: Arc<dyn Fn() -> BarrierTime + Send + Sync>,
}
/// One host registry's forwarding collector; it retains no GC manager or retired generation.
#[derive(Clone)]
pub struct GCMetrics(Arc<GCMetricsInner>);
impl GCMetrics {
    pub fn new() -> Result<Self, prometheus::Error> {
        Self::new_with_clock(Arc::new(|| {
            let elapsed = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default();
            BarrierTime::from_unix(elapsed.as_secs() as i64, elapsed.subsec_nanos() as i64)
        }))
    }
    pub fn new_with_clock(
        clock: Arc<dyn Fn() -> BarrierTime + Send + Sync>,
    ) -> Result<Self, prometheus::Error> {
        let accesses = IntCounterVec::new(
            Opts::new(
                "pd_gc_gc_state_cache_access_total",
                "Counter of GC state cache accesses by result.",
            ),
            &["result"],
        )?;
        let children =
            ["hit", "slow_hit", "miss"].map(|label| accesses.with_label_values(&[label]));
        let gauge = GaugeVec::new(
            Opts::new("pd_gc_gc_safepoint", "The ts of gc safepoint"),
            &["type"],
        )?;
        let gauge_child = gauge.with_label_values(&["gc_safepoint"]);
        Ok(Self(Arc::new(GCMetricsInner {
            barrier_desc: Desc::new(
                BARRIER_NAME.into(),
                BARRIER_HELP.into(),
                vec![
                    "scope".into(),
                    "keyspace_id".into(),
                    "keyspace_name".into(),
                    "barrier_id".into(),
                ],
                HashMap::new(),
            )?,
            accesses,
            children,
            gauge,
            gauge_child,
            gauge_set: AtomicBool::new(false),
            active: Mutex::new(Weak::new()),
            clock,
        })))
    }
    pub fn register(registry: &Registry) -> Result<Self, prometheus::Error> {
        let metrics = Self::new()?;
        registry.register(Box::new(metrics.clone()))?;
        Ok(metrics)
    }
    pub fn collectors(&self) -> impl Iterator<Item = Box<dyn Collector>> {
        std::iter::once(Box::new(self.clone()) as Box<dyn Collector>)
    }
    fn activate_metrics(&self, observations: &Arc<BarrierMetrics>) {
        *self.0.active.lock().unwrap() = Arc::downgrade(observations);
    }
    fn detach_metrics(&self, observations: &Arc<BarrierMetrics>) {
        let mut active = self.0.active.lock().unwrap();
        if active.ptr_eq(&Arc::downgrade(observations)) {
            *active = Weak::new();
        }
    }
    pub(crate) fn cache_metrics(&self, result: usize) {
        self.0.children[result].inc();
    }
    pub(crate) fn gc_point_metrics(&self, target: u64) {
        self.0.gauge_child.set(target as f64);
        self.0.gauge_set.store(true, Ordering::Release);
    }
}
impl Collector for GCMetrics {
    fn desc(&self) -> Vec<&Desc> {
        let mut result = vec![&self.0.barrier_desc];
        result.extend(self.0.accesses.desc());
        result.extend(self.0.gauge.desc());
        result
    }
    fn collect(&self) -> Vec<MetricFamily> {
        let mut result = self.0.accesses.collect();
        if self.0.gauge_set.load(Ordering::Acquire) {
            result.extend(self.0.gauge.collect());
        }
        let active = self.0.active.lock().unwrap().upgrade();
        if let Some(active) = active {
            let samples = active.snapshot_metrics((self.0.clock)());
            if !samples.is_empty() {
                let mut family = MetricFamily::default();
                family.set_name(BARRIER_NAME.into());
                family.set_help(BARRIER_HELP.into());
                family.set_field_type(MetricType::GAUGE);
                family.set_metric(samples);
                result.push(family);
            }
        }
        result
    }
}
impl crate::gc_read::Shared {
    pub(crate) fn activate_gc_metrics(&self) {
        if let Some(metrics) = self.metrics_binding.get() {
            metrics.activate_metrics(&self.barrier_metrics);
        }
    }
    pub(crate) fn retire_gc_metrics(&self) {
        self.barrier_metrics.clear_metrics();
        if let Some(metrics) = self.metrics_binding.get() {
            metrics.detach_metrics(&self.barrier_metrics);
        }
    }
    pub(crate) fn cache_access_metrics(&self, result: usize) {
        if let Some(metrics) = self.metrics_binding.get() {
            metrics.cache_metrics(result);
        }
    }
    pub(crate) fn gc_point_metrics(&self, target: u64) {
        if let Some(metrics) = self.metrics_binding.get() {
            metrics.gc_point_metrics(target);
        }
    }
}
impl GCStateManager {
    pub fn attach_gc_metrics(&self, metrics: GCMetrics) -> Result<(), GCReadError> {
        if let Some(current) = self.shared.metrics_binding.get() {
            if !Arc::ptr_eq(&current.0, &metrics.0) {
                return Err(GCReadError::InvalidArgument {
                    argument: "gc metrics",
                    value: "already bound".into(),
                });
            }
        } else {
            self.shared
                .metrics_binding
                .set(metrics)
                .map_err(|_| GCReadError::InvalidArgument {
                    argument: "gc metrics",
                    value: "already bound".into(),
                })?;
        }
        if self.generation().is_some_and(|g| !g.cancel.is_cancelled()) {
            self.shared.activate_gc_metrics();
        }
        Ok(())
    }
}

pub(crate) struct GCBarrierObservation {
    pub epoch: u64,
    pub id: u32,
    pub name: String,
    pub local: Vec<GCBarrier>,
    pub global: Vec<GlobalGCBarrier>,
    pub now: BarrierTime,
}
impl GCBarrierObservation {
    pub(crate) fn publish_metrics(self, metrics: &BarrierMetrics) -> Vec<BarrierWarning> {
        metrics.observe_metrics(
            self.epoch,
            self.id,
            &self.name,
            &self.local,
            &self.global,
            self.now,
        )
    }
}
