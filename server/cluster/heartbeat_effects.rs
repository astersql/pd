// Copyright 2026 AsterSQL.
//! Host-owned heartbeat effect state and submission.
use crate::{RegionHeartbeatResult, prepare_region_heartbeat};
use astersql_pd_core::{BasicCluster, RegionInfo, StaleRegionError};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_ratelimit::{ConcurrentRunner, RunnerError};
use astersql_pd_schedule_config::CheckerConfigProvider;
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_statistics::{LabelStatistics, RegionStatistics};
use astersql_pd_storage_endpoint::{RegionMetaError, StorageEndpoint};
use astersql_pd_syncer::{Downstreams, HistoryError};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

/// Mutable domain state serialized by the host, including live configuration.
pub struct HeartbeatEffectsState<C> {
    pub cluster: BasicCluster,
    pub region_stats: Option<RegionStatistics>,
    pub label_stats: LabelStatistics,
    pub config: C,
    pub rules: RuleManager,
    /// Installed canonical affinity manager; None means this host has no affinity state.
    pub affinity: Option<astersql_pd_schedule_affinity::manager::Manager>,
}
/// Independent runner lanes, started and stopped by the host.
pub struct HeartbeatRunners<'a> {
    pub subtree: &'a ConcurrentRunner,
    pub misc: &'a ConcurrentRunner,
    pub sync: &'a ConcurrentRunner,
}
/// Stable effect identities also used as runner deduplication names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeartbeatEffect {
    UpdateSubTree,
    HandleOverlaps,
    ObserveRegionStats,
    CollectRegionStats,
    SaveRegionToKV,
    SyncRegionToFollower,
}
/// Nonfatal effect failures; admitted root state is never rolled back.
#[derive(Debug)]
pub enum HeartbeatEffectFailure {
    Admission(HeartbeatEffect, RunnerError),
    Cancelled(HeartbeatEffect),
    DeleteRegion(u64, RegionMetaError),
    SaveRegion(u64, RegionMetaError),
    History(HistoryError),
}
type Completion = oneshot::Receiver<Vec<HeartbeatEffectFailure>>;
/// Acceptance and optional completion receipts. Dropping this does not cancel work.
pub struct HeartbeatEffectsReceipt {
    pub heartbeat: RegionHeartbeatResult,
    completions: Vec<(HeartbeatEffect, Result<Completion, RunnerError>)>,
}
impl HeartbeatEffectsReceipt {
    /// Wait for submitted tasks; stop/join the host runners to resolve cancelled tasks.
    /// A keyed pending task replaced by a newer report resolves as Cancelled.
    pub async fn wait(&mut self) -> Vec<HeartbeatEffectFailure> {
        let mut failures = vec![];
        for (kind, completion) in std::mem::take(&mut self.completions) {
            match completion {
                Err(error) => failures.push(HeartbeatEffectFailure::Admission(kind, error)),
                Ok(receiver) => match receiver.await {
                    Ok(errors) => failures.extend(errors),
                    Err(_) => failures.push(HeartbeatEffectFailure::Cancelled(kind)),
                },
            }
        }
        failures
    }
}
/// Accept a region then submit independent side effects without awaiting storage.
/// The caller supplies host time in Unix seconds and optional storage/sync owners.
#[allow(clippy::too_many_arguments)]
pub fn submit_region_heartbeat<C: CheckerConfigProvider + Send + 'static>(
    state: &Arc<Mutex<HeartbeatEffectsState<C>>>,
    region: RegionInfo,
    bucket_enabled: bool,
    runners: &HeartbeatRunners<'_>,
    storage: Option<Arc<StorageEndpoint>>,
    paths: ClusterPaths,
    downstreams: Option<Downstreams>,
    now_unix_seconds: i64,
) -> Result<HeartbeatEffectsReceipt, StaleRegionError> {
    let (result, repair_subtree, collect) = {
        let mut state = state.lock().unwrap();
        let result = prepare_region_heartbeat(&state.cluster, region, bucket_enabled)?
            .commit_root(&mut state.cluster)?;
        let repair = state
            .cluster
            .regions
            .get_region(result.region.meta().id)
            .is_some_and(|origin| origin.get_ref() < 2);
        let collect = state.region_stats.as_ref().is_some_and(|stats| {
            result.guide.save_cache
                || result.guide.save_kv
                || stats.region_stats_need_update(&result.region, &state.config)
        });
        (result, repair, collect)
    };
    let guide = result.guide;
    let id = result.region.meta().id;
    let changed = guide.save_cache || guide.save_kv;
    let mut receipt = HeartbeatEffectsReceipt {
        heartbeat: result,
        completions: vec![],
    };
    // Go's duplicate fast path observes config before repairing a missing subtree.
    if !changed && collect {
        submit_collect(
            &mut receipt,
            state.clone(),
            runners.misc,
            HeartbeatEffect::ObserveRegionStats,
            now_unix_seconds,
        );
    }
    if guide.save_cache || (!changed && repair_subtree) {
        let shared = state.clone();
        let region = receipt.heartbeat.region.clone();
        receipt.enqueue(
            runners.subtree,
            HeartbeatEffect::UpdateSubTree,
            guide.retained || !changed,
            move |cancel| async move {
                if cancel.is_cancelled() {
                    return None;
                }
                shared
                    .lock()
                    .unwrap()
                    .cluster
                    .regions
                    .check_and_put_sub_tree(&region);
                Some(vec![])
            },
        );
    }
    if !changed {
        return Ok(receipt);
    }
    if !receipt.heartbeat.overlaps.is_empty() {
        let shared = state.clone();
        let overlaps = receipt.heartbeat.overlaps.clone();
        receipt.enqueue(
            runners.misc,
            HeartbeatEffect::HandleOverlaps,
            false,
            move |cancel| async move {
                let mut state = shared.lock().unwrap();
                let HeartbeatEffectsState {
                    region_stats,
                    label_stats,
                    rules,
                    affinity,
                    ..
                } = &mut *state;
                if astersql_pd_cluster_utils::handle_overlaps(
                    &overlaps,
                    region_stats.as_mut(),
                    label_stats,
                    rules,
                    affinity.as_mut(),
                    || cancel.is_cancelled(),
                )
                .is_err()
                {
                    return None;
                }
                Some(vec![])
            },
        );
    }
    if collect {
        submit_collect(
            &mut receipt,
            state.clone(),
            runners.misc,
            HeartbeatEffect::CollectRegionStats,
            now_unix_seconds,
        );
    }
    if guide.save_kv {
        if let Some(storage) = storage {
            let overlaps = receipt.heartbeat.overlaps.clone();
            let region = receipt.heartbeat.region.clone();
            receipt.enqueue(
                runners.misc,
                HeartbeatEffect::SaveRegionToKV,
                false,
                move |_cancel| async move {
                    // Independent best-effort deletes, followed by save, even after errors.
                    // Runner cancellation drops in-flight I/O; admission is never undone.
                    let mut failures = vec![];
                    for overlap in overlaps {
                        if let Err(error) = storage.delete_region(&paths, overlap.meta()).await {
                            failures.push(HeartbeatEffectFailure::DeleteRegion(
                                overlap.meta().id,
                                error,
                            ));
                        }
                    }
                    if let Err(error) = storage.save_region(&paths, region.meta()).await {
                        failures.push(HeartbeatEffectFailure::SaveRegion(id, error));
                    }
                    Some(failures)
                },
            );
        }
    }
    if guide.save_kv || guide.need_sync {
        if let Some(downstreams) = downstreams {
            let region = receipt.heartbeat.region.clone();
            let shared = state.clone();
            receipt.enqueue(
                runners.sync,
                HeartbeatEffect::SyncRegionToFollower,
                true,
                move |cancel| async move {
                    if cancel.is_cancelled() {
                        return None;
                    }
                    // Serialize history publication with full snapshot capture/binding.
                    let _state = shared.lock().unwrap();
                    let failures = downstreams
                        .append([region])
                        .err()
                        .map(HeartbeatEffectFailure::History)
                        .into_iter()
                        .collect();
                    downstreams.broadcast(false);
                    Some(failures)
                },
            );
        }
    }
    Ok(receipt)
}

impl HeartbeatEffect {
    fn name(self) -> &'static str {
        match self {
            Self::UpdateSubTree => "UpdateSubTree",
            Self::HandleOverlaps => "HandleOverlaps",
            Self::ObserveRegionStats => "ObserveRegionStatsAsync",
            Self::CollectRegionStats => "CollectRegionStatsAsync",
            Self::SaveRegionToKV => "SaveRegionToKV",
            Self::SyncRegionToFollower => "SyncRegionToFollower",
        }
    }
}
impl HeartbeatEffectsReceipt {
    fn enqueue<F, Fut>(
        &mut self,
        runner: &ConcurrentRunner,
        kind: HeartbeatEffect,
        retained: bool,
        task: F,
    ) where
        F: FnOnce(tokio_util::sync::CancellationToken) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Option<Vec<HeartbeatEffectFailure>>> + Send + 'static,
    {
        let (send, receive) = oneshot::channel();
        let admitted = runner.run_task_with_id(
            self.heartbeat.region.meta().id,
            kind.name(),
            retained,
            move |cancel| async move {
                if let Some(result) = task(cancel).await {
                    let _ = send.send(result);
                }
            },
        );
        self.completions.push((kind, admitted.map(|()| receive)));
    }
}
fn submit_collect<C: CheckerConfigProvider + Send + 'static>(
    receipt: &mut HeartbeatEffectsReceipt,
    shared: Arc<Mutex<HeartbeatEffectsState<C>>>,
    runner: &ConcurrentRunner,
    kind: HeartbeatEffect,
    now: i64,
) {
    let event = receipt.heartbeat.region.clone();
    receipt.enqueue(runner, kind, false, move |cancel| async move {
        let mut state = shared.lock().unwrap();
        let HeartbeatEffectsState {
            cluster,
            region_stats,
            config,
            rules,
            ..
        } = &mut *state;
        if let Some(stats) = region_stats {
            if astersql_pd_cluster_utils::collect(
                Some(cluster),
                &event,
                stats,
                config,
                rules,
                now,
                || cancel.is_cancelled(),
            )
            .is_err()
            {
                return None;
            }
        }
        Some(vec![])
    });
}
