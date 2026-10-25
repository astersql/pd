// Copyright 2026 AsterSQL.
//! Pause and role recovery over canonical region and operator snapshots.
use astersql_pd_core::{RegionInfo, is_in_joint_state};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_operator::{
    BuilderContext, BuilderError, Operator, PeerOperatorFactory, RangeOperatorFactory,
};
use std::{
    sync::atomic::{AtomicI64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

fn unix_seconds() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64) - i64::from(e.duration().subsec_nanos() != 0),
    }
}

/// An independently paused checker. Updates replace the previous deadline;
/// zero and negative TTLs resume immediately. All deadlines use Unix seconds.
#[derive(Default, Debug)]
pub struct PauseController {
    delay_until: AtomicI64,
}
impl PauseController {
    /// Observe the current wall clock without owning a timer or runtime.
    pub fn is_paused(&self) -> bool {
        self.is_paused_at(unix_seconds())
    }
    /// Explicit clock boundary for host snapshots and deterministic tests.
    pub fn is_paused_at(&self, now_unix_seconds: i64) -> bool {
        now_unix_seconds < self.delay_until.load(Ordering::SeqCst)
    }
    /// Replace the deadline relative to the current wall clock.
    pub fn pause_or_resume(&self, ttl_seconds: i64) {
        self.pause_or_resume_at(ttl_seconds, unix_seconds());
    }
    /// Match Go's int64 arithmetic, including overflow, without panicking.
    pub fn pause_or_resume_at(&self, ttl_seconds: i64, now_unix_seconds: i64) {
        self.delay_until
            .store(now_unix_seconds.wrapping_add(ttl_seconds), Ordering::SeqCst);
    }
}

/// The same optional operation as Go Check, retaining factory failures as
/// typed diagnostics instead of discarding them in a library-global logger.
/// Earlier learner failures can coexist with a later successful promotion.
#[derive(Default)]
pub struct RoleCheckOutcome {
    pub operator: Option<Operator>,
    pub errors: Vec<BuilderError>,
}

/// Promote the first eligible learner in the region's classified peer-ID order.
#[derive(Default, Debug)]
pub struct LearnerChecker {
    pub pause: PauseController,
}
impl LearnerChecker {
    /// Use the host's store/config/rule snapshot and version capability. Existing
    /// peer promotions require no ID allocation and never mutate the region.
    pub async fn check<V>(
        &self,
        context: BuilderContext<'_, V>,
        region: &RegionInfo,
        supports_conf_change_v2: bool,
    ) -> RoleCheckOutcome {
        let mut result = RoleCheckOutcome::default();
        if self
            .pause
            .is_paused_at(context.now_unix_nanos.div_euclid(1_000_000_000) as i64)
        {
            return result;
        }
        for peer in region.learners() {
            let factory = PeerOperatorFactory::new(
                "promote-learner",
                BuilderContext {
                    stores: context.stores,
                    config: context.config,
                    rules: context.rules,
                    now_unix_nanos: context.now_unix_nanos,
                },
                region,
                supports_conf_change_v2,
            );
            match factory
                .create_promote_learner(peer.store_id, || {
                    Err(BuilderError::Allocation(
                        "unexpected learner recovery allocation".into(),
                    ))
                })
                .await
            {
                Ok(operator) => {
                    result.operator = Some(operator);
                    break;
                }
                Err(error) => result.errors.push(error),
            }
        }
        result
    }
}

/// Leave joint state, transferring a demoting leader first when possible.
#[derive(Default, Debug)]
pub struct JointStateChecker {
    pub pause: PauseController,
}
impl JointStateChecker {
    /// Recovery skips placement fitting in the canonical leave-joint factory.
    /// Factory errors produce no operation and remain available to the host.
    pub async fn check<V>(
        &self,
        context: BuilderContext<'_, V>,
        region: &RegionInfo,
    ) -> RoleCheckOutcome {
        let mut result = RoleCheckOutcome::default();
        if self
            .pause
            .is_paused_at(context.now_unix_nanos.div_euclid(1_000_000_000) as i64)
            || !is_in_joint_state(&region.meta().peers)
        {
            return result;
        }
        match RangeOperatorFactory::new("leave-joint-state", context, region, true)
            .create_leave_joint()
            .await
        {
            Ok(operator) => {
                operator.set_priority_level(PriorityLevel::HIGH);
                result.operator = Some(operator);
            }
            Err(error) => result.errors.push(error),
        }
        result
    }
}
