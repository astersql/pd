// Copyright 2026 AsterSQL.
//! Builder target recording and validation over host-owned scheduling snapshots.
use crate::{OpKind, Operator};
use astersql_pd_core::{Peer, PeerRole, RegionInfo, StoresInfo, is_in_joint_state};
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_filter::StoreStateFilter;
use astersql_pd_schedule_placement::{PeerRoleType, Rule, fit_region, match_label_constraints};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

/// Checks skipped explicitly by the operation creator. Neither option skips
/// peer identity, target role, health, or store-state validation.
#[derive(Clone, Copy, Debug, Default)]
pub struct BuilderOptions {
    pub skip_origin_joint_state_check: bool,
    pub skip_placement_rules_check: bool,
}
/// Borrowed canonical state. `rules` is the ordered apply-range result from
/// RuleManager::get_rules_for_apply_region, not an independently maintained rule set.
pub struct BuilderContext<'a, V> {
    pub stores: &'a StoresInfo,
    pub config: &'a dyn SharedConfigProvider<Version = V>,
    pub rules: &'a [Arc<Rule>],
    pub now_unix_nanos: i128,
}
/// Stable domain errors; no protocol-specific debug rendering is required.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuilderError {
    InvalidOriginPeer,
    NoLeader,
    NoPlacementRule,
    OriginJoint,
    InvalidPeer,
    JointPeer,
    DuplicatePeer(u64),
    MissingPeer(u64),
    TargetLeaderRemoval(u64),
    NotLearner(u64),
    AlreadyLearner(u64),
    Unhealthy(u64),
    NotVoter(u64),
    InvalidPeerMap,
    MultipleLeaders,
    NoExpectedVoter,
    NoVoter,
    LeaderNotAllowed,
    NoSteps,
    NoValidLeader,
    Allocation(String),
    /// Target validation succeeded; step planning belongs to subsequent tasks.
    PlanningRequired,
}
impl std::fmt::Display for BuilderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSteps => f.write_str("no operator step is built"),
            Self::NoVoter => f.write_str("cannot create operator: target peers have no voter"),
            Self::LeaderNotAllowed => {
                f.write_str("cannot create operator: target leader is not allowed")
            }
            Self::NoExpectedVoter => f.write_str("region need at least 1 voter or leader"),
            Self::MultipleLeaders => f.write_str("region cannot have multiple leaders"),
            other => write!(f, "builder target error: {other:?}"),
        }
    }
}
impl std::error::Error for BuilderError {}
/// Mutable operation intent. The first error is sticky and all following
/// target mutations become no-ops. Cloned peers preserve input ownership.
pub struct Builder<'a, V> {
    pub(crate) context: BuilderContext<'a, V>,
    pub(crate) origin_peers: BTreeMap<u64, Peer>,
    pub(crate) target_peers: BTreeMap<u64, Peer>,
    pub(crate) origin_leader: u64,
    pub(crate) target_leader: u64,
    pub(crate) target_leaders: Vec<u64>,
    pub(crate) expected_roles: BTreeMap<u64, PeerRoleType>,
    unhealthy: BTreeSet<u64>,
    rules: Vec<Arc<Rule>>,
    options: BuilderOptions,
    pub(crate) force_target_leader: bool,
    pub(crate) description: String,
    pub(crate) region: RegionInfo,
    error: Option<BuilderError>,
}
impl<'a, V> Builder<'a, V> {
    /// Freeze origin peers and the applicable fitted rules for this build.
    pub fn new(
        description: &str,
        context: BuilderContext<'a, V>,
        region: &RegionInfo,
        options: BuilderOptions,
    ) -> Self {
        let mut error = None;
        let mut origin_peers = BTreeMap::new();
        for p in &region.meta().peers {
            if p.store_id == 0 {
                error = Some(BuilderError::InvalidOriginPeer);
                break;
            }
            origin_peers.insert(p.store_id, p.clone());
        }
        let origin_leader = region.leader().map_or(0, |p| p.store_id);
        if error.is_none() && !origin_peers.contains_key(&origin_leader) {
            error = Some(BuilderError::NoLeader);
        }
        let mut rules = vec![];
        if error.is_none()
            && !options.skip_placement_rules_check
            && context.config.is_placement_rules_enabled()
        {
            rules = fit_region(&context.stores.get_stores(), region, context.rules, true)
                .rule_fits
                .into_iter()
                .map(|f| f.rule)
                .collect();
            if rules.is_empty() {
                error = Some(BuilderError::NoPlacementRule);
            }
        }
        if error.is_none()
            && !options.skip_origin_joint_state_check
            && is_in_joint_state(&region.meta().peers)
        {
            error = Some(BuilderError::OriginJoint);
        }
        let unhealthy = region
            .pending_peers()
            .iter()
            .map(|p| p.store_id)
            .chain(region.down_peers().iter().map(|p| p.peer.store_id))
            .collect();
        Self {
            context,
            description: description.into(),
            region: region.clone(),
            target_peers: origin_peers.clone(),
            origin_peers,
            origin_leader,
            target_leader: 0,
            target_leaders: vec![],
            expected_roles: BTreeMap::new(),
            unhealthy,
            rules,
            options,
            force_target_leader: false,
            error,
        }
    }
    pub fn error(&self) -> Option<&BuilderError> {
        self.error.as_ref()
    }
    pub fn target_peers(&self) -> &BTreeMap<u64, Peer> {
        &self.target_peers
    }
    pub fn target_leader(&self) -> u64 {
        self.target_leader
    }
    pub fn target_leaders(&self) -> &[u64] {
        &self.target_leaders
    }
    pub(crate) fn reject(&mut self, error: BuilderError) {
        self.error = Some(error);
    }
    pub fn add_peer(&mut self, peer: Option<Peer>) -> &mut Self {
        if self.error.is_some() {
            return self;
        }
        match peer {
            None => self.reject(BuilderError::InvalidPeer),
            Some(p) if p.store_id == 0 => self.reject(BuilderError::InvalidPeer),
            Some(p) if is_in_joint_state(std::slice::from_ref(&p)) => {
                self.reject(BuilderError::JointPeer)
            }
            Some(p) if self.target_peers.contains_key(&p.store_id) => {
                self.reject(BuilderError::DuplicatePeer(p.store_id))
            }
            Some(p) => {
                self.target_peers.insert(p.store_id, p);
            }
        }
        self
    }
    pub fn remove_peer(&mut self, store: u64) -> &mut Self {
        if self.error.is_some() {
            return self;
        }
        if !self.target_peers.contains_key(&store) {
            self.reject(BuilderError::MissingPeer(store));
        } else if self.target_leader == store {
            self.reject(BuilderError::TargetLeaderRemoval(store));
        } else {
            self.target_peers.remove(&store);
        }
        self
    }
    pub fn promote_learner(&mut self, store: u64) -> &mut Self {
        if self.error.is_some() {
            return self;
        }
        match self.target_peers.get(&store) {
            None => self.reject(BuilderError::MissingPeer(store)),
            Some(p) if !p.is_learner() => self.reject(BuilderError::NotLearner(store)),
            Some(_) if self.unhealthy.contains(&store) => {
                self.reject(BuilderError::Unhealthy(store))
            }
            Some(_) => self.target_peers.get_mut(&store).unwrap().role = PeerRole::Voter,
        }
        self
    }
    pub fn demote_voter(&mut self, store: u64) -> &mut Self {
        if self.error.is_some() {
            return self;
        }
        match self.target_peers.get_mut(&store) {
            None => self.reject(BuilderError::MissingPeer(store)),
            Some(p) if p.is_learner() => self.reject(BuilderError::AlreadyLearner(store)),
            Some(p) => p.role = PeerRole::Learner,
        }
        self
    }
    pub fn set_leader(&mut self, store: u64) -> &mut Self {
        if self.error.is_some() {
            return self;
        }
        match self.target_peers.get(&store) {
            None => self.reject(BuilderError::MissingPeer(store)),
            Some(p) if p.is_learner() => self.reject(BuilderError::NotVoter(store)),
            Some(_) if self.unhealthy.contains(&store) => {
                self.reject(BuilderError::Unhealthy(store))
            }
            Some(_) => self.target_leader = store,
        }
        self
    }
    pub fn set_leaders(&mut self, mut stores: Vec<u64>) -> &mut Self {
        if self.error.is_some() {
            return self;
        }
        stores.sort_unstable();
        self.target_leaders.extend(stores.into_iter().filter(|s| {
            self.target_peers.get(s).is_some_and(|p| !p.is_learner()) && !self.unhealthy.contains(s)
        }));
        self
    }
    pub fn set_peers(&mut self, peers: BTreeMap<u64, Peer>) -> &mut Self {
        if self.error.is_some() {
            return self;
        }
        if peers
            .iter()
            .any(|(&s, p)| s == 0 || p.store_id != s || is_in_joint_state(std::slice::from_ref(p)))
        {
            self.reject(BuilderError::InvalidPeerMap);
        } else {
            if !peers.contains_key(&self.target_leader) {
                self.target_leader = 0;
            }
            self.target_peers = peers;
        }
        self
    }
    pub fn set_expected_roles(&mut self, roles: BTreeMap<u64, PeerRoleType>) -> &mut Self {
        if self.error.is_some() {
            return self;
        }
        let (mut leaders, mut voters) = (0, 0);
        for (&s, r) in &roles {
            match r {
                PeerRoleType::Leader => {
                    if leaders > 0 {
                        self.reject(BuilderError::MultipleLeaders);
                        return self;
                    }
                    self.target_leader = s;
                    leaders += 1;
                }
                PeerRoleType::Voter => voters += 1,
                PeerRoleType::Follower | PeerRoleType::Learner if self.target_leader == s => {
                    self.target_leader = 0
                }
                _ => (),
            }
        }
        if leaders + voters == 0 {
            self.reject(BuilderError::NoExpectedVoter);
        } else {
            self.expected_roles = roles;
        }
        self
    }
    pub fn enable_force_target_leader(&mut self) -> &mut Self {
        self.force_target_leader = true;
        self
    }
    /// Current leader bypasses store/placement checks, but never role/witness checks.
    pub async fn allow_leader(&self, peer: &Peer, ignore_cluster_limit: bool) -> bool {
        self.allow_leader_at(peer, ignore_cluster_limit, self.origin_leader)
            .await
    }
    pub(crate) async fn allow_leader_at(
        &self,
        peer: &Peer,
        ignore_cluster_limit: bool,
        current_leader: u64,
    ) -> bool {
        if matches!(peer.role, PeerRole::Learner | PeerRole::DemotingVoter) || peer.is_witness {
            return false;
        }
        if peer.store_id == current_leader {
            return true;
        }
        let Some(store) = self.context.stores.get_store(peer.store_id) else {
            return false;
        };
        if ignore_cluster_limit {
            return true;
        }
        let filter = StoreStateFilter {
            action_scope: "operator-builder".into(),
            transfer_leader: true,
            now_unix_nanos: self.context.now_unix_nanos,
            ..Default::default()
        };
        if !filter.target(self.context.config, &store).await.is_ok() {
            return false;
        }
        self.options.skip_placement_rules_check
            || self.rules.is_empty()
            || self.rules.iter().any(|r| {
                matches!(r.role, PeerRoleType::Leader | PeerRoleType::Voter)
                    && match_label_constraints(Some(&store), &r.label_constraints)
            })
    }
    /// Validate targets before planning. This deliberately does not allocate IDs
    /// or fabricate steps; a missing/learner selected leader is reset like Go.
    pub async fn validate_targets(&mut self) -> Result<(), BuilderError> {
        if let Some(e) = &self.error {
            return Err(e.clone());
        }
        let result = self.validate_targets_inner().await;
        if let Err(e) = &result {
            self.error = Some(e.clone());
        }
        result
    }
    async fn validate_targets_inner(&mut self) -> Result<(), BuilderError> {
        if !self.target_peers.values().any(|p| !p.is_learner()) {
            return Err(BuilderError::NoVoter);
        }
        if !self
            .target_peers
            .get(&self.target_leader)
            .is_some_and(|p| !p.is_learner())
        {
            self.target_leader = 0;
        }
        if self.target_leader != 0
            && !self
                .allow_leader(
                    &self.target_peers[&self.target_leader],
                    self.force_target_leader,
                )
                .await
        {
            return Err(BuilderError::LeaderNotAllowed);
        }
        Ok(())
    }
    /// No-change builds report the source's no-step error. Changed targets are
    /// explicitly handed to future planning rather than returning an empty operator.
    pub async fn build(&mut self, _kind: OpKind) -> Result<Operator, BuilderError> {
        self.validate_targets().await?;
        let same = self.target_peers.len() == self.origin_peers.len()
            && self.target_peers.iter().all(|(s, p)| {
                self.origin_peers
                    .get(s)
                    .is_some_and(|o| o.role == p.role && o.is_witness == p.is_witness)
            });
        let e = if same && (self.target_leader == 0 || self.target_leader == self.origin_leader) {
            BuilderError::NoSteps
        } else {
            BuilderError::PlanningRequired
        };
        self.reject(e.clone());
        Err(e)
    }
}

/// Test a non-forced candidate after the same origin admission as Builder::new.
pub async fn is_allowed_leader_target<V>(
    context: BuilderContext<'_, V>,
    region: &RegionInfo,
    peer: &Peer,
) -> bool {
    let b = Builder::new(
        "check-target-leader",
        context,
        region,
        BuilderOptions::default(),
    );
    b.error().is_none() && b.allow_leader(peer, false).await
}
