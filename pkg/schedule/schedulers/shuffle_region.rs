// Copyright 2026 AsterSQL.
//! Role-selective random peer relocation using canonical indexes and placement.
use crate::{
    EvictLeaderContext, EvictLeaderResult, KeyRangeDecodeError, SchedulerError,
    ShuffleLeaderConfig, decode_config, encode_config,
};
use astersql_pd_core::{KeyRange, Peer};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_core::BasicCluster;
use astersql_pd_schedule_filter::{
    Filter, PlacementGuardContext, RegionDownFilter, RegionPendingFilter, RegionReplicatedFilter,
    RegionReplicationContext, SpecialUseFilter, StoreStateFilter, select_one_region,
};
use astersql_pd_schedule_operator::{BuilderContext, BuilderError, OpKind, PeerOperatorFactory};
use astersql_pd_storage_endpoint::StorageEndpoint;
use std::{cell::RefCell, sync::Arc};
const NAME: &str = "shuffle-region-scheduler";
/// Persisted JSON preserves null/empty role lists. Validation belongs to role updates,
/// as Go's reload decodes strings without invoking its HTTP role validator.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ShuffleRegionConfig {
    #[serde(flatten)]
    ranges: ShuffleLeaderConfig,
    #[serde(default)]
    roles: Option<Vec<String>>,
}
impl ShuffleRegionConfig {
    pub fn from_args(args: &[&str]) -> Result<Self, KeyRangeDecodeError> {
        Ok(Self {
            ranges: ShuffleLeaderConfig::from_args(args)?,
            roles: Some(vec!["leader".into(), "follower".into(), "learner".into()]),
        })
    }
    pub fn ranges(&self) -> Vec<KeyRange> {
        self.ranges.ranges()
    }
    pub fn roles(&self) -> Option<&[String]> {
        self.roles.as_deref()
    }
    pub fn is_role_allowed(&self, role: &str) -> bool {
        self.roles
            .as_ref()
            .is_some_and(|v| v.iter().any(|r| r == role))
    }
    pub fn set_roles(&mut self, roles: Vec<String>) -> Result<(), ShuffleRegionError> {
        for r in &roles {
            if !matches!(r.as_str(), "leader" | "follower" | "learner") {
                return Err(ShuffleRegionError::InvalidRole(r.clone()));
            }
        }
        self.roles = Some(roles);
        Ok(())
    }
}
#[derive(Debug, thiserror::Error)]
pub enum ShuffleRegionError {
    #[error("invalid role:{0}")]
    InvalidRole(String),
    #[error(transparent)]
    Config(#[from] SchedulerError),
}
/// Host owns admission counts, time, randomness, peer allocation and submission.
pub struct ShuffleRegionScheduler {
    config: ShuffleRegionConfig,
}
impl ShuffleRegionScheduler {
    pub fn new(config: ShuffleRegionConfig) -> Self {
        Self { config }
    }
    pub fn config(&self) -> &ShuffleRegionConfig {
        &self.config
    }
    pub fn encode_config(&self) -> Result<Vec<u8>, SchedulerError> {
        encode_config(&self.config)
    }
    pub fn is_schedule_allowed(count: u64, limit: u64) -> bool {
        count < limit
    }
    pub async fn persist(
        &self,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), ShuffleRegionError> {
        storage
            .save_scheduler_config(paths, NAME, &self.encode_config()?)
            .await
            .map_err(SchedulerError::from)?;
        Ok(())
    }
    /// Publish only after persistence, retaining the prior roles on validation or I/O error.
    pub async fn update_roles(
        &mut self,
        roles: Vec<String>,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), ShuffleRegionError> {
        self.update_optional_roles(Some(roles), storage, paths)
            .await
    }
    /// Preserve JSON null separately from an empty role list, publishing after I/O.
    pub async fn update_optional_roles(
        &mut self,
        roles: Option<Vec<String>>,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), ShuffleRegionError> {
        let mut candidate = self.config.clone();
        if let Some(roles) = &roles {
            candidate.set_roles(roles.clone())?;
        }
        candidate.roles = roles;
        storage
            .save_scheduler_config(paths, NAME, &encode_config(&candidate)?)
            .await
            .map_err(SchedulerError::from)?;
        self.config = candidate;
        Ok(())
    }
    pub async fn reload(
        &mut self,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), ShuffleRegionError> {
        let data = storage
            .load_scheduler_config(paths, NAME)
            .await
            .map_err(SchedulerError::from)?;
        self.config = decode_config(&data)?;
        Ok(())
    }
    pub async fn schedule<C: BasicCluster, P: SharedConfigProvider>(
        &self,
        ctx: &mut EvictLeaderContext<'_, C, P>,
        supports_conf_change_v2: bool,
        allocate: &mut dyn FnMut() -> Result<u64, BuilderError>,
    ) -> EvictLeaderResult {
        let mut out = EvictLeaderResult::default();
        let state = StoreStateFilter {
            action_scope: NAME.into(),
            move_region: true,
            operator_level: PriorityLevel::LOW,
            now_unix_nanos: ctx.now_unix_nanos,
            ..Default::default()
        };
        let special = SpecialUseFilter::new(NAME, &[]);
        let mut stores = ctx.cluster.get_stores();
        stores.sort_by_key(|s| s.id());
        let mut sources = vec![];
        for (i, s) in stores.iter().enumerate() {
            if state.source(ctx.config, s).await.is_ok() && special.source(ctx.config, s).is_ok() {
                sources.push(i);
            }
        }
        (ctx.shuffle)(&mut sources);
        let ranges = self.config.ranges();
        let mut selected = None;
        'sources: for i in sources {
            let source = &stores[i];
            for role in ["follower", "leader", "learner"] {
                if !self.config.is_role_allowed(role) {
                    continue;
                }
                let regions = match role {
                    "follower" => ctx.cluster.rand_follower_regions(
                        source.id(),
                        &ranges,
                        ctx.random_index,
                        ctx.shuffle,
                    ),
                    "leader" => ctx.cluster.rand_leader_regions(
                        source.id(),
                        &ranges,
                        ctx.random_index,
                        ctx.shuffle,
                    ),
                    _ => ctx.cluster.rand_learner_regions(
                        source.id(),
                        &ranges,
                        ctx.random_index,
                        ctx.shuffle,
                    ),
                };
                let replica = RegionReplicatedFilter::new(RegionReplicationContext {
                    conf: ctx.config,
                    stores: &stores,
                    manager: RefCell::new(&mut *ctx.rules),
                    now: ctx.now_unix_nanos,
                });
                if let Some(region) = select_one_region(
                    &regions,
                    None,
                    &[&RegionPendingFilter, &RegionDownFilter, &replica],
                ) {
                    selected = Some((source.clone(), region));
                    break 'sources;
                }
            }
        }
        let Some((source, region)) = selected else {
            return out;
        };
        let placement = PlacementGuardContext {
            conf: ctx.config,
            stores: &stores,
            manager: ctx.rules,
            region: region.clone(),
            now: ctx.now_unix_nanos,
        }
        .safeguard(NAME, &source, None);
        let mut targets = vec![];
        for store in &stores {
            if region.get_store_peer(store.id()).is_none()
                && state.target(ctx.config, store).await.is_ok()
                && special.target(ctx.config, store).is_ok()
                && placement.target(ctx.config, store).is_ok()
            {
                targets.push(store.id());
            }
        }
        if targets.is_empty() {
            return out;
        }
        let target = targets[(ctx.random_index)(targets.len())];
        let old = region
            .get_store_peer(source.id())
            .expect("canonical role index contains source peer");
        let rules = ctx.rules.get_rules_for_apply_region(&region);
        let factory = PeerOperatorFactory::new(
            NAME,
            BuilderContext {
                stores: ctx.cluster.stores(),
                config: ctx.config,
                rules: &rules,
                now_unix_nanos: ctx.now_unix_nanos,
            },
            &region,
            supports_conf_change_v2,
        );
        match factory
            .create_move_peer(
                source.id(),
                Some(Peer {
                    store_id: target,
                    role: old.role,
                    ..Default::default()
                }),
                OpKind::REGION,
                allocate,
            )
            .await
        {
            Ok(op) => {
                op.set_priority_level(PriorityLevel::LOW);
                out.operators.push(Arc::new(op));
            }
            Err(e) => out.errors.push(e),
        }
        out
    }
}
