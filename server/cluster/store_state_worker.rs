// Copyright 2026 AsterSQL.
//! Periodic store lifecycle checks on the host executor and admission gate.
use crate::HeartbeatEffectsState;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::CheckerConfigProvider;
use astersql_pd_storage_endpoint::StorageEndpoint;
use std::{
    future::Future,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{runtime::Handle, task::JoinHandle};
use tokio_util::sync::CancellationToken;

/// A host-owned job. Cancellation is followed by join, including any active tick.
pub struct StoreStateWorker {
    cancel: CancellationToken,
    join: JoinHandle<()>,
}
impl StoreStateWorker {
    /// First check follows one full interval, matching Go's ticker.
    pub fn start<F, Fut>(
        executor: &Handle,
        parent: &CancellationToken,
        interval: Duration,
        mut check: F,
    ) -> Self
    where
        F: FnMut() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        assert!(!interval.is_zero());
        let cancel = parent.child_token();
        let token = cancel.clone();
        let join = executor.spawn(async move {
            let mut ticker =
                tokio::time::interval_at(tokio::time::Instant::now() + interval, interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! { biased;
                    _ = token.cancelled() => break,
                    _ = ticker.tick() => { if !token.is_cancelled() { check().await; } }
                }
            }
        });
        Self { cancel, join }
    }
    /// Wait until the last admitted transaction has finished.
    pub async fn stop(self) -> Result<(), tokio::task::JoinError> {
        self.cancel.cancel();
        self.join.await
    }
}
/// Host must serialize this operation with heartbeat and metadata admissions.
/// Errors belong to individual stores; one failed write must not stop the job.
pub async fn check_store_states<C: CheckerConfigProvider>(
    state: &Arc<Mutex<HeartbeatEffectsState<C>>>,
    storage: &StorageEndpoint,
    paths: ClusterPaths,
    now_unix_nanos: i128,
    prepared: bool,
) -> Vec<(u64, String)> {
    use astersql_pd_core::{NodeState, StoreState};
    let actions = {
        let state = state.lock().unwrap();
        state
            .cluster
            .stores
            .get_stores()
            .into_iter()
            .filter_map(|store| {
                let meta = store.meta()?;
                let remove = match meta.node_state {
                    NodeState::Preparing => {
                        let ready = store.uptime_nanos()
                            >= state
                                .config
                                .snapshot()
                                .schedule
                                .max_store_preparing_time
                                .as_nanos()
                            || state.cluster.regions.total_region_count() < 100
                            || (prepared
                                && preparing_threshold(&state, &store).is_some_and(|threshold| {
                                    store.region_size() as f64 >= threshold
                                }));
                        if !ready || store.is_physically_destroyed() {
                            return None;
                        }
                        false
                    }
                    NodeState::Removing => {
                        if store.region_size() != 0 {
                            return None;
                        }
                        false
                    }
                    NodeState::Removed => {
                        if store.down_time_nanos(now_unix_nanos)
                            <= 30 * 24 * 3600 * 1_000_000_000_i64
                        {
                            return None;
                        }
                        true
                    }
                    _ => return None,
                };
                let mut next = store.as_ref().clone();
                if !remove {
                    next.set_store_state(
                        if meta.node_state == NodeState::Preparing {
                            StoreState::Up
                        } else {
                            StoreState::Tombstone
                        },
                        None,
                    );
                }
                Some((next, remove))
            })
            .collect::<Vec<_>>()
    };
    let mut errors = Vec::new();
    for (next, remove) in actions {
        let meta = next.meta().unwrap();
        let result = if remove {
            storage.delete_store_meta(&paths, meta).await
        } else {
            storage.save_store_meta(&paths, meta).await
        };
        if let Err(error) = result {
            errors.push((next.id(), error.to_string()));
            continue;
        }
        let mut state = state.lock().unwrap();
        if remove {
            state.cluster.stores.delete_store(next.id());
        } else {
            // Merge the state option into the current heartbeat view, as Go PutStore does.
            state.cluster.stores.patch_store(next.id(), |current| {
                current.set_store_state(meta.state, None)
            });
        }
        if remove || meta.node_state == NodeState::Removed {
            state.rules.remove_store_cache(next.id());
        }
    }
    errors
}

fn preparing_threshold<C: CheckerConfigProvider>(
    state: &HeartbeatEffectsState<C>,
    store: &astersql_pd_core::StoreInfo,
) -> Option<f64> {
    use astersql_pd_core::get_store_topo_weight;
    use astersql_pd_schedule_placement::match_label_constraints;
    let stores = state.cluster.stores.get_stores();
    let candidates = stores.iter().map(AsRef::as_ref).collect::<Vec<_>>();
    if !state.config.is_placement_rules_enabled() {
        let labels = state
            .config
            .get_location_labels()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        return Some(
            state.cluster.regions.get_region_size_by_range(b"", b"") as f64
                * state.config.get_max_replicas() as f64
                * get_store_topo_weight(
                    store,
                    &candidates,
                    &labels,
                    state.config.get_max_replicas(),
                )
                * 0.9,
        );
    }
    // An uninitialized rule manager is a dependency gap, never a zero threshold.
    if !state.rules.is_initialized() {
        return None;
    }
    let mut boundaries = vec![Vec::new()];
    boundaries.extend(state.rules.get_split_keys(b"", b""));
    boundaries.push(Vec::new());
    let mut threshold = 0.0;
    for range in boundaries.windows(2) {
        for rule in state.rules.get_rules_for_apply_range(&range[0], &range[1]) {
            if !match_label_constraints(Some(store), &rule.label_constraints) {
                continue;
            }
            let eligible = candidates
                .iter()
                .copied()
                .filter(|s| {
                    !s.is_removing()
                        && !s.is_removed()
                        && match_label_constraints(Some(s), &rule.label_constraints)
                })
                .collect::<Vec<_>>();
            let labels = rule
                .location_labels
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            threshold += state
                .cluster
                .regions
                .get_region_size_by_range(&range[0], &range[1]) as f64
                * rule.count as f64
                * get_store_topo_weight(store, &eligible, &labels, rule.count as i64);
        }
    }
    Some(threshold * 0.9)
}
