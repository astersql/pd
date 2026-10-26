// Copyright 2026 AsterSQL.
//! Random adjacent merge over canonical cluster, replication and statistics snapshots.
use crate::{KeyRangeDecodeError, SchedulerError, encode_config, get_key_ranges};
use astersql_pd_core::KeyRange;
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::{CheckerConfigProvider, SchedulerConfigProvider};
use astersql_pd_schedule_core::BasicCluster;
use astersql_pd_schedule_filter::{
    RegionDownFilter, RegionPendingFilter, RegionReplicationContext, StoreStateFilter,
    is_region_healthy, is_region_replicated, select_one_region,
};
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_schedule_operator::{
    BuilderContext, BuilderError, OpKind, Operator, RangeOperatorFactory,
};
use astersql_pd_statistics::StatisticsSnapshot;
/// CLI defaults to all keys. JSON retains null versus empty ranges.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct RandomMergeConfig {
    #[serde(default)]
    ranges: Option<Vec<astersql_pd_keyutil::KeyRange>>,
}
impl RandomMergeConfig {
    pub fn new(ranges: Vec<KeyRange>) -> Self {
        Self {
            ranges: Some(
                ranges
                    .into_iter()
                    .map(|r| astersql_pd_keyutil::KeyRange::new(r.start_key, r.end_key))
                    .collect(),
            ),
        }
    }
    pub fn from_args(args: &[&str]) -> Result<Self, KeyRangeDecodeError> {
        Ok(Self::new(get_key_ranges(args)?))
    }
    pub fn ranges(&self) -> Vec<KeyRange> {
        self.ranges
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|r| KeyRange {
                start_key: r.start_key.clone(),
                end_key: r.end_key.clone(),
            })
            .collect()
    }
}
/// Host supplies time, random draws, statistics and existing canonical rule/label managers.
/// `choose_previous` is Go's even parity random draw; one-way mode ignores it.
pub struct RandomMergeContext<'a, 'b, C, P: CheckerConfigProvider> {
    pub cluster: &'a C,
    pub replication: &'a RegionReplicationContext<'b, P>,
    pub labels: Option<&'a RegionLabeler>,
    pub statistics: &'a StatisticsSnapshot,
    pub hot_degree: i64,
    pub random_index: &'a mut dyn FnMut(usize) -> usize,
    pub shuffle: &'a mut dyn FnMut(&mut [usize]),
    pub choose_previous: bool,
    pub supports_conf_change_v2: bool,
}
/// Produces the active/passive pair together, or no operation. The host owns admission.
pub struct RandomMergeScheduler {
    config: RandomMergeConfig,
}
impl RandomMergeScheduler {
    pub fn new(config: RandomMergeConfig) -> Self {
        Self { config }
    }
    pub fn config(&self) -> &RandomMergeConfig {
        &self.config
    }
    pub fn encode_config(&self) -> Result<Vec<u8>, SchedulerError> {
        encode_config(&self.config)
    }
    pub fn is_schedule_allowed(count: u64, limit: u64) -> bool {
        count < limit
    }
    /// Gating does not allocate. Factory errors preserve their typed cause and emit no partial pair.
    pub async fn schedule<C, P, F>(
        &self,
        ctx: &mut RandomMergeContext<'_, '_, C, P>,
        allocate: F,
    ) -> Result<Option<[Operator; 2]>, BuilderError>
    where
        C: BasicCluster,
        P: CheckerConfigProvider + SchedulerConfigProvider,
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let rep = ctx.replication;
        let filter = StoreStateFilter {
            action_scope: "random-merge-scheduler".into(),
            move_region: true,
            operator_level: PriorityLevel::LOW,
            now_unix_nanos: rep.now,
            ..Default::default()
        };
        let mut sources = vec![];
        for s in ctx.cluster.get_stores() {
            if filter.source(rep.conf, &s).await.is_ok() {
                sources.push(s);
            }
        }
        sources.sort_by_key(|s| s.id());
        if sources.is_empty() {
            return Ok(None);
        }
        let source = &sources[(ctx.random_index)(sources.len())];
        let sampled = ctx.cluster.rand_leader_regions(
            source.id(),
            &self.config.ranges(),
            &mut *ctx.random_index,
            &mut *ctx.shuffle,
        );
        let Some(region) =
            select_one_region(&sampled, None, &[&RegionPendingFilter, &RegionDownFilter])
        else {
            return Ok(None);
        };
        let (previous, mut target) = ctx.cluster.get_adjacent_regions(&region);
        if !rep.conf.is_one_way_merge_enabled()
            && ((ctx.choose_previous && previous.is_some()) || target.is_none())
        {
            target = previous;
        }
        let Some(target) = target else {
            return Ok(None);
        };
        if !is_region_healthy(&region)
            || !is_region_healthy(&target)
            || !is_region_replicated(rep, &region)
            || !is_region_replicated(rep, &target)
            || ctx.statistics.is_region_hot(&region, ctx.hot_degree)
            || ctx.statistics.is_region_hot(&target, ctx.hot_degree)
            || !astersql_pd_schedule_checker::allow_merge(
                rep.conf,
                Some(&rep.manager.borrow()),
                ctx.labels,
                rep.now,
                &region,
                &target,
            )
        {
            return Ok(None);
        }
        let rules = if rep.conf.is_placement_rules_enabled() {
            rep.manager.borrow().get_rules_for_apply_region(&region)
        } else {
            vec![]
        };
        let pair = RangeOperatorFactory::new(
            "random-merge-scheduler",
            BuilderContext {
                stores: ctx.cluster.stores(),
                config: rep.conf,
                rules: &rules,
                now_unix_nanos: rep.now,
            },
            &region,
            ctx.supports_conf_change_v2,
        )
        .create_merge(&target, OpKind::MERGE, allocate)
        .await?;
        for op in &pair {
            op.set_priority_level(PriorityLevel::LOW);
        }
        Ok(Some(pair))
    }
}
