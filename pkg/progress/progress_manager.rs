// Copyright 2026 AsterSQL.
use crate::{InvalidUpdateInterval, Progress, ProgressIndicator};
use astersql_pd_core::{NodeState, StoreInfo};
use prometheus::{Gauge, GaugeVec, Opts, Registry};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};
use tokio_util::sync::CancellationToken;

const GC_INTERVAL: Duration = Duration::from_secs(120);
const TTL: Duration = Duration::from_secs(30);
type DurationGetter = Arc<dyn Fn() -> Duration + Send + Sync>;
struct State {
    active: BTreeMap<u64, ProgressIndicator>,
    completed: BTreeMap<u64, ProgressIndicator>,
    getter: DurationGetter,
    series: BTreeMap<(u64, String), [Gauge; 3]>,
}
/// Manager owns store progress snapshots and host-registered metrics.
/// Updates, reset, and GC are serialized; returned values are detached copies.
pub struct Manager {
    state: Mutex<State>,
    interval: Duration,
    metrics: [GaugeVec; 3],
}
impl Manager {
    /// Creates a manager without spawning a task or installing global metrics.
    pub fn new(
        getter: DurationGetter,
        interval: Duration,
        registry: &Registry,
    ) -> Result<Self, prometheus::Error> {
        if interval.is_zero() {
            return Err(prometheus::Error::Msg(InvalidUpdateInterval.to_string()));
        }
        let mut metrics = Vec::new();
        for (name, help) in [
            ("progress", "The current progress of corresponding action"),
            ("speed", "The current speed of corresponding action"),
            ("eta", "The ETA of corresponding action"),
        ] {
            metrics.push(GaugeVec::new(
                Opts::new(format!("pd_cluster_{name}"), help),
                &["address", "store", "action"],
            )?);
        }
        // Roll back only our collectors on partial registration failure.
        for (i, metric) in metrics.iter().enumerate() {
            if let Err(error) = registry.register(Box::new(metric.clone())) {
                for registered in &metrics[..i] {
                    registry.unregister(Box::new(registered.clone()))?;
                }
                return Err(error);
            }
        }
        Ok(Self {
            state: Mutex::new(State {
                active: BTreeMap::new(),
                completed: BTreeMap::new(),
                getter,
                series: BTreeMap::new(),
            }),
            interval,
            metrics: metrics.try_into().expect("three metrics"),
        })
    }
    /// Replaces the dynamic patrol-duration source for subsequent removing samples.
    pub fn set_patrol_regions_duration_getter(&self, getter: DurationGetter) {
        self.state.lock().unwrap().getter = getter;
    }
    /// Resets active progress and all series, retaining completed entries like Go.
    pub fn reset(&self) {
        let mut state = self.state.lock().unwrap();
        for metric in &self.metrics {
            metric.reset();
        }
        state.active.clear();
        state.series.clear();
    }
    /// Returns an independent active snapshot; completed and absent IDs return None.
    pub fn get_progress_by_store_id(&self, id: u64) -> Option<Progress> {
        self.state
            .lock()
            .unwrap()
            .active
            .get(&id)
            .map(|p| p.progress().clone())
    }
    /// Returns a completed snapshot retained until GC, useful for lifecycle inspection.
    pub fn completed_progress_by_store_id(&self, id: u64) -> Option<Progress> {
        self.state
            .lock()
            .unwrap()
            .completed
            .get(&id)
            .map(|p| p.progress().clone())
    }
    /// Averages active stores of an action, saturating an infinite ETA sum.
    pub fn get_average_progress_by_action(&self, action: &str) -> Option<Progress> {
        let state = self.state.lock().unwrap();
        let mut count = 0;
        let (mut percent, mut speed, mut eta) = (0.0, 0.0, 0.0);
        for p in state
            .active
            .values()
            .map(|p| p.progress())
            .filter(|p| p.action == action)
        {
            count += 1;
            percent += p.progress_percent;
            speed += p.current_speed;
            eta += p.left_second;
        }
        if count == 0 {
            return None;
        }
        let n = count as f64;
        Some(Progress {
            action: action.into(),
            progress_percent: percent / n,
            current_speed: speed / n,
            left_second: if eta == f64::INFINITY {
                f64::MAX
            } else {
                eta / n
            },
        })
    }
    /// Updates a store using the system completion clock; absent stores are ignored.
    pub fn update_progress(&self, store: Option<&StoreInfo>, current: f64, threshold: f64) {
        self.update_progress_at(store, current, threshold, SystemTime::now());
    }
    /// Updates using a host-supplied clock, leaving sample intervals fixed.
    pub fn update_progress_at(
        &self,
        store: Option<&StoreInfo>,
        mut current: f64,
        threshold: f64,
        now: SystemTime,
    ) {
        let Some(store) = store else { return };
        let id = store.id();
        let mut state = self.state.lock().unwrap();
        if state.active.get(&id).is_some_and(|p| {
            (p.progress().action == "preparing" && !store.is_preparing())
                || (p.progress().action == "removing" && !store.is_removing())
        }) {
            let mut p = state.active.remove(&id).unwrap();
            p.push_at(p.target_region_size(), now);
            state.completed.insert(id, p);
        }
        let (action, target) = match store.node_state() {
            NodeState::Preparing => ("preparing", threshold),
            NodeState::Removing => {
                if let Some(p) = state
                    .active
                    .get(&id)
                    .filter(|p| p.progress().action == "removing")
                {
                    current = (p.target_region_size() - current).max(0.0);
                    ("removing", 0.0)
                } else {
                    let target = current;
                    current = 0.0;
                    ("removing", target)
                }
            }
            _ => return,
        };
        if !state.active.contains_key(&id) {
            state.active.insert(
                id,
                ProgressIndicator::new(action, current, target, self.interval)
                    .expect("validated interval"),
            );
            return;
        }
        let duration = if action == "removing" {
            Some((state.getter)())
        } else {
            None
        };
        let p = state.active.get_mut(&id).unwrap();
        p.raise_target(target);
        if let Some(duration) = duration {
            p.adjust_window_length(duration);
        }
        p.push_at(current, now);
        let p = p.progress();
        let label = id.to_string();
        let values = [p.progress_percent, p.current_speed, p.left_second];
        let handles = state.series.entry((id, action.into())).or_insert_with(|| {
            self.metrics
                .each_ref()
                .map(|metric| metric.with_label_values(&["", &label, action]))
        });
        for (metric, value) in handles.iter().zip(values) {
            metric.set(value);
        }
    }
    /// Deletes completed entries strictly older than 30 seconds and their series.
    pub fn gc_completed_progress_at(&self, now: SystemTime) -> usize {
        let mut state = self.state.lock().unwrap();
        let before = state.completed.len();
        let State {
            completed, series, ..
        } = &mut *state;
        completed.retain(|id, p| {
            let expired = p
                .complete_at()
                .is_some_and(|at| now.duration_since(at).is_ok_and(|age| age > TTL));
            if expired && series.remove(&(*id, p.progress().action.clone())).is_some() {
                let label = id.to_string();
                for metric in &self.metrics {
                    metric
                        .remove_label_values(&["", &label, &p.progress().action])
                        .expect("fixed label arity");
                }
            }
            !expired
        });
        before - state.completed.len()
    }
    /// Runs GC on the caller's runtime until cancellation. Caller owns and joins it.
    pub async fn gc(&self, cancel: &CancellationToken) {
        let mut ticker =
            tokio::time::interval_at(tokio::time::Instant::now() + GC_INTERVAL, GC_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {biased; _=cancel.cancelled()=>return,_=ticker.tick()=>{self.gc_completed_progress_at(SystemTime::now());}}
        }
    }
}
