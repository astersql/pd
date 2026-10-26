// Copyright 2026 AsterSQL.
//! Bounded heartbeat queue and urgent witness-leader recovery transfers.
use crate::EvictLeaderContext;
use astersql_pd_core::RegionInfo;
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::SchedulerConfigProvider;
use astersql_pd_schedule_core::BasicCluster;
use astersql_pd_schedule_filter::StoreStateFilter;
use astersql_pd_schedule_operator::{
    BuilderContext, BuilderError, OpKind, Operator, PeerOperatorFactory,
};
use std::sync::{
    Arc,
    mpsc::{self, Receiver, SyncSender},
};
const NAME: &str = "transfer-witness-leader-scheduler";
/// Recovery failures are consumed, just like Go's per-event logged errors.
#[derive(Debug, thiserror::Error)]
pub enum TransferWitnessLeaderError {
    #[error("no target store to schedule")]
    NoTarget,
    #[error(transparent)]
    Builder(#[from] BuilderError),
}
#[derive(Default)]
pub struct TransferWitnessLeaderResult {
    pub operators: Vec<Arc<Operator>>,
    pub errors: Vec<TransferWitnessLeaderError>,
}
/// The host validates heartbeat eligibility and owns submission/admission. Events
/// retain immutable region snapshots; current store safety is read at consumption.
/// Cloned senders support concurrent heartbeat producers with Go-like backpressure.
pub struct TransferWitnessLeaderScheduler {
    sender: SyncSender<Arc<RegionInfo>>,
    receiver: Receiver<Arc<RegionInfo>>,
}
impl Default for TransferWitnessLeaderScheduler {
    fn default() -> Self {
        Self::new()
    }
}
impl TransferWitnessLeaderScheduler {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::sync_channel(10_000);
        Self { sender, receiver }
    }
    /// Blocking send and nonblocking try_send retain the bounded channel contract.
    pub fn recv_region_info(&self) -> SyncSender<Arc<RegionInfo>> {
        self.sender.clone()
    }
    /// Emergency recovery bypasses scheduler operator-count limits. Target store
    /// transfer-in budgets still apply, at Urgent priority.
    pub fn is_schedule_allowed(&self) -> bool {
        true
    }
    /// At most three events, including rejected and duplicate events, per call.
    pub async fn schedule<C: BasicCluster, P: SchedulerConfigProvider>(
        &mut self,
        ctx: &mut EvictLeaderContext<'_, C, P>,
    ) -> TransferWitnessLeaderResult {
        let mut out = TransferWitnessLeaderResult::default();
        for _ in 0..3 {
            let Ok(region) = self.receiver.try_recv() else {
                break;
            };
            match transfer(ctx, &region).await {
                Ok(op) => out.operators.push(Arc::new(op)),
                Err(e) => out.errors.push(e),
            }
        }
        out
    }
}
async fn transfer<C: BasicCluster, P: SchedulerConfigProvider>(
    ctx: &mut EvictLeaderContext<'_, C, P>,
    region: &RegionInfo,
) -> Result<Operator, TransferWitnessLeaderError> {
    let filter = StoreStateFilter {
        action_scope: NAME.into(),
        transfer_leader: true,
        operator_level: PriorityLevel::URGENT,
        now_unix_nanos: ctx.now_unix_nanos,
        ..Default::default()
    };
    let unhealthy: std::collections::HashSet<_> = region
        .down_peers()
        .iter()
        .map(|p| p.peer.store_id)
        .chain(region.pending_peers().iter().map(|p| p.store_id))
        .collect();
    let mut targets = vec![];
    for id in region.followers().keys() {
        if unhealthy.contains(id) {
            continue;
        }
        if let Some(store) = ctx.cluster.get_store(*id) {
            if filter.target(ctx.config, &store).await.is_ok() {
                targets.push(*id);
            }
        }
    }
    if targets.is_empty() {
        return Err(TransferWitnessLeaderError::NoTarget);
    }
    let target = targets[(ctx.random_index)(targets.len())];
    let rules = ctx.rules.get_rules_for_apply_region(region);
    let factory = PeerOperatorFactory::new(
        NAME,
        BuilderContext {
            stores: ctx.cluster.stores(),
            config: ctx.config,
            rules: &rules,
            now_unix_nanos: ctx.now_unix_nanos,
        },
        region,
        false,
    );
    let op = factory
        .create_transfer_leader(target, targets, OpKind::WITNESS_LEADER, || {
            Err(BuilderError::Allocation(
                "leader transfer must not allocate".into(),
            ))
        })
        .await?;
    op.set_priority_level(PriorityLevel::URGENT);
    Ok(op)
}
