// Copyright 2026 AsterSQL.
//! Peer/leader factories record intent on the canonical Builder.
use crate::{
    Builder, BuilderContext, BuilderError, BuilderOptions, OpKind, Operator, SimpleBuildOptions,
};
use astersql_pd_core::{Peer, PeerRole, RegionInfo};
use astersql_pd_schedule_placement::PeerRoleType;
use std::collections::BTreeMap;

/// One immutable region/config snapshot per factory request. The host supplies
/// the cluster-version capability, just as it supplies the peer ID allocator.
/// No runtime, protocol conversion, or independent membership planner is owned here.
pub struct PeerOperatorFactory<'a, V> {
    builder: Builder<'a, V>,
    joint: bool,
}
impl<'a, V> PeerOperatorFactory<'a, V> {
    pub fn new(
        description: &str,
        context: BuilderContext<'a, V>,
        region: &RegionInfo,
        supports_conf_change_v2: bool,
    ) -> Self {
        let joint = supports_conf_change_v2 && context.config.is_use_joint_consensus();
        Self {
            builder: Builder::new(description, context, region, BuilderOptions::default()),
            joint,
        }
    }
    /// Transfer factories alone skip the origin joint-state check. Forced
    /// transfers also skip placement fitting; role/health validation still applies.
    fn for_transfer(self, force: bool) -> Self {
        let b = self.builder;
        Self {
            builder: Builder::new(
                &b.description,
                b.context,
                &b.region,
                BuilderOptions {
                    skip_origin_joint_state_check: true,
                    skip_placement_rules_check: force,
                },
            ),
            joint: false,
        }
    }
    async fn finish<F>(&mut self, kind: OpKind, allocate: F) -> Result<Operator, BuilderError>
    where
        F: FnMut() -> Result<u64, BuilderError>,
    {
        if self.joint {
            self.builder
                .build_joint(kind, SimpleBuildOptions::default(), allocate)
                .await
        } else {
            self.builder
                .build_simple(kind, SimpleBuildOptions::default(), allocate)
                .await
        }
    }
    pub async fn create_add_peer<F>(
        mut self,
        peer: Option<Peer>,
        kind: OpKind,
        allocate: F,
    ) -> Result<Operator, BuilderError>
    where
        F: FnMut() -> Result<u64, BuilderError>,
    {
        self.builder.add_peer(peer);
        self.finish(kind, allocate).await
    }
    pub async fn create_remove_peer<F>(
        mut self,
        store: u64,
        kind: OpKind,
        allocate: F,
    ) -> Result<Operator, BuilderError>
    where
        F: FnMut() -> Result<u64, BuilderError>,
    {
        self.builder.remove_peer(store);
        self.finish(kind, allocate).await
    }
    pub async fn create_promote_learner<F>(
        mut self,
        store: u64,
        allocate: F,
    ) -> Result<Operator, BuilderError>
    where
        F: FnMut() -> Result<u64, BuilderError>,
    {
        self.builder.promote_learner(store);
        self.finish(OpKind(0), allocate).await
    }
    pub async fn create_demote_voter<F>(
        mut self,
        store: u64,
        allocate: F,
    ) -> Result<Operator, BuilderError>
    where
        F: FnMut() -> Result<u64, BuilderError>,
    {
        self.builder.demote_voter(store);
        self.finish(OpKind(0), allocate).await
    }
    pub async fn create_promote_learner_and_remove_peer<F>(
        mut self,
        promote: u64,
        remove: u64,
        allocate: F,
    ) -> Result<Operator, BuilderError>
    where
        F: FnMut() -> Result<u64, BuilderError>,
    {
        self.builder.promote_learner(promote).remove_peer(remove);
        self.finish(OpKind(0), allocate).await
    }
    pub async fn create_demote_learner_and_remove_peer<F>(
        mut self,
        demote: u64,
        remove: u64,
        allocate: F,
    ) -> Result<Operator, BuilderError>
    where
        F: FnMut() -> Result<u64, BuilderError>,
    {
        if !self.builder.context.config.is_use_joint_consensus() {
            return Err(BuilderError::JointConsensusDisabled);
        }
        self.builder.demote_voter(demote).remove_peer(remove);
        self.finish(OpKind(0), allocate).await
    }
    pub async fn create_transfer_leader<F>(
        self,
        target: u64,
        alternatives: Vec<u64>,
        kind: OpKind,
        allocate: F,
    ) -> Result<Operator, BuilderError>
    where
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let mut factory = self.for_transfer(false);
        factory.builder.set_leader(target).set_leaders(alternatives);
        factory.finish(kind, allocate).await
    }
    pub async fn create_force_transfer_leader<F>(
        self,
        target: u64,
        kind: OpKind,
        allocate: F,
    ) -> Result<Operator, BuilderError>
    where
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let mut factory = self.for_transfer(true);
        factory
            .builder
            .set_leader(target)
            .enable_force_target_leader();
        factory.finish(kind, allocate).await
    }
    pub async fn create_move_peer<F>(
        mut self,
        old: u64,
        peer: Option<Peer>,
        kind: OpKind,
        allocate: F,
    ) -> Result<Operator, BuilderError>
    where
        F: FnMut() -> Result<u64, BuilderError>,
    {
        self.builder.remove_peer(old).add_peer(peer);
        self.finish(kind, allocate).await
    }
    pub async fn create_move_leader<F>(
        mut self,
        old: u64,
        peer: Option<Peer>,
        kind: OpKind,
        allocate: F,
    ) -> Result<Operator, BuilderError>
    where
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let target = peer.as_ref().map_or(0, |p| p.store_id);
        self.builder
            .remove_peer(old)
            .add_peer(peer)
            .set_leader(target);
        self.finish(kind, allocate).await
    }
    pub async fn create_replace_leader_peer<F>(
        mut self,
        old: u64,
        peer: Option<Peer>,
        leader: u64,
        kind: OpKind,
        allocate: F,
    ) -> Result<Operator, BuilderError>
    where
        F: FnMut() -> Result<u64, BuilderError>,
    {
        self.builder
            .remove_peer(old)
            .add_peer(peer)
            .set_leader(leader);
        self.finish(kind, allocate).await
    }
    /// Role entries carry the explicit iteration order of the source map: Go
    /// assigns old-peer witness flags by iteration index, not by store identity.
    /// Preserve this observable assignment without introducing library randomness.
    pub async fn create_move_region<F>(
        mut self,
        roles: Vec<(u64, PeerRoleType)>,
        kind: OpKind,
        allocate: F,
    ) -> Result<Operator, BuilderError>
    where
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let mut peers = BTreeMap::new();
        let mut expected = BTreeMap::new();
        for (i, (store, role)) in roles.into_iter().enumerate() {
            if expected.insert(store, role.clone()).is_some() {
                self.builder.reject(BuilderError::InvalidPeerMap);
                break;
            }
            peers.insert(
                store,
                Peer {
                    store_id: store,
                    role: if role == PeerRoleType::Learner {
                        PeerRole::Learner
                    } else {
                        PeerRole::Voter
                    },
                    is_witness: self
                        .builder
                        .region
                        .meta()
                        .peers
                        .get(i)
                        .is_some_and(|p| p.is_witness),
                    ..Default::default()
                },
            );
        }
        self.builder.set_peers(peers).set_expected_roles(expected);
        self.finish(kind, allocate).await
    }
}
