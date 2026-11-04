// Copyright 2026 AsterSQL.
use astersql_pd_core::StoreInfo;
use astersql_pd_errs::catalog::{
    ERR_STORE_NOT_FOUND, ERR_UNSAFE_RECOVERY_INVALID_INPUT, ERR_UNSAFE_RECOVERY_IS_RUNNING,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stage {
    #[default]
    Idle,
    CollectReport,
    TombstoneTiFlashLearner,
    ForceLeaderForCommitMerge,
    ForceLeader,
    DemoteFailedVoter,
    CreateEmptyRegion,
    ExitForceLeader,
    Finished,
    Failed,
}
impl Stage {
    pub fn is_running(self) -> bool {
        !matches!(self, Self::Idle | Self::Finished | Self::Failed)
    }
}
#[derive(Clone, Debug, Default)]
pub struct RemoveFailedStoresOptions {
    pub plan_execution_timeout_nanos: i64,
    pub disable_paranoid_check: bool,
}
#[derive(Clone, Debug, Default)]
pub struct StartRequest {
    pub failed_stores: BTreeSet<u64>,
    pub timeout_seconds: u64,
    pub auto_detect: bool,
    pub options: RemoveFailedStoresOptions,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecoveryError {
    Running,
    Invalid(String),
    StoreNotFound(u64),
    Backend(String),
}
impl std::fmt::Display for RecoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Running => std::fmt::Display::fmt(
                &ERR_UNSAFE_RECOVERY_IS_RUNNING.instantiate([] as [String; 0]),
                f,
            ),
            Self::Invalid(s) => {
                std::fmt::Display::fmt(&ERR_UNSAFE_RECOVERY_INVALID_INPUT.instantiate([s]), f)
            }
            Self::StoreNotFound(id) => {
                std::fmt::Display::fmt(&ERR_STORE_NOT_FOUND.instantiate([id.to_string()]), f)
            }
            Self::Backend(s) => f.write_str(s),
        }
    }
}
impl std::error::Error for RecoveryError {}
#[async_trait::async_trait]
pub trait RecoveryCluster: Send {
    fn stores(&self) -> Vec<Arc<StoreInfo>>;
    async fn bury_store(&mut self, id: u64) -> Result<(), RecoveryError>;
    fn set_recovery_halt(&mut self, halt: bool);
}
/// Shared by host terms, so reassigning a controller cannot reuse a recovery step.
pub struct RecoverySteps(AtomicU64);
impl RecoverySteps {
    pub fn new(seed: u64) -> Self {
        Self(AtomicU64::new(seed))
    }
    pub(crate) fn next(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst).wrapping_add(1)
    }
}
pub struct Controller {
    pub(crate) plans: BTreeMap<u64, crate::RecoveryPlan>,
    pub(crate) affected: crate::PlanBatch,
    pub(crate) output: Vec<crate::StageOutput>,
    pub(crate) reset_region_cache: bool,
    pub(crate) steps: Arc<RecoverySteps>,
    pub(crate) recovery_error: Option<RecoveryError>,
    pub(crate) stage: Stage,
    pub(crate) step: u64,
    pub(crate) recovery_start_step: u64,
    pub(crate) failed_stores: BTreeSet<u64>,
    pub(crate) num_stores_reported: usize,
    pub(crate) report_requests: BTreeMap<u64, i128>,
    pub(crate) expected_reports: BTreeMap<u64, Option<crate::StoreReport>>,
    pub(crate) timeout_unix_nanos: i128,
    pub(crate) auto_detect: bool,
    pub(crate) plan_execution_timeout_nanos: i64,
    pub(crate) disable_paranoid_check: bool,
}
impl Controller {
    pub fn stage(&self) -> Stage {
        self.stage
    }
    pub fn step(&self) -> u64 {
        self.step
    }
    pub fn expected_report_stores(&self) -> impl Iterator<Item = u64> + '_ {
        self.expected_reports.keys().copied()
    }

    pub fn new(steps: Arc<RecoverySteps>) -> Self {
        Self {
            plans: BTreeMap::new(),
            affected: crate::PlanBatch::default(),
            output: Vec::new(),
            reset_region_cache: false,
            steps,
            recovery_error: None,
            stage: Stage::Idle,
            step: 0,
            recovery_start_step: 0,
            failed_stores: BTreeSet::new(),
            expected_reports: BTreeMap::new(),
            num_stores_reported: 0,
            report_requests: BTreeMap::new(),
            timeout_unix_nanos: 0,
            auto_detect: false,
            plan_execution_timeout_nanos: 60_000_000_000,
            disable_paranoid_check: false,
        }
    }
    pub async fn remove_failed_stores_with_options(
        &mut self,
        cluster: &mut impl RecoveryCluster,
        request: StartRequest,
        now: i128,
    ) -> Result<(), RecoveryError> {
        if self.stage.is_running() {
            return Err(RecoveryError::Running);
        }
        let stores = cluster.stores();
        if !request.auto_detect {
            if request.failed_stores.is_empty() {
                return Err(RecoveryError::Invalid("no store specified".into()));
            }
            for id in &request.failed_stores {
                let store = stores
                    .iter()
                    .find(|s| s.id() == *id)
                    .ok_or_else(|| RecoveryError::Invalid(format!("store {id} doesn't exist")))?;
                if (store.is_preparing() || store.is_serving()) && !store.is_disconnected(now) {
                    return Err(RecoveryError::Invalid(format!(
                        "store {id} is up and connected"
                    )));
                }
            }
            for id in &request.failed_stores {
                match cluster.bury_store(*id).await {
                    Ok(()) | Err(RecoveryError::StoreNotFound(_)) => {}
                    Err(e) => return Err(e),
                }
            }
        }
        self.plans.clear();
        self.affected = crate::PlanBatch::default();
        self.output.clear();
        self.reset_region_cache = false;
        self.recovery_error = None;
        self.num_stores_reported = 0;
        self.report_requests.clear();
        self.failed_stores = request.failed_stores;
        self.auto_detect = request.auto_detect;
        self.expected_reports = cluster
            .stores()
            .into_iter()
            .filter(|s| {
                !s.is_removed()
                    && !s.is_physically_destroyed()
                    && !self.failed_stores.contains(&s.id())
            })
            .map(|s| (s.id(), None))
            .collect();
        // Go time.Duration(uint64 seconds) multiplication uses signed int64 wrapping.
        self.timeout_unix_nanos =
            now + i128::from((request.timeout_seconds as i64).wrapping_mul(1_000_000_000));
        self.recovery_start_step = self.steps.next();
        // Go changeStage(CollectReport) advances once more after recording the run start.

        self.plan_execution_timeout_nanos = if request.options.plan_execution_timeout_nanos > 0 {
            request.options.plan_execution_timeout_nanos
        } else {
            60_000_000_000
        };
        self.disable_paranoid_check = request.options.disable_paranoid_check;
        self.change_stage(Stage::CollectReport, now);
        cluster.set_recovery_halt(true);
        Ok(())
    }
}
