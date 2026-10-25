// Copyright 2026 AsterSQL.
//! Sequential heartbeat acknowledgement over immutable, canonical domain steps.
use crate::*;
use astersql_pd_core::{RegionEpoch, RegionInfo};
use astersql_pd_core_constant::PriorityLevel;
use std::{
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// The built-in steps supported by this operator; payloads retain their domain APIs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpStep {
    TransferLeader(TransferLeader),
    AddPeer(AddPeer),
    AddLearner(AddLearner),
    PromoteLearner(PromoteLearner),
    RemovePeer(RemovePeer),
    BecomeWitness(BecomeWitness),
    BecomeNonWitness(BecomeNonWitness),
    BatchSwitchWitness(BatchSwitchWitness),
    ChangePeerV2Enter(ChangePeerV2Enter),
    ChangePeerV2Leave(ChangePeerV2Leave),
    MergeRegion(MergeRegion),
    SplitRegion(SplitRegion),
}
impl OpStep {
    pub fn is_finish(&self, region: &RegionInfo) -> bool {
        match self {
            Self::TransferLeader(s) => s.is_finish(region),
            Self::AddPeer(s) => s.is_finish(region),
            Self::AddLearner(s) => s.is_finish(region),
            Self::PromoteLearner(s) => s.is_finish(region),
            Self::RemovePeer(s) => s.is_finish(region),
            Self::BecomeWitness(s) => s.is_finish(region),
            Self::BecomeNonWitness(s) => s.is_finish(region),
            Self::BatchSwitchWitness(s) => s.is_finish(region),
            Self::ChangePeerV2Enter(s) => s.is_finish(region),
            Self::ChangePeerV2Leave(s) => s.is_finish(region),
            Self::MergeRegion(s) => s.is_finish(region),
            Self::SplitRegion(s) => s.is_finish(region),
        }
    }
    pub fn conf_ver_changed(&self, region: &RegionInfo) -> u64 {
        match self {
            Self::TransferLeader(s) => s.conf_ver_changed(region),
            Self::AddPeer(s) => s.conf_ver_changed(region),
            Self::AddLearner(s) => s.conf_ver_changed(region),
            Self::PromoteLearner(s) => s.conf_ver_changed(region),
            Self::RemovePeer(s) => s.conf_ver_changed(region),
            Self::BecomeWitness(s) => s.conf_ver_changed(region),
            Self::BecomeNonWitness(s) => s.conf_ver_changed(region),
            Self::BatchSwitchWitness(s) => s.conf_ver_changed(region),
            Self::ChangePeerV2Enter(s) => s.conf_ver_changed(region),
            Self::ChangePeerV2Leave(s) => s.conf_ver_changed(region),
            Self::MergeRegion(s) => s.conf_ver_changed(region),
            Self::SplitRegion(s) => s.conf_ver_changed(region),
        }
    }
    pub fn influence(&self, influence: &OpInfluence, region: &RegionInfo) -> () {
        match self {
            Self::TransferLeader(s) => s.influence(influence, region),
            Self::AddPeer(s) => s.influence(influence, region),
            Self::AddLearner(s) => s.influence(influence, region),
            Self::PromoteLearner(s) => s.influence(influence, region),
            Self::RemovePeer(s) => s.influence(influence, region),
            Self::BecomeWitness(s) => s.influence(influence, region),
            Self::BecomeNonWitness(s) => s.influence(influence, region),
            Self::BatchSwitchWitness(s) => s.influence(influence, region),
            Self::ChangePeerV2Enter(s) => s.influence(influence, region),
            Self::ChangePeerV2Leave(s) => s.influence(influence, region),
            Self::MergeRegion(s) => s.influence(influence, region),
            Self::SplitRegion(s) => s.influence(influence, region),
        }
    }
    pub fn timeout(&self, size: i64) -> Duration {
        match self {
            Self::TransferLeader(s) => s.timeout(size),
            Self::AddPeer(s) => s.timeout(size),
            Self::AddLearner(s) => s.timeout(size),
            Self::PromoteLearner(s) => s.timeout(size),
            Self::RemovePeer(s) => s.timeout(size),
            Self::BecomeWitness(s) => s.timeout(size),
            Self::BecomeNonWitness(s) => s.timeout(size),
            Self::BatchSwitchWitness(s) => s.timeout(size),
            Self::ChangePeerV2Enter(s) => s.timeout(size),
            Self::ChangePeerV2Leave(s) => s.timeout(size),
            Self::MergeRegion(s) => s.timeout(size),
            Self::SplitRegion(s) => s.timeout(size),
        }
    }
}
impl From<TransferLeader> for OpStep {
    fn from(step: TransferLeader) -> Self {
        Self::TransferLeader(step)
    }
}
impl From<AddPeer> for OpStep {
    fn from(step: AddPeer) -> Self {
        Self::AddPeer(step)
    }
}
impl From<AddLearner> for OpStep {
    fn from(step: AddLearner) -> Self {
        Self::AddLearner(step)
    }
}
impl From<PromoteLearner> for OpStep {
    fn from(step: PromoteLearner) -> Self {
        Self::PromoteLearner(step)
    }
}
impl From<RemovePeer> for OpStep {
    fn from(step: RemovePeer) -> Self {
        Self::RemovePeer(step)
    }
}
impl From<BecomeWitness> for OpStep {
    fn from(step: BecomeWitness) -> Self {
        Self::BecomeWitness(step)
    }
}
impl From<BecomeNonWitness> for OpStep {
    fn from(step: BecomeNonWitness) -> Self {
        Self::BecomeNonWitness(step)
    }
}
impl From<BatchSwitchWitness> for OpStep {
    fn from(step: BatchSwitchWitness) -> Self {
        Self::BatchSwitchWitness(step)
    }
}
impl From<ChangePeerV2Enter> for OpStep {
    fn from(step: ChangePeerV2Enter) -> Self {
        Self::ChangePeerV2Enter(step)
    }
}
impl From<ChangePeerV2Leave> for OpStep {
    fn from(step: ChangePeerV2Leave) -> Self {
        Self::ChangePeerV2Leave(step)
    }
}
impl From<MergeRegion> for OpStep {
    fn from(step: MergeRegion) -> Self {
        Self::MergeRegion(step)
    }
}
impl From<SplitRegion> for OpStep {
    fn from(step: SplitRegion) -> Self {
        Self::SplitRegion(step)
    }
}

struct Execution {
    current: usize,
    finished: Vec<Option<Instant>>,
    influence: Option<OpInfluence>,
}
/// Heartbeat-driven operation. The execution mutex serializes progress and its cache.
/// No runtime, transport, global clock or storage backend is installed by this library.
pub struct Operator {
    desc: String,
    brief: String,
    region_id: u64,
    region_epoch: Option<RegionEpoch>,
    kind: OpKind,
    approximate_size: i64,
    steps: Vec<OpStep>,
    execution: Mutex<Execution>,
    status: OpStatusTracker,
    clock: Option<Arc<dyn Clock>>,
    timeout_nanos: i64,
    priority: RwLock<PriorityLevel>,
    health_check: AtomicBool,
}
impl Operator {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        desc: impl Into<String>,
        brief: impl Into<String>,
        region_id: u64,
        region_epoch: Option<RegionEpoch>,
        kind: OpKind,
        approximate_size: i64,
        steps: Vec<OpStep>,
    ) -> Self {
        Self::build(
            desc.into(),
            brief.into(),
            region_id,
            region_epoch,
            kind,
            approximate_size,
            steps,
            None,
        )
    }
    /// Accept a host clock for deterministic expiration and completion timestamps.
    #[allow(clippy::too_many_arguments)]
    pub fn with_clock(
        desc: impl Into<String>,
        brief: impl Into<String>,
        region_id: u64,
        region_epoch: Option<RegionEpoch>,
        kind: OpKind,
        approximate_size: i64,
        steps: Vec<OpStep>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self::build(
            desc.into(),
            brief.into(),
            region_id,
            region_epoch,
            kind,
            approximate_size,
            steps,
            Some(clock),
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn build(
        desc: String,
        brief: String,
        region_id: u64,
        region_epoch: Option<RegionEpoch>,
        kind: OpKind,
        approximate_size: i64,
        steps: Vec<OpStep>,
        clock: Option<Arc<dyn Clock>>,
    ) -> Self {
        // Source sums signed duration seconds in floating point, truncates, then
        // multiplies by one second with Go's signed integer wrapping.
        let seconds: f64 = steps
            .iter()
            .map(|s| (s.timeout(approximate_size).as_nanos() as u64 as i64) as f64 / 1e9)
            .sum();
        let timeout_nanos = (seconds as i64).wrapping_mul(1_000_000_000);
        let status = clock.as_ref().map_or_else(OpStatusTracker::new, |c| {
            OpStatusTracker::with_clock(c.clone())
        });
        Self {
            desc,
            brief,
            region_id,
            region_epoch,
            kind,
            approximate_size,
            execution: Mutex::new(Execution {
                current: 0,
                finished: vec![None; steps.len()],
                influence: None,
            }),
            steps,
            status,
            clock,
            timeout_nanos,
            priority: RwLock::new(if kind.0 & OpKind::ADMIN.0 != 0 {
                PriorityLevel::URGENT
            } else {
                PriorityLevel::MEDIUM
            }),
            health_check: AtomicBool::new(false),
        }
    }
    pub fn desc(&self) -> &str {
        &self.desc
    }
    pub fn brief(&self) -> &str {
        &self.brief
    }
    pub fn region_id(&self) -> u64 {
        self.region_id
    }
    pub fn region_epoch(&self) -> Option<&RegionEpoch> {
        self.region_epoch.as_ref()
    }
    pub fn kind(&self) -> OpKind {
        self.kind
    }
    pub fn approximate_size(&self) -> i64 {
        self.approximate_size
    }
    pub fn timeout_nanos(&self) -> i64 {
        self.timeout_nanos
    }
    pub fn len(&self) -> usize {
        self.steps.len()
    }
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
    pub fn step(&self, index: usize) -> Option<&OpStep> {
        self.steps.get(index)
    }
    fn execution(&self) -> std::sync::MutexGuard<'_, Execution> {
        self.execution
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
    pub fn current_step(&self) -> usize {
        self.execution().current
    }
    pub fn step_finish_time(&self, index: usize) -> Option<Instant> {
        self.execution().finished.get(index).copied().flatten()
    }
    pub fn status(&self) -> OpStatus {
        self.status.status()
    }
    pub fn reach_time_of(&self, status: OpStatus) -> Option<Instant> {
        self.status.reach_time_of(status)
    }
    pub fn is_end(&self) -> bool {
        self.status.is_end()
    }
    pub fn start(&self) -> bool {
        self.status.to(STARTED)
    }
    pub fn has_started(&self) -> bool {
        self.reach_time_of(STARTED).is_some()
    }
    pub fn cancel(&self) -> bool {
        let _guard = self.execution();
        self.status.to(CANCELED)
    }
    pub fn replace(&self) -> bool {
        let _guard = self.execution();
        self.status.to(REPLACED)
    }
    fn success_at(&self, current: usize) -> bool {
        current >= self.steps.len() && (self.status.to(SUCCESS) || self.status() == SUCCESS)
    }
    pub fn check_success(&self) -> bool {
        self.success_at(self.execution().current)
    }
    fn timeout_at(&self, current: usize) -> bool {
        !self.success_at(current)
            && self
                .status
                .check_timeout(Duration::from_nanos(self.timeout_nanos.max(0) as u64))
    }
    pub fn check_timeout(&self) -> bool {
        self.timeout_at(self.execution().current)
    }
    pub fn check_expired(&self) -> bool {
        self.status.check_expired(Duration::from_secs(3))
    }
    /// Return the first unsatisfied step. Completion wins over a simultaneous timeout.
    /// Like Go, the heartbeat that detects timeout still returns its pending step.
    /// CREATED operators may advance, but can succeed only after Start.
    pub fn check(&self, region: &RegionInfo) -> Option<&OpStep> {
        let mut state = self.execution();
        if self.is_end() {
            return None;
        }
        while state.current < self.steps.len() {
            let i = state.current;
            if !self.steps[i].is_finish(region) {
                break;
            }
            state.finished[i] = Some(self.clock.as_ref().map_or_else(Instant::now, |c| c.now()));
            state.current += 1;
        }
        let next = self.steps.get(state.current);
        self.timeout_at(state.current);
        next
    }
    /// Include the current step because it may already have taken effect in this heartbeat.
    pub fn conf_ver_changed(&self, region: &RegionInfo) -> u64 {
        let end = self
            .execution()
            .current
            .saturating_add(1)
            .min(self.steps.len());
        self.steps[..end]
            .iter()
            .fold(0u64, |n, s| n.wrapping_add(s.conf_ver_changed(region)))
    }
    pub fn priority_level(&self) -> PriorityLevel {
        *self
            .priority
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
    pub fn set_priority_level(&self, level: PriorityLevel) {
        *self
            .priority
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = level;
    }
    pub fn need_store_health_check(&self) -> bool {
        self.health_check.load(Ordering::Relaxed)
    }
    pub fn set_store_health_check(&self, need: bool) {
        self.health_check.store(need, Ordering::Relaxed);
    }
    pub fn unfinished_influence(&self, influence: &OpInfluence, region: &RegionInfo) {
        let current = self.execution().current;
        for step in &self.steps[current..] {
            if !step.is_finish(region) {
                step.influence(influence, region);
            }
        }
    }
    /// Cache the first complete influence snapshot; None only reuses an existing cache.
    pub fn total_influence(&self, influence: &OpInfluence, region: Option<&RegionInfo>) {
        let mut state = self.execution();
        if state.influence.is_none() {
            let Some(region) = region else {
                return;
            };
            let total = OpInfluence::new();
            for step in &self.steps {
                step.influence(&total, region);
            }
            state.influence = Some(total);
        }
        influence.add(state.influence.as_ref().expect("initialized above"));
    }
}
