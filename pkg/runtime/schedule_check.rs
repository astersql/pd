// Copyright 2026 AsterSQL.
//! Serialize canonical checker patrol decisions with the term's existing mutations.
use super::*;
use astersql_pd_schedule::Coordinator;
use astersql_pd_schedule_checker::{
    AffinityContext, CheckerDispatchContext, PatrolSource, PatrolStatus, ReplicaFaultContext,
    RuleMissingContext, patrol_scan_limit,
};
use astersql_pd_schedule_config::{CheckerConfigProvider, SharedConfigProvider};
use std::{
    collections::{BTreeSet, VecDeque},
    time::Duration,
};

pub(super) struct PatrolScan {
    metrics: Option<astersql_pd_schedule_checker::CheckerMetrics>,
    cursor: Vec<u8>,
    last: tokio::time::Instant,
    round: tokio::time::Instant,
    status: tokio::sync::watch::Sender<PatrolStatus>,
}
impl PatrolScan {
    pub(super) fn new(
        coordinator: &Coordinator<ProviderSnapshot<()>>,
        metrics: Option<astersql_pd_schedule_checker::CheckerMetrics>,
    ) -> Self {
        let (status, rx) = tokio::sync::watch::channel(PatrolStatus::default());
        coordinator
            .get_checker_controller()
            .lock()
            .unwrap()
            .set_patrol_status(rx);
        Self {
            metrics,
            cursor: vec![],
            last: tokio::time::Instant::now(),
            round: tokio::time::Instant::now(),
            status,
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn tick(
        &mut self,
        r: &mut Resources,
        coordinator: &Coordinator<ProviderSnapshot<()>>,
        cluster: &BasicCluster,
        config: &ProviderSnapshot<()>,
        labels: &astersql_pd_schedule_labeler::RegionLabeler,
        affinity: &mut astersql_pd_schedule_affinity::manager::Manager,
        ids: &mut VecDeque<u64>,
    ) -> Result<(), String> {
        let interval =
            Duration::from_nanos(config.get_patrol_region_interval().as_nanos().max(1) as u64);
        if self.last.elapsed() < interval.max(Duration::from_millis(1)) {
            return Ok(());
        }
        self.last = tokio::time::Instant::now();
        if config.schedule.halt_scheduling {
            return Ok(());
        }
        let scan_start = std::time::Instant::now();
        let count = cluster.regions.total_region_count();
        let limit = patrol_scan_limit(count);
        let regions = cluster
            .regions
            .scan_regions(&self.cursor, &[], limit as isize);
        self.cursor = regions
            .last()
            .map(|r| r.meta().key_range.end_key.clone())
            .unwrap_or_default();
        let checker = coordinator.get_checker_controller();
        let mut work = {
            let checker = checker.lock().unwrap();
            let mut work = checker
                .priority
                .get_priority_regions(config, now().1 as i128)
                .into_iter()
                .map(|id| (id, PatrolSource::Priority))
                .collect::<Vec<_>>();
            work.extend(
                checker
                    .pending_processed_regions()
                    .into_iter()
                    .map(|id| (id, PatrolSource::Pending)),
            );
            work
        };
        if let Some(scope) = &r.metrics {
            let checker = checker.lock().unwrap();
            scope.checkers.observe_patrol_scan(
                scan_start.elapsed(),
                checker.priority.len(),
                checker.pending_processed_regions().len(),
            );
        }
        work.extend(regions.iter().map(|r| (r.meta().id, PatrolSource::Scan)));
        let admission = coordinator.operator_handle();
        let snapshot = r
            .hot
            .snapshot(0, work.iter().map(|(id, _)| *id).collect())
            .await
            .map_err(|e| e.to_string())?;
        // Merge checks consult both the current region and its neighbors.
        let hot = snapshot
            .read
            .values()
            .chain(snapshot.write.values())
            .flatten()
            .map(|peer| peer.region_id)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|id| {
                cluster.regions.get_region(*id).is_some_and(|region| {
                    snapshot.is_region_hot(&region, config.get_hot_region_cache_hits_threshold())
                })
            })
            .collect();
        let mut seen = BTreeSet::new();
        for (id, source) in work {
            if !seen.insert(id) {
                continue;
            }
            let Some(region) = cluster.regions.get_region(id) else {
                checker.lock().unwrap().priority.remove_priority_region(id);
                continue;
            };
            if r.role == ClusterRole::Pd && ids.len() < 64 {
                let (last, count) = r.alloc_count(128).await.map_err(|e| e.to_string())?;
                ids.extend(last - u64::from(count) + 1..=last);
            }
            let ctx = CheckerDispatchContext {
                affinity: AffinityContext {
                    rules: RuleMissingContext {
                        replica: ReplicaFaultContext {
                            config,
                            stores: &cluster.stores,
                            rules: &[],
                            now_unix_nanos: now().1 as i128,
                            supports_conf_change_v2: true,
                        },
                        witness_enabled: config.is_witness_allowed(),
                    },
                    labels,
                    storage: &r.storage,
                    paths: &r.paths,
                    region_exists: true,
                },
                cluster,
                operators: &admission,
                hot_regions: &hot,
            };
            let remote = r.remote_ids.clone();
            let mut state = r.state.lock().unwrap();
            let mut checker = checker.lock().unwrap();
            checker.priority.inspect(
                config,
                &mut state.rules,
                &cluster.stores.get_stores(),
                &region,
                now().1 as i128,
            );
            let result = checker
                .check_patrol_region(
                    &ctx,
                    &mut state.rules,
                    affinity,
                    &region,
                    source,
                    &mut || planner_id(remote.as_deref(), ids, "checker reservation exhausted"),
                )
                .await;
            drop(checker);
            drop(state);
            coordinator
                .get_operator_records()
                .bury_removed(&result.rejected, std::time::Instant::now());
            if !result.errors.is_empty() {
                *r.metrics_error.lock().unwrap() =
                    Some(format!("checker {id}: {:?}", result.errors));
            }
        }
        self.status.send_modify(|s| {
            s.cursor = self.cursor.clone();
            s.interval = interval;
            s.scan_limit = limit;
            s.workers = 1;
            s.ticks += 1;
            if self.cursor.is_empty() {
                s.last_round = self.round.elapsed();
                s.rounds += 1;
            }
        });
        if self.cursor.is_empty() {
            if let Some(scope) = &r.metrics {
                scope.checkers.observe_patrol_round(self.round.elapsed());
            }
            self.round = tokio::time::Instant::now();
        }
        Ok(())
    }
}

impl Drop for PatrolScan {
    fn drop(&mut self) {
        if let Some(metrics) = &self.metrics {
            metrics.observe_patrol_round(Duration::ZERO);
        }
        self.status.send_modify(|s| {
            s.last_round = Duration::ZERO;
            s.workers = 0;
        });
    }
}
