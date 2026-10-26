// Copyright 2026 AsterSQL.
//! Conservative projected balance scores using canonical stores and operator influence.
use astersql_pd_core_constant::{ResourceKind, ScheduleKind, SchedulePolicy};
use astersql_pd_schedule_config::{ScheduleConfig, SchedulerConfigProvider};
use astersql_pd_schedule_core::SchedulerCluster;
use astersql_pd_schedule_operator::OpInfluence;
use astersql_pd_schedule_plan::BalanceSchedulerPlan;

/// A candidate cannot be scored until both stores have been selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BalanceError {
    #[error("balance source is absent")]
    MissingSource,
    #[error("balance target is absent")]
    MissingTarget,
}

/// Scores after the conservative tolerance and pending influence have been applied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BalanceScores {
    pub source_score: f64,
    pub target_score: f64,
}
impl BalanceScores {
    /// Equality and unordered (NaN) scores never admit a move.
    pub fn should_balance(self) -> bool {
        self.source_score > self.target_score
    }
}

/// A borrowed scheduling generation. Candidate changes do not cache stale scores.
pub struct BalanceSolver<'a> {
    kind: ScheduleKind,
    config: &'a ScheduleConfig,
    average_region_size: i64,
    tolerant_ratio: f64,
}
impl<'a> BalanceSolver<'a> {
    /// Explicit input boundary also supports range-cluster tolerance overrides.
    /// `max_region_count` is the maximum *observed* per-store region count.
    pub fn new(
        kind: ScheduleKind,
        config: &'a ScheduleConfig,
        average_region_size: i64,
        max_region_count: usize,
    ) -> Self {
        let ratio = config.tolerant_size_ratio;
        let tolerant_ratio = if ratio != 0.0 {
            ratio
        } else if kind.resource == ResourceKind::LEADER && kind.policy == SchedulePolicy::BY_COUNT {
            5.0
        } else {
            (max_region_count as f64 * 0.005).max(1.0)
        };
        Self {
            kind,
            config,
            average_region_size,
            tolerant_ratio,
        }
    }
    /// Read canonical region indexes rather than store heartbeat count estimates.
    pub fn from_cluster<C: SchedulerCluster>(kind: ScheduleKind, cluster: &'a C) -> Self {
        let max_count = cluster
            .get_stores()
            .iter()
            .map(|s| cluster.store_region_count(s.id()))
            .max()
            .unwrap_or(0);
        Self::new(
            kind,
            cluster.get_scheduler_config().get_schedule_config(),
            cluster.average_region_size(),
            max_count,
        )
    }
    /// Score a range projection using its separate tolerance override.
    /// Host config and root store counts remain inherited; average size is local.
    pub fn from_range<C: astersql_pd_schedule_core::BasicCluster + ?Sized>(
        kind: ScheduleKind,
        config: &'a ScheduleConfig,
        range: &crate::RangeCluster<'_, C>,
    ) -> Self {
        use astersql_pd_schedule_core::BasicCluster;
        let max_count = range
            .get_stores()
            .iter()
            .map(|s| range.store_region_count(s.id()))
            .max()
            .unwrap_or(0);
        let mut solver = Self::new(kind, config, range.average_region_size(), max_count);
        let ratio = range.get_tolerant_size_ratio(config.tolerant_size_ratio);
        if ratio != 0.0 {
            solver.tolerant_ratio = ratio;
        }
        solver
    }
    pub fn tolerant_ratio(&self) -> f64 {
        self.tolerant_ratio
    }
    pub fn tolerant_resource(&self) -> i64 {
        if (self.kind.resource == ResourceKind::LEADER
            || self.kind.resource == ResourceKind::WITNESS)
            && self.kind.policy == SchedulePolicy::BY_COUNT
        {
            self.tolerant_ratio as i64
        } else {
            (self.average_region_size as f64 * self.tolerant_ratio) as i64
        }
    }
    /// The candidate replaces the margin when larger; it is never added twice.
    pub fn region_score_delta(&self, plan: &BalanceSchedulerPlan) -> i64 {
        plan.region.as_ref().map_or(self.tolerant_resource(), |r| {
            self.tolerant_resource().max(r.stats().approximate_size)
        })
    }
    /// No cluster mutation, command creation or I/O occurs during scoring.
    pub fn evaluate(
        &self,
        plan: &BalanceSchedulerPlan,
        influence: &OpInfluence,
    ) -> Result<BalanceScores, BalanceError> {
        let source = plan.source.as_ref().ok_or(BalanceError::MissingSource)?;
        let target = plan.target.as_ref().ok_or(BalanceError::MissingTarget)?;
        let mut source_influence = influence
            .get_store_influence(source.id())
            .resource_property(self.kind);
        let mut target_influence = influence
            .get_store_influence(target.id())
            .resource_property(self.kind);
        if source_influence > 0 {
            source_influence = source_influence.wrapping_neg();
        }
        if target_influence < 0 {
            target_influence = target_influence.wrapping_neg();
        }
        let (source_delta, target_delta) = if self.kind.resource == ResourceKind::REGION {
            (
                source_influence
                    .wrapping_mul(5)
                    .wrapping_sub(self.region_score_delta(plan)),
                target_influence
                    .wrapping_mul(5)
                    .wrapping_add(self.region_score_delta(plan)),
            )
        } else {
            (
                source_influence.wrapping_sub(self.tolerant_resource()),
                target_influence.wrapping_add(self.tolerant_resource()),
            )
        };
        let score = |store: &astersql_pd_core::StoreInfo, delta| match self.kind.resource {
            ResourceKind::LEADER => store.leader_score(self.kind.policy, delta),
            ResourceKind::REGION => store.region_score(
                &self.config.region_score_formula_version,
                self.config.high_space_ratio,
                self.config.low_space_ratio,
                delta,
            ),
            ResourceKind::WITNESS => store.witness_score(delta),
            _ => 0.0,
        };
        Ok(BalanceScores {
            source_score: score(source, source_delta),
            target_score: score(target, target_delta),
        })
    }
}
