// Copyright 2026 AsterSQL.
//! Typed adapters from restored policies to canonical scheduling algorithms.
use super::*;
use astersql_pd_schedule_config::{
    ProviderSnapshot, SchedulerConfigProvider, SharedConfigProvider,
};
use astersql_pd_schedule_filter::{AffinityFilter, RegionReplicationContext};
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_schedule_operator::{BuilderError, ControllerAdmission, OpKind, Operator};
use astersql_pd_schedule_schedulers::*;
use astersql_pd_statistics::StatisticsSnapshot;
use std::{
    cell::RefCell,
    collections::{HashSet, VecDeque},
    time::Duration,
};

pub(super) struct PolicyInputs<'a> {
    pub diagnostic: Option<DiagnosticRecorder>,
    pub diagnostic_recorded: bool,
    pub cluster: &'a BasicCluster,
    pub config: &'a ProviderSnapshot<()>,
    pub rules: &'a mut RuleManager,
    pub statistics: &'a StatisticsSnapshot,
    pub labels: &'a Arc<RegionLabeler>,
    pub ranges: &'a astersql_pd_schedule_keyrange::Manager,
    pub fast: &'a astersql_pd_schedule_operator::FastOperatorInfluence,
    pub admission: &'a ControllerAdmission,
    pub storage: &'a StorageEndpoint,
    pub paths: ClusterPaths,
    pub ids: &'a mut VecDeque<u64>,
    pub hot: &'a mut HotLoadProjection,
}
pub(super) fn allowed(
    policy: &BuiltinScheduler,
    config: &ProviderSnapshot<()>,
    admission: &ControllerAdmission,
) -> bool {
    let count = |kind| admission.operator_count(kind);
    match policy {
        BuiltinScheduler::BalanceRegion(_) | BuiltinScheduler::ShuffleRegion(_) => {
            count(OpKind::REGION) < config.get_region_schedule_limit()
        }
        BuiltinScheduler::BalanceWitness(_) => {
            count(OpKind::WITNESS) < config.get_witness_schedule_limit()
        }
        BuiltinScheduler::RandomMerge(_) => {
            count(OpKind::MERGE) < config.get_merge_schedule_limit()
        }
        BuiltinScheduler::Hot { .. } => {
            count(OpKind::HOT_REGION) < config.get_hot_region_schedule_limit()
        }
        BuiltinScheduler::GrantHot(_) => GrantHotScheduler::is_schedule_allowed(
            count(OpKind::REGION),
            config.get_region_schedule_limit(),
            count(OpKind::LEADER),
            config.get_leader_schedule_limit(),
        ),
        BuiltinScheduler::ShuffleHot(p) => p.is_schedule_allowed(
            [
                count(OpKind::HOT_REGION),
                count(OpKind::REGION),
                count(OpKind::LEADER),
            ],
            config.get_region_schedule_limit(),
            config.get_leader_schedule_limit(),
        ),
        BuiltinScheduler::ScatterRange(_) | BuiltinScheduler::BalanceRange { .. } => true,
        _ => count(OpKind::LEADER) < config.get_leader_schedule_limit(),
    }
}
impl PolicyInputs<'_> {
    pub(super) async fn attempt(
        &mut self,
        policy: &mut BuiltinScheduler,
    ) -> Result<Vec<Arc<Operator>>, String> {
        let nanos = now().1 as i128;
        let mut random = |len: usize| {
            if len == 0 {
                0
            } else {
                rand::Rng::gen_range(&mut rand::thread_rng(), 0..len)
            }
        };
        let mut shuffle = |values: &mut [usize]| {
            use rand::seq::SliceRandom;
            values.shuffle(&mut rand::thread_rng());
        };
        let mut allocate = || {
            self.ids
                .pop_front()
                .ok_or_else(|| BuilderError::Allocation("scheduler reservation exhausted".into()))
        };
        let influence = self.admission.get_op_influence(Some(self.cluster), &[]);
        self.fast.add_to_at(
            self.cluster,
            &influence,
            tokio::time::Instant::now().into_std(),
        );
        let mut context = EvictLeaderContext {
            cluster: self.cluster,
            config: self.config,
            rules: self.rules,
            now_unix_nanos: nanos,
            random_index: &mut random,
            shuffle: &mut shuffle,
        };
        let leader = match policy {
            BuiltinScheduler::EvictLeader(p) => Some(p.schedule(&mut context).await),
            BuiltinScheduler::GrantLeader(p) => Some(p.schedule(&mut context).await),
            BuiltinScheduler::Label(p) => Some(p.schedule(&mut context).await),
            BuiltinScheduler::ShuffleLeader(p) => Some(p.schedule(&mut context).await),
            BuiltinScheduler::ShuffleRegion(p) => {
                Some(p.schedule(&mut context, true, &mut allocate).await)
            }
            BuiltinScheduler::EvictStoppingStore(p) => {
                let (out, error) = p.schedule(&mut context, self.storage, &self.paths).await;
                if let Some(e) = error {
                    return Err(e.to_string());
                }
                Some(out)
            }
            BuiltinScheduler::EvictSlowStore { disk, network } => {
                let (mut out, error) = disk.schedule(&mut context, self.storage, &self.paths).await;
                if let Some(e) = error {
                    return Err(e.to_string());
                }
                let (net, errors) = network
                    .schedule(&mut context, self.storage, &self.paths)
                    .await;
                if !errors.is_empty() {
                    return Err(format!("{errors:?}"));
                }
                out.operators.extend(net.operators);
                out.errors.extend(net.errors);
                Some(out)
            }
            BuiltinScheduler::SlowTrend(p) => {
                let (out, error) = p
                    .schedule(
                        &mut context,
                        self.storage,
                        &self.paths,
                        now().0,
                        self.config.store.is_raft_kv2(),
                        self.config
                            .get_slow_store_evicting_affected_store_ratio_threshold(),
                    )
                    .await;
                if let Some(e) = error {
                    return Err(e.to_string());
                }
                Some(out)
            }
            BuiltinScheduler::TransferWitness(p) => {
                for region in self.cluster.regions.get_regions() {
                    if region.leader().is_some_and(|p| p.is_witness) {
                        p.recv_region_info().try_send(region).ok();
                    }
                }
                let out = p.schedule(&mut context).await;
                return Ok(out.operators);
            }
            _ => None,
        };
        if let Some(out) = leader {
            return Ok(out.operators);
        }
        drop(context);
        if let BuiltinScheduler::BalanceLeader(p) = policy {
            let mut ctx = LeaderBalanceContext {
                cluster: self.cluster,
                config: self.config,
                rules: self.rules,
                affinity: AffinityFilter {
                    manager: None,
                    labels: self.labels,
                    now: now().0 as u64,
                },
                statistics: self.statistics,
                hot_degree: self.config.get_hot_region_cache_hits_threshold(),
                key_ranges: self.ranges,
                influence: &influence,
                now_unix_nanos: nanos,
                random_index: &mut random,
                shuffle: &mut shuffle,
            };
            let out = p
                .schedule(
                    &mut ctx,
                    self.diagnostic.is_some() && !self.diagnostic_recorded,
                )
                .await;
            if let Some(recorder) = self
                .diagnostic
                .as_mut()
                .filter(|_| !self.diagnostic_recorded)
            {
                recorder.set_result_from_plans(
                    &out.operators,
                    &out.plans
                        .iter()
                        .map(|p| p as &dyn std::any::Any)
                        .collect::<Vec<_>>(),
                    now().0 as u64,
                );
                self.diagnostic_recorded = true;
            }
            return Ok(out.operators);
        }
        if let BuiltinScheduler::BalanceWitness(p) = policy {
            let mut ctx = WitnessBalanceContext {
                cluster: self.cluster,
                config: self.config,
                rules: self.rules,
                influence: &influence,
                now_unix_nanos: nanos,
                random_index: &mut random,
                shuffle: &mut shuffle,
                supports_conf_change_v2: true,
            };
            return Ok(p.schedule(&mut ctx, false).await.operators);
        }

        if let BuiltinScheduler::RandomMerge(p) = policy {
            let stores = self.cluster.stores.get_stores();
            let replication = RegionReplicationContext {
                conf: self.config,
                stores: &stores,
                manager: RefCell::new(&mut *self.rules),
                now: nanos,
            };
            let mut ctx = RandomMergeContext {
                cluster: self.cluster,
                replication: &replication,
                labels: Some(self.labels),
                statistics: self.statistics,
                hot_degree: self.config.get_hot_region_cache_hits_threshold(),
                random_index: &mut random,
                shuffle: &mut shuffle,
                choose_previous: rand::random(),
                supports_conf_change_v2: true,
            };
            return p
                .schedule(&mut ctx, &mut allocate)
                .await
                .map(|ops| ops.into_iter().flatten().map(Arc::new).collect())
                .map_err(|e| e.to_string());
        }
        if matches!(
            policy,
            BuiltinScheduler::BalanceRegion(_)
                | BuiltinScheduler::ScatterRange(_)
                | BuiltinScheduler::BalanceRange { .. }
        ) {
            let mut ctx = RegionBalanceContext {
                cluster: self.cluster,
                config: self.config,
                rules: self.rules,
                affinity: AffinityFilter {
                    manager: None,
                    labels: self.labels,
                    now: now().0 as u64,
                },
                statistics: self.statistics,
                hot_degree: self.config.get_hot_region_cache_hits_threshold(),
                key_ranges: self.ranges,
                influence: &influence,
                now_unix_nanos: nanos,
                random_index: &mut random,
                shuffle: &mut shuffle,
                allocate: &mut allocate,
                supports_conf_change_v2: true,
                allow_empty_regions: false,
            };
            return match policy {
                BuiltinScheduler::BalanceRegion(p) => {
                    let out = p
                        .schedule(
                            &mut ctx,
                            self.diagnostic.is_some() && !self.diagnostic_recorded,
                        )
                        .await;
                    if let Some(recorder) = self
                        .diagnostic
                        .as_mut()
                        .filter(|_| !self.diagnostic_recorded)
                    {
                        recorder.set_result_from_plans(
                            &out.operators,
                            &out.plans
                                .iter()
                                .map(|p| p as &dyn std::any::Any)
                                .collect::<Vec<_>>(),
                            now().0 as u64,
                        );
                        self.diagnostic_recorded = true;
                    }
                    Ok(out.operators)
                }
                BuiltinScheduler::ScatterRange(p) => Ok(p
                    .schedule(&mut ctx, self.admission.operator_count(OpKind::RANGE))
                    .await
                    .operators),
                BuiltinScheduler::BalanceRange { scheduler, jobs } => {
                    if !scheduler
                        .check_job(jobs, &ctx, self.admission.operator_count(OpKind::RANGE))
                        .await
                        .map_err(|e| e.to_string())?
                    {
                        return Ok(vec![]);
                    }
                    Ok(scheduler.schedule(&mut ctx).await.operators)
                }
                _ => unreachable!(),
            };
        }
        let params = if let BuiltinScheduler::Hot { config, .. } = policy {
            Some(&*config)
        } else {
            None
        };
        if let Some(params) = params {
            self.hot.update_history_load_config(
                Duration::from_nanos(params.history_sample_duration.as_nanos().max(0) as u64),
                Duration::from_nanos(params.history_sample_interval.as_nanos().max(0) as u64),
            );
        }
        let details = hot_details(self.cluster, self.statistics, self.hot, params);
        let regions = self
            .admission
            .operators()
            .iter()
            .map(|op| op.region_id())
            .collect::<HashSet<_>>();
        match policy {
            BuiltinScheduler::GrantHot(p) => {
                let mut ctx = GrantHotContext {
                    cluster: self.cluster,
                    config: self.config,
                    rules: self.rules,
                    now: nanos,
                    supports_conf_change_v2: true,
                    allocate: &mut allocate,
                    active_regions: &regions,
                };
                Ok(p.schedule(
                    &details,
                    [rand::random(), rand::random(), rand::random()],
                    &mut ctx,
                )
                .await
                .operators)
            }
            BuiltinScheduler::ShuffleHot(p) => {
                let mut ctx = ShuffleHotContext {
                    cluster: self.cluster,
                    config: self.config,
                    rules: self.rules,
                    now: nanos,
                    supports_conf_change_v2: true,
                    allocate: &mut allocate,
                };
                Ok(p.schedule(
                    &details,
                    [rand::random(), rand::random(), rand::random()],
                    &mut ctx,
                )
                .await
                .operators)
            }
            BuiltinScheduler::Hot { scheduler, config } => {
                hot_schedule(
                    scheduler,
                    config,
                    self.cluster,
                    self.config,
                    self.rules,
                    self.statistics,
                    self.hot,
                    &details,
                    &mut allocate,
                )
                .await
            }
            _ => unreachable!("all builtin policies have typed adapters"),
        }
    }
}

fn hot_details(
    cluster: &BasicCluster,
    stats: &StatisticsSnapshot,
    pending: &mut HotLoadProjection,
    config: Option<&HotRegionParams>,
) -> [Vec<astersql_pd_statistics::StoreLoadDetail>; 4] {
    use astersql_pd_core_constant::ResourceKind;
    use astersql_pd_statistics::{summary_store_infos, summary_stores_load};
    let stores = cluster
        .stores
        .get_stores()
        .iter()
        .map(|s| s.as_ref().clone())
        .collect::<Vec<_>>();
    let mut infos = summary_store_infos(&stores);
    pending.summary_pending_influence(&mut infos, config, &WallClock);
    std::array::from_fn(|i| {
        let resource = HotResource::ALL[i];
        let peers = if resource.rw() == astersql_pd_statistics_utils::RWType::READ {
            &stats.read
        } else {
            &stats.write
        };
        let peers = peers.iter().map(|(id, p)| (*id, p.clone())).collect();
        summary_stores_load(
            &infos,
            &stats.stores,
            Some(&mut pending.history),
            &peers,
            true,
            resource.rw(),
            if resource.operation() == HotSourceOperation::TransferLeader {
                ResourceKind::LEADER
            } else {
                ResourceKind::REGION
            },
            std::time::Duration::from_nanos(now().1 as u64),
        )
        .into_values()
        .collect()
    })
}
struct WallClock;
impl astersql_pd_schedule_operator::Clock for WallClock {
    fn now(&self) -> std::time::Instant {
        std::time::Instant::now()
    }
}
#[allow(clippy::too_many_arguments)]
async fn hot_schedule(
    scheduler: &mut HotScheduler,
    params: &HotRegionParams,
    cluster: &BasicCluster,
    config: &ProviderSnapshot<()>,
    rules: &mut RuleManager,
    stats: &StatisticsSnapshot,
    pending: &mut HotLoadProjection,
    details: &[Vec<astersql_pd_statistics::StoreLoadDetail>; 4],
    allocate: &mut dyn FnMut() -> Result<u64, BuilderError>,
) -> Result<Vec<Arc<Operator>>, String> {
    use astersql_pd_schedule_filter::PlacementGuardContext;
    use astersql_pd_statistics_utils::RWType;
    let params = params.effective(true, true);
    struct Offer {
        resource: usize,
        source: usize,
        destination: usize,
        peer: usize,
        region: Arc<RegionInfo>,
        scope: Option<Arc<HotPlacementScope>>,
        top: [Option<f64>; 2],
    }
    let mut offers = Vec::new();
    let stores = cluster.stores.get_stores();
    let pending_ids = pending
        .region_pendings
        .keys()
        .copied()
        .collect::<HashSet<_>>();
    let mut rank_inputs = std::array::from_fn(|_| HotRankInputs::default());
    for resource in HotResource::ALL {
        let i = resource as usize;
        let names = match resource {
            HotResource::WritePeer => &params.write_peer_priorities,
            HotResource::WriteLeader => &params.write_leader_priorities,
            _ => &params.read_priorities,
        };
        let priorities = std::array::from_fn(|j| match names[j].as_str() {
            "byte" => 0,
            "key" => 1,
            "query" => 2,
            "cpu" => 3,
            _ => unreachable!(),
        });
        let source = HotSourceSelector::new(
            params.clone(),
            priorities,
            resource.rw(),
            resource.operation(),
        )
        .map_err(|e| e.to_string())?;
        let destination = HotDestinationSelector::new(
            params.clone(),
            priorities,
            resource.rw(),
            resource.operation(),
        )
        .map_err(|e| e.to_string())?;
        let mut placement =
            HotPlacementLoads::new(details[i].clone()).map_err(|e| e.to_string())?;
        let restricted = [
            rules.may_restrict_store_load(true),
            rules.may_restrict_store_load(false),
        ];
        let placement_v2 = config.is_placement_rules_enabled();
        for (s, decision) in source.filter_src_stores(&details[i], placement_v2, restricted) {
            if !decision.admitted() {
                continue;
            }
            let selected = source.filter_hot_peers(
                &s.hot_peers,
                &pending_ids,
                now().0,
                config.get_hot_region_cache_hits_threshold(),
                10,
            );
            let top = std::array::from_fn(|j| {
                selected.nth[j].map(|p| p.get_load(priorities[j] as isize))
            });
            for peer in selected.peers {
                let Some(region) = cluster.regions.get_region(peer.region_id) else {
                    continue;
                };
                let replication = RegionReplicationContext {
                    conf: config,
                    stores: &stores,
                    manager: RefCell::new(&mut *rules),
                    now: now().1 as i128,
                };
                let Ok((_, fit)) = source.get_region(
                    Some(&region),
                    s.store_summary_info.store_info.id(),
                    &replication,
                ) else {
                    continue;
                };
                drop(replication);
                let scope =
                    placement.prepare_for_region(&region, fit.as_deref(), s, resource.operation());
                if !source
                    .source_store_decision(s, scope.as_ref().map(|s| &s.expect))
                    .admitted()
                {
                    continue;
                }
                let mut guard = PlacementGuardContext {
                    conf: config,
                    stores: &stores,
                    manager: rules,
                    region: region.clone(),
                    now: now().1 as i128,
                };
                let dst_scope = scope.as_ref().map(|s| s.destination_scope());
                for d in destination
                    .filter_dst_stores(
                        &details[i],
                        &s.store_summary_info.store_info,
                        peer,
                        &mut guard,
                        fit.clone(),
                        dst_scope.as_ref(),
                    )
                    .await
                {
                    offers.push(Offer {
                        resource: i,
                        source: details[i].iter().position(|v| std::ptr::eq(v, s)).unwrap(),
                        destination: details[i].iter().position(|v| std::ptr::eq(v, d)).unwrap(),
                        peer: s
                            .hot_peers
                            .iter()
                            .position(|p| std::ptr::eq(p, peer))
                            .unwrap(),
                        region: region.clone(),
                        scope: scope.clone(),
                        top,
                    });
                }
            }
        }
        if !details[i].is_empty() {
            let mut max = details[i][0].load_pred.max();
            let mut min = details[i][0].load_pred.min();
            for d in &details[i] {
                let hi = d.load_pred.max();
                let lo = d.load_pred.min();
                for j in 0..4 {
                    max.loads[j] = max.loads[j].max(hi.loads[j]);
                    min.loads[j] = min.loads[j].min(lo.loads[j]);
                }
                max.hot_peer_count = max.hot_peer_count.max(hi.hot_peer_count);
                min.hot_peer_count = min.hot_peer_count.min(lo.hot_peer_count);
            }
            let ratios = [
                params.byte_rate_rank_step_ratio,
                params.key_rate_rank_step_ratio,
                params.query_rate_rank_step_ratio,
                params.cpu_rate_rank_step_ratio,
            ];
            let mut step = max.clone();
            for j in 0..4 {
                step.loads[j] = (max.loads[j] - min.loads[j]) * ratios[j];
            }
            step.hot_peer_count =
                (max.hot_peer_count - min.hot_peer_count) * params.count_rank_step_ratio;
            rank_inputs[i] = HotRankInputs {
                max_source: max,
                min_destination: min,
                step,
            };
        }
    }
    scheduler
        .refresh_rank_inputs(rank_inputs)
        .map_err(|e| e.to_string())?;
    let candidates = std::array::from_fn(|i| {
        offers
            .iter()
            .filter(|o| o.resource == i)
            .map(|o| {
                let s = &details[i][o.source];
                let d = &details[i][o.destination];
                let mut candidate = HotSolveCandidate::new(s, d, &s.hot_peers[o.peer], &o.region);
                candidate.scope = o.scope.as_deref();
                candidate.source_top = o.top;
                candidate
            })
            .collect()
    });
    let mut ctx = HotSolveContext {
        stores: &cluster.stores,
        config,
        rules: Some(rules),
        now_unix_nanos: now().1 as i128,
        supports_conf_change_v2: true,
        allocate,
    };
    let out = scheduler
        .schedule(&HotResource::ALL, rand::random(), &candidates, &mut ctx)
        .await
        .map_err(|e| e.to_string())?;
    let (admitted, _) = out.admit_pending(pending, &params, |rw, id, store| {
        let peers = if rw == RWType::READ {
            &stats.read
        } else {
            &stats.write
        };
        peers
            .get(&store)?
            .iter()
            .find(|p| p.region_id == id)
            .cloned()
    });
    Ok(admitted.map(|(_, r)| r.operators).unwrap_or_default())
}

struct BorrowedPolicy<'a, 'b> {
    inputs: &'a mut PolicyInputs<'b>,
    policy: &'a mut BuiltinScheduler,
}
impl LoopScheduler for BorrowedPolicy<'_, '_> {
    type Error = String;
    type Config = ProviderSnapshot<()>;
    fn admission_config(&self) -> &Self::Config {
        self.inputs.config
    }
    fn waiting_random(&mut self) -> f64 {
        rand::random()
    }
    fn allowed(&self) -> bool {
        allowed(self.policy, self.inputs.config, self.inputs.admission)
    }
    fn min_interval(&self) -> i64 {
        if matches!(self.policy, BuiltinScheduler::Hot { .. }) {
            HotScheduler::min_interval()
        } else {
            MIN_SCHEDULE_INTERVAL
        }
    }
    fn next_interval(&self, current: i64) -> i64 {
        if matches!(self.policy, BuiltinScheduler::Hot { .. }) {
            HotScheduler::next_interval(current)
        } else {
            interval_grow(current, MAX_SCHEDULE_INTERVAL, IntervalGrowth::Exponential)
        }
    }
    fn evict_leader(&self) -> bool {
        matches!(self.policy, BuiltinScheduler::EvictLeader(_))
    }
    async fn schedule(
        &mut self,
        _: &CacheCluster<'_>,
        _: bool,
    ) -> Result<Vec<Arc<Operator>>, String> {
        self.inputs.attempt(self.policy).await
    }
    async fn clean(&mut self, _: &BasicCluster) -> Result<(), String> {
        Ok(())
    }
}
impl PolicyInputs<'_> {
    pub(super) async fn schedule(
        &mut self,
        policy: &mut BuiltinScheduler,
        interval: &mut Duration,
        cancel: &CancellationToken,
    ) -> Result<Vec<Arc<Operator>>, String> {
        let cluster = Arc::new(self.cluster.clone());
        let labels = self.labels.clone();
        let adapter = BorrowedPolicy {
            inputs: self,
            policy,
        };
        let mut controller =
            ScheduleController::new(adapter, cluster, LoopControl::from_parent(cancel), || {
                now().1
            });
        controller.labeler = Some(labels);
        controller.set_interval(interval.as_nanos() as i64);
        let output = controller.schedule(false).await;
        *interval = Duration::from_nanos(controller.interval() as u64);
        output
    }
}
