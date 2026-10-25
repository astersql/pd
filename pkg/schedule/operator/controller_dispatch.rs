// Copyright 2026 AsterSQL.
//! Host-driven heartbeat dispatch using canonical admission, steps and stream routing.
use crate::*;
use astersql_pd_core::{BasicCluster, RegionInfo};
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_hbstream::{
    HeartbeatCommand, Operation,
    routing::{RoutingClosed, RoutingHandle},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// Only heartbeat dispatch checks stale step state and unexpected confver changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchSource {
    Heartbeat,
    Create,
    Notifier,
}

/// Preserve the step's typed rejection at the controller boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DispatchStepError {
    Leader(LeaderTransferError),
    Peer(PeerStepError),
    Remove(RemovePeerError),
    Joint(JointStepError),
}
impl std::fmt::Display for DispatchStepError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Leader(e) => e.fmt(f),
            Self::Peer(e) => e.fmt(f),
            Self::Remove(e) => e.fmt(f),
            Self::Joint(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for DispatchStepError {}

impl OpStep {
    /// Build an optional domain command, preserving each step's suppression rules.
    pub fn get_cmd(&self, r: &RegionInfo, v2: bool) -> Option<Operation> {
        match self {
            Self::TransferLeader(s) => Some(s.get_cmd(r, v2)),
            Self::AddPeer(s) => s.get_cmd(r, v2),
            Self::AddLearner(s) => s.get_cmd(r, v2),
            Self::PromoteLearner(s) => Some(s.get_cmd(r, v2)),
            Self::RemovePeer(s) => Some(s.get_cmd(r, v2)),
            Self::BecomeWitness(s) => Some(s.get_cmd(r, v2)),
            Self::BecomeNonWitness(s) => Some(s.get_cmd(r, v2)),
            Self::BatchSwitchWitness(s) => Some(s.get_cmd(r, v2)),
            Self::ChangePeerV2Enter(s) => s.get_cmd(r, v2),
            Self::ChangePeerV2Leave(s) => s.get_cmd(r, v2),
            Self::MergeRegion(s) => s.get_cmd(r, v2),
            Self::SplitRegion(s) => Some(s.get_cmd(r, v2)),
        }
    }
    /// The unhealthy permission applies only to leader transfer; Down always applies.
    pub fn check_in_progress<C: SharedConfigProvider>(
        &self,
        c: &BasicCluster,
        cfg: &C,
        r: &RegionInfo,
        health: bool,
        now: i128,
    ) -> Result<(), DispatchStepError> {
        match self {
            Self::TransferLeader(s) => s
                .check_in_progress(c, cfg, r, health, now)
                .map_err(DispatchStepError::Leader),
            Self::AddPeer(s) => s
                .check_in_progress(c, cfg, r, false, now)
                .map_err(DispatchStepError::Peer),
            Self::AddLearner(s) => s
                .check_in_progress(c, cfg, r, false, now)
                .map_err(DispatchStepError::Peer),
            Self::PromoteLearner(s) => s
                .check_in_progress(c, cfg, r, false, now)
                .map_err(DispatchStepError::Peer),
            Self::RemovePeer(s) => s
                .check_in_progress(c, cfg, r, false, now)
                .map_err(DispatchStepError::Remove),
            Self::BecomeWitness(s) => s
                .check_in_progress(c, cfg, r, false, now)
                .map_err(DispatchStepError::Peer),
            Self::BecomeNonWitness(s) => s
                .check_in_progress(c, cfg, r, false, now)
                .map_err(DispatchStepError::Peer),
            Self::BatchSwitchWitness(s) => s
                .check_in_progress(c, cfg, r, false, now)
                .map_err(DispatchStepError::Peer),
            Self::ChangePeerV2Enter(s) => s
                .check_in_progress(c, cfg, r, false, now)
                .map_err(DispatchStepError::Joint),
            Self::ChangePeerV2Leave(s) => s
                .check_in_progress(c, cfg, r, false, now)
                .map_err(DispatchStepError::Joint),
            Self::MergeRegion(s) => s
                .check_in_progress(c, cfg, r, false, now)
                .map_err(|e| match e {}),
            Self::SplitRegion(s) => s
                .check_in_progress(c, cfg, r, false, now)
                .map_err(|e| match e {}),
        }
    }
}
/// Next retry: two seconds for fast steps, five seconds for all others (including nil).
pub fn next_push_deadline(step: Option<&OpStep>, now: Instant) -> Instant {
    let fast = matches!(
        step,
        Some(
            OpStep::TransferLeader(_)
                | OpStep::PromoteLearner(_)
                | OpStep::ChangePeerV2Enter(_)
                | OpStep::ChangePeerV2Leave(_)
        )
    );
    now + Duration::from_secs(if fast { 2 } else { 5 })
}
/// Observable effects for host callbacks/history and waiting-queue promotion.
/// `enqueued` acknowledges only queue admission, never transport delivery.
#[derive(Default)]
pub struct DispatchOutcome {
    pub enqueued: bool,
    pub removed: Vec<Arc<Operator>>,
    pub completed: Option<Arc<Operator>>,
    pub record_non_witness: bool,
    pub promote_waiting: bool,
    pub stale_error: Option<DispatchStepError>,
}
/// Poll result distinguishes an empty/future queue from a discarded entry.
pub struct DispatchPoll {
    pub region: Option<Arc<RegionInfo>>,
    pub next: bool,
    pub removed: Vec<Arc<Operator>>,
}
/// Borrows shared state; the host owns clocks, runtime and callback/promotion policy.
/// Use one serial host dispatcher per admission controller. Admission/removal may
/// race with dispatch as in Go; identity-checked removal cannot delete replacements.
/// After each outcome, invoke success/TTL hooks and promote waiting when requested;
/// enqueue newly admitted operators with `schedule` and dispatch Create immediately.
/// The caller supplies the negotiated conf-change version, never a Cargo feature.
pub struct ControllerDispatch<'a, C> {
    pub admission: &'a ControllerAdmission,
    cluster: &'a BasicCluster,
    config: &'a C,
    streams: &'a RoutingHandle,
    cluster_id: u64,
    pub push_queue: ConcurrentHeapOpQueue,
    pub fast_operators: FastOperatorInfluence,
}
impl<'a, C: SharedConfigProvider> ControllerDispatch<'a, C> {
    pub fn new(
        admission: &'a ControllerAdmission,
        cluster: &'a BasicCluster,
        config: &'a C,
        streams: &'a RoutingHandle,
        cluster_id: u64,
    ) -> Self {
        Self {
            admission,
            cluster,
            config,
            streams,
            cluster_id,
            push_queue: Default::default(),
            fast_operators: Default::default(),
        }
    }
    pub fn schedule(&self, op: Arc<Operator>, now: Instant) {
        let region = self.cluster.regions.get_region(op.region_id());
        let step = region.as_deref().and_then(|r| op.check(r));
        let deadline = next_push_deadline(step, now);
        self.push_queue.push(OperatorWithTime { op, deadline });
    }
    fn remove(&self, op: &Arc<Operator>, reason: CancelReason) -> Vec<Arc<Operator>> {
        self.admission
            .remove_limited_operator(self.cluster, self.config, op, reason)
    }
    /// No leader or a suppressed step command enqueues nothing. Queue closure is
    /// typed and leaves the operation running for a later retry/rebind.
    pub async fn send_schedule_command(
        &self,
        r: &RegionInfo,
        step: &OpStep,
        v2: bool,
    ) -> Result<bool, RoutingClosed> {
        let Some(command) = step
            .get_cmd(r, v2)
            .and_then(|cmd| HeartbeatCommand::for_region(self.cluster_id, r, cmd))
        else {
            return Ok(false);
        };
        self.streams
            .send(command)
            .await
            .map_err(|_| RoutingClosed)?;
        Ok(true)
    }
    pub async fn dispatch(
        &self,
        r: &RegionInfo,
        source: DispatchSource,
        now: Instant,
        unix_nanos: i128,
        v2: bool,
    ) -> Result<DispatchOutcome, RoutingClosed> {
        let mut out = DispatchOutcome::default();
        let Some(op) = self.admission.get_operator(r.meta().id) else {
            return Ok(out);
        };
        let step = op.check(r);
        match op.status() {
            STARTED => {
                if let Some(step) = step {
                    if source == DispatchSource::Heartbeat {
                        if let Err(error) = step.check_in_progress(
                            self.cluster,
                            self.config,
                            r,
                            op.need_store_health_check(),
                            unix_nanos,
                        ) {
                            out.stale_error = Some(error);
                            out.removed = self.remove(&op, CancelReason::StaleStatus);
                            if !out.removed.is_empty() {
                                out.promote_waiting = true;
                                return Ok(out);
                            }
                        }
                        let origin = op.region_epoch().copied().unwrap_or_default();
                        let latest = r.meta().epoch.unwrap_or_default();
                        if latest.conf_ver.wrapping_sub(origin.conf_ver) > op.conf_ver_changed(r) {
                            out.removed = self.remove(&op, CancelReason::EpochNotMatch);
                            if !out.removed.is_empty() {
                                out.promote_waiting = true;
                                return Ok(out);
                            }
                        }
                    }
                    out.enqueued = self.send_schedule_command(r, step, v2).await?;
                }
            }
            SUCCESS => {
                out.record_non_witness =
                    (0..op.len()).any(|i| matches!(op.step(i), Some(OpStep::BecomeNonWitness(_))));
                out.completed = Some(op.clone());
                out.removed = self.admission.detach_operator(&op, &[]);
                ack_removed_operators(&out.removed, self.cluster, self.config);
                if op.reach_time_of(STARTED).is_some_and(|start| {
                    now.saturating_duration_since(start) < Duration::from_secs(10)
                }) {
                    self.fast_operators.put_at(op, now);
                }
            }
            TIMEOUT => {
                out.removed = self.remove(&op, CancelReason::Timeout);
            }
            _ => {
                out.removed = self.remove(&op, CancelReason::NotInRunningState);
            }
        }
        out.promote_waiting = !out.removed.is_empty();
        Ok(out)
    }
    /// Missing regions and newer merge range versions are removed before deadlines.
    /// Resolve the current operator by region ID, matching Go's replacement retry.
    pub fn poll_due(&self, now: Instant) -> DispatchPoll {
        let mut result = DispatchPoll {
            region: None,
            next: false,
            removed: vec![],
        };
        let Some(mut item) = self.push_queue.pop() else {
            return result;
        };
        result.next = true;
        let Some(op) = self.admission.get_operator(item.op.region_id()) else {
            return result;
        };
        let Some(region) = self.cluster.regions.get_region(op.region_id()) else {
            result.removed = self.remove(&op, CancelReason::RegionNotFound);
            return result;
        };
        if op.additional_info().has_related_merge_region()
            && region.meta().epoch.unwrap_or_default().version
                > op.region_epoch().copied().unwrap_or_default().version
        {
            result.removed = self.remove(&op, CancelReason::EpochNotMatch);
            return result;
        }
        let step = op.check(&region);
        if step.is_none() {
            result.region = Some(region);
            return result;
        }
        if now < item.deadline {
            self.push_queue.push(item);
            result.next = false;
            return result;
        }
        item.deadline = next_push_deadline(step, now);
        self.push_queue.push(item);
        result.region = Some(region);
        result
    }
    /// Drain due entries, leaving future deadlines queued. The host consumes the
    /// returned effects to record terminal operations and promote waiting batches.
    pub async fn push_due(
        &self,
        now: Instant,
        unix_nanos: i128,
        v2: bool,
    ) -> Result<Vec<DispatchOutcome>, RoutingClosed> {
        let mut outcomes = Vec::new();
        loop {
            let poll = self.poll_due(now);
            if !poll.next {
                break;
            }
            if !poll.removed.is_empty() {
                outcomes.push(DispatchOutcome {
                    removed: poll.removed,
                    ..Default::default()
                });
            }
            if let Some(region) = poll.region {
                outcomes.push(
                    self.dispatch(&region, DispatchSource::Notifier, now, unix_nanos, v2)
                        .await?,
                );
            }
        }
        Ok(outcomes)
    }
}
