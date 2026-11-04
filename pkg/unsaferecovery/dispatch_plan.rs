// Copyright 2026 AsterSQL.
use crate::{
    Controller, PlanBatch, PreparedPlan, RecoveryError, RecoveryIdAllocator, RecoveryIndex,
    RecoveryPlan, Stage,
};
use astersql_pd_core::StoresInfo;
use std::collections::BTreeMap;

/// Timestamped domain event; the host formats its local wall clock for HTTP.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StageOutput {
    pub unix_nanos: Option<i128>,
    pub info: String,
    pub actions: BTreeMap<String, Vec<String>>,
    pub details: Vec<String>,
}
impl Controller {
    pub fn dispatch_plan(&mut self, store: u64, now: i128) -> Option<RecoveryPlan> {
        if !self.stage.is_running() || self.expected_reports.get(&store)?.is_some() {
            return None;
        }
        if self
            .report_requests
            .get(&store)
            .is_some_and(|deadline| *deadline >= now)
        {
            return None;
        }
        let plan = self.plans.entry(store).or_default();
        plan.step = self.step;
        self.report_requests.insert(
            store,
            now.saturating_add(i128::from(self.plan_execution_timeout_nanos)),
        );
        Some(plan.clone())
    }
    pub fn take_reset_region_cache(&mut self) -> bool {
        std::mem::take(&mut self.reset_region_cache)
    }
    /// All stores must confirm the current step before this method can advance it.
    pub async fn advance_reports(
        &mut self,
        index: &RecoveryIndex,
        stores: &StoresInfo,
        ids: &mut impl RecoveryIdAllocator,
        now: i128,
    ) -> Result<Option<PreparedPlan>, RecoveryError> {
        if !self.stage.is_running() || self.num_stores_reported != self.expected_reports.len() {
            return Ok(None);
        }
        self.plans.clear();
        self.report_requests.clear();
        let mut phase = match self.stage {
            Stage::CollectReport | Stage::TombstoneTiFlashLearner => 0,
            Stage::ForceLeaderForCommitMerge => 1,
            Stage::ForceLeader => 2,
            Stage::DemoteFailedVoter => 3,
            Stage::CreateEmptyRegion => 4,
            Stage::ExitForceLeader => 5,
            _ => return Ok(None),
        };
        let mut recheck = false;
        loop {
            let (stage, batch) = match phase {
                0 => (
                    Stage::TombstoneTiFlashLearner,
                    self.generate_tombstone_tiflash_learner_plan(index, stores)?,
                ),
                1 => (
                    Stage::ForceLeaderForCommitMerge,
                    self.generate_force_leader_plan(index, stores, true)?,
                ),
                2 => (
                    Stage::ForceLeader,
                    self.generate_force_leader_plan(index, stores, false)?,
                ),
                3 => (
                    Stage::DemoteFailedVoter,
                    self.generate_demote_failed_voter_plan(index)?,
                ),
                4 => (
                    Stage::CreateEmptyRegion,
                    self.generate_create_empty_region_plan(index, stores, ids)
                        .await?,
                ),
                5 => {
                    let mut batch = PlanBatch::default();
                    for (store, report) in &self.expected_reports {
                        if report
                            .as_ref()
                            .is_some_and(|r| r.peers.iter().any(|p| p.is_force_leader))
                        {
                            batch.plans.insert(*store, RecoveryPlan::default());
                        }
                    }
                    (Stage::ExitForceLeader, batch)
                }
                _ => unreachable!(),
            };
            if batch.has_work() || (phase == 5 && !batch.plans.is_empty()) {
                self.affected
                    .affected_regions
                    .extend(&batch.affected_regions);
                self.affected
                    .affected_meta_regions
                    .extend(&batch.affected_meta_regions);
                self.affected
                    .affected_table_ids
                    .extend(&batch.affected_table_ids);
                self.affected
                    .newly_created_regions
                    .extend(&batch.newly_created_regions);
                self.plans = batch.plans.clone();
                self.change_stage(stage, now);
                return Ok(Some(PreparedPlan { stage, batch }));
            }
            if phase == 3 && !recheck {
                recheck = true;
                phase = 0;
                continue;
            }
            if phase == 5 {
                self.change_stage(
                    if self.recovery_error.is_some() {
                        Stage::Failed
                    } else {
                        Stage::Finished
                    },
                    now,
                );
                return Ok(None);
            }
            phase += 1;
        }
    }
    pub(crate) fn change_stage(&mut self, stage: Stage, now: i128) {
        if stage == Stage::Finished && self.step > self.recovery_start_step.wrapping_add(1) {
            self.reset_region_cache = true;
        }
        self.stage = stage;
        self.output.push(self.stage_output(now));
        if !stage.is_running() {
            self.plans.clear();
            self.report_requests.clear();
        }
        for report in self.expected_reports.values_mut() {
            *report = None;
        }
        self.num_stores_reported = 0;
        self.step = self.steps.next();
    }
    pub fn show(&self, now: i128) -> Vec<StageOutput> {
        if self.stage == Stage::Idle {
            return vec![StageOutput {
                info: "No on-going recovery.".into(),
                ..Default::default()
            }];
        }
        let mut output = self.output.clone();
        if self.stage.is_running() {
            output.push(self.report_status(now));
        }
        output
    }
    fn report_status(&self, now: i128) -> StageOutput {
        let mut out = StageOutput {
            unix_nanos: Some(now),
            ..Default::default()
        };
        if self.num_stores_reported == self.expected_reports.len() {
            out.info = format!(
                "Collected reports from all {} alive stores",
                self.expected_reports.len()
            );
        } else {
            out.info = format!(
                "Collecting reports from alive stores({}/{})",
                self.num_stores_reported,
                self.expected_reports.len()
            );
            for (label, kind) in [
                ("Stores that have not dispatched plan: ", 0),
                ("Stores that have reported to PD: ", 1),
                ("Stores that have not reported to PD: ", 2),
            ] {
                let ids = self.expected_reports.iter().filter_map(|(id, report)| {
                    let actual = if report.is_some() {
                        1
                    } else if self.report_requests.contains_key(id) {
                        2
                    } else {
                        0
                    };
                    (actual == kind).then_some(*id)
                });
                out.details.push(format!("{label}{}", joined(ids)));
            }
        }
        out
    }
    fn stage_output(&self, now: i128) -> StageOutput {
        let mut out = StageOutput {
            unix_nanos: Some(now),
            info: match self.stage {
                Stage::CollectReport => "Unsafe recovery enters collect report stage",
                Stage::TombstoneTiFlashLearner => {
                    "Unsafe recovery enters tombstone TiFlash learner stage"
                }
                Stage::ForceLeaderForCommitMerge => {
                    "Unsafe recovery enters force leader for commit merge stage"
                }
                Stage::ForceLeader => "Unsafe recovery enters force leader stage",
                Stage::DemoteFailedVoter => "Unsafe recovery enters demote Failed voter stage",
                Stage::CreateEmptyRegion => "Unsafe recovery enters create empty region stage",
                Stage::ExitForceLeader => "Unsafe recovery enters exit force leader stage",
                Stage::Finished => "Unsafe recovery Finished",
                Stage::Failed => "Unsafe recovery Failed",
                Stage::Idle => "",
            }
            .into(),
            ..Default::default()
        };
        if self.stage == Stage::CollectReport {
            out.details.push(if self.auto_detect {
                "auto detect mode with no specified Failed stores".into()
            } else {
                format!("Failed stores {}", joined(self.failed_stores.iter()))
            });
            if self.disable_paranoid_check {
                out.details.push("paranoid check disabled".into());
            }
            if self.plan_execution_timeout_nanos != 60_000_000_000 {
                out.details.push(format!(
                    "plan execution timeout {}",
                    astersql_pd_typeutil::Duration::from_nanos(self.plan_execution_timeout_nanos)
                ));
            }
        }
        if self.stage == Stage::ExitForceLeader
            && let Some(error) = &self.recovery_error
        {
            out.details.push(format!("triggered by error: {error}"));
        }
        if matches!(self.stage, Stage::Finished | Stage::Failed) {
            if self.stage == Stage::Failed {
                out.info = format!(
                    "Unsafe recovery Failed: {}",
                    self.recovery_error
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_default()
                );
            }
            if !self.affected.affected_meta_regions.is_empty() {
                out.details.push(format!(
                    "affected meta regions: {}",
                    joined(self.affected.affected_meta_regions.iter())
                ));
            }
            if !self.affected.affected_table_ids.is_empty() {
                out.details.push(format!(
                    "affected table ids: {}",
                    joined(self.affected.affected_table_ids.iter())
                ));
            }
            out.details
                .push(if self.affected.newly_created_regions.is_empty() {
                    "no newly created empty regions".into()
                } else {
                    format!(
                        "newly created empty regions: {}",
                        joined(self.affected.newly_created_regions.iter())
                    )
                });
            if self.stage == Stage::Failed
                && self.num_stores_reported != self.expected_reports.len()
            {
                out.details.extend(self.report_status(now).details);
            }
        }
        for (store, plan) in &self.plans {
            let mut actions = vec![];
            match self.stage {
                Stage::ForceLeader | Stage::ForceLeaderForCommitMerge => {
                    if let Some(force) = &plan.force_leader {
                        actions.push(format!(
                            "force leader on regions: {}",
                            joined(force.enter_force_leaders.iter())
                        ));
                    }
                }
                Stage::TombstoneTiFlashLearner | Stage::DemoteFailedVoter => {
                    if self.stage == Stage::DemoteFailedVoter {
                        for demote in &plan.demotes {
                            let peers = demote
                                .failed_voters
                                .iter()
                                .map(|p| format!("{{ {}}}", astersql_pd_core::peer_text(p)))
                                .collect::<Vec<_>>()
                                .join(", ");
                            actions
                                .push(format!("region {} demotes peers {peers}", demote.region_id));
                        }
                    }
                    actions.extend(
                        plan.tombstones
                            .iter()
                            .map(|id| format!("tombstone the peer of region {id}")),
                    );
                }
                Stage::CreateEmptyRegion => {
                    for region in &plan.creates {
                        actions.push(format!(
                            "create region {}: {}",
                            region.id,
                            astersql_pd_core::region_to_hex_meta(Some(region))
                                .to_string()
                                .replace('<', "{ ")
                                .replace('>', "}")
                        ));
                    }
                }
                _ => {}
            }
            if !actions.is_empty() {
                out.actions.insert(format!("store {store}"), actions);
            }
        }
        out
    }
}
fn joined<T: std::fmt::Display>(ids: impl IntoIterator<Item = T>) -> String {
    ids.into_iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}
