// Copyright 2026 AsterSQL.
//! Administrative peer intent over the canonical factory and controller.
use crate::{Handler, HandlerError};
use astersql_pd_core::{Peer, PeerRole, RegionInfo};
use astersql_pd_schedule_config::{CheckerConfigProvider, SchedulerConfigProvider};
use astersql_pd_schedule_operator::{BuilderContext, BuilderError, OpKind, PeerOperatorFactory};
use astersql_pd_schedule_placement::{PeerRoleType, Rule};
use std::{sync::Arc, time::Instant};

/// Domain request; transport parsing belongs to the adapter. Region role order
/// is explicit because the source factory assigns witness flags by map iteration.
#[derive(Clone, Debug)]
pub enum AdminPeerRequest {
    TransferLeader(u64),
    TransferRegion(Vec<(u64, PeerRoleType)>),
    TransferPeer { from: u64, to: u64 },
    AddPeer(u64),
    AddLearner(u64),
    RemovePeer(u64),
}
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum AdminPeerError {
    #[error(transparent)]
    Handler(#[from] HandlerError),
    #[error("invalid region ID")]
    InvalidRegionId,
    #[error("region {0} not found")]
    RegionNotFound(u64),
    #[error("region has no voter in store {0}")]
    NoVoter(u64),
    #[error("region has no peer in store {0}")]
    NoPeer(u64),
    #[error("region already has peer in store {0}")]
    ExistingPeer(u64),
    #[error("store {0} not found")]
    StoreNotFound(u64),
    #[error("store {0} removed")]
    StoreRemoved(u64),
    #[error("store {0} unhealthy")]
    StoreUnhealthy(u64),
    #[error("transfer region without peer role is not supported when placement rules enabled")]
    MissingRole,
    #[error(transparent)]
    Builder(#[from] BuilderError),
    #[error("failed to add operator: {0}")]
    Admission(astersql_pd_schedule_operator::AdmissionError),
}
/// Host capability/clock and the current RuleManager apply-range result. No
/// private state, allocator, runtime, or copied configuration is installed here.
pub struct AdminPeerContext<'a> {
    pub rules: &'a [Arc<Rule>],
    pub now_unix_nanos: i128,
    pub supports_conf_change_v2: bool,
    pub now: Instant,
}
impl<C: CheckerConfigProvider + SchedulerConfigProvider + Send + Sync + 'static> Handler<'_, C> {
    fn admin_region(&self, id: u64) -> Result<Arc<RegionInfo>, AdminPeerError> {
        self.coordinator()?
            .get_cluster()
            .regions
            .get_region(id)
            .ok_or(AdminPeerError::RegionNotFound(id))
    }
    /// Validate a decimal region identifier. Adapters map typed failures to HTTP
    /// 500 (unbootstrapped), 400 (invalid ID), or 404 (missing region).
    pub fn pre_check_for_region(&self, input: &str) -> Result<Arc<RegionInfo>, AdminPeerError> {
        self.coordinator()?;
        let digits = input;
        if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
            return Err(AdminPeerError::InvalidRegionId);
        }
        self.admin_region(
            digits
                .parse()
                .map_err(|_| AdminPeerError::InvalidRegionId)?,
        )
    }
    /// Validate management input, build real steps, and admit with Admin priority.
    /// ID allocation is a host I/O boundary; failure never inserts an operator.
    pub async fn add_peer_operator<F>(
        &self,
        region_id: u64,
        request: AdminPeerRequest,
        context: AdminPeerContext<'_>,
        allocate: F,
    ) -> Result<(), AdminPeerError>
    where
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let region = self.admin_region(region_id)?;
        let c = self.coordinator()?;
        let check_store = |id| {
            let s = c
                .get_cluster()
                .stores
                .get_store(id)
                .ok_or(AdminPeerError::StoreNotFound(id))?;
            if s.is_removed() {
                return Err(AdminPeerError::StoreRemoved(id));
            }
            if s.is_unhealthy(context.now_unix_nanos) {
                return Err(AdminPeerError::StoreUnhealthy(id));
            }
            Ok(())
        };
        let factory = |description: &str| {
            PeerOperatorFactory::new(
                description,
                BuilderContext {
                    stores: &c.get_cluster().stores,
                    config: &**c.get_config(),
                    rules: context.rules,
                    now_unix_nanos: context.now_unix_nanos,
                },
                &region,
                context.supports_conf_change_v2,
            )
        };
        let op = match request {
            AdminPeerRequest::TransferLeader(to) => {
                if !region
                    .meta()
                    .peers
                    .iter()
                    .any(|p| p.store_id == to && !p.is_learner())
                {
                    return Err(AdminPeerError::NoVoter(to));
                }
                factory("admin-transfer-leader")
                    .create_transfer_leader(to, vec![], OpKind::ADMIN, allocate)
                    .await?
            }
            AdminPeerRequest::TransferRegion(mut roles) => {
                for (_, role) in &mut roles {
                    if matches!(role, PeerRoleType::Unknown(s) if s.is_empty()) {
                        if c.get_config().is_placement_rules_enabled() {
                            return Err(AdminPeerError::MissingRole);
                        }
                        *role = PeerRoleType::Voter;
                    }
                }
                for (id, _) in &roles {
                    check_store(*id)?;
                }
                factory("admin-move-region")
                    .create_move_region(roles, OpKind::ADMIN, allocate)
                    .await?
            }
            AdminPeerRequest::TransferPeer { from, to } => {
                let old = region
                    .meta()
                    .peers
                    .iter()
                    .find(|p| p.store_id == from)
                    .ok_or(AdminPeerError::NoPeer(from))?;
                check_store(to)?;
                let peer = Peer {
                    store_id: to,
                    role: old.role,
                    is_witness: old.is_witness,
                    ..Default::default()
                };
                factory("admin-move-peer")
                    .create_move_peer(from, Some(peer), OpKind::ADMIN, allocate)
                    .await?
            }
            AdminPeerRequest::AddPeer(to) | AdminPeerRequest::AddLearner(to) => {
                if region.meta().peers.iter().any(|p| p.store_id == to) {
                    return Err(AdminPeerError::ExistingPeer(to));
                }
                check_store(to)?;
                let learner = matches!(request, AdminPeerRequest::AddLearner(_));
                let peer = Peer {
                    store_id: to,
                    role: if learner {
                        PeerRole::Learner
                    } else {
                        PeerRole::Voter
                    },
                    ..Default::default()
                };
                factory(if learner {
                    "admin-add-learner"
                } else {
                    "admin-add-peer"
                })
                .create_add_peer(Some(peer), OpKind::ADMIN, allocate)
                .await?
            }
            AdminPeerRequest::RemovePeer(from) => {
                if !region.meta().peers.iter().any(|p| p.store_id == from) {
                    return Err(AdminPeerError::NoPeer(from));
                }
                factory("admin-remove-peer")
                    .create_remove_peer(from, OpKind::ADMIN, allocate)
                    .await?
            }
        };
        let outcome = c
            .get_operator_controller()
            .add_operators(c.get_cluster(), &[Arc::new(op)])
            .map_err(AdminPeerError::Admission)?;
        self.archive_removed(&outcome.removed, context.now)?;
        Ok(())
    }
}
