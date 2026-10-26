// Copyright 2026 AsterSQL.
//! Random hot leader relocation over host-prepared statistics and real placement guards.
use crate::{HotResource, SchedulerError, decode_config, encode_config};
use astersql_pd_core::{BasicCluster, Peer};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_filter::{
    Filter, HotRegionEvictedTargetFilter, PlacementGuardContext, StoreStateFilter,
};
use astersql_pd_schedule_operator::{
    BuilderContext, BuilderError, OpKind, Operator, PeerOperatorFactory,
};
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_statistics::StoreLoadDetail;
use astersql_pd_storage_endpoint::StorageEndpoint;
use std::sync::Arc;
const NAME: &str = "shuffle-hot-region-scheduler";
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ShuffleHotConfig {
    #[serde(default)]
    pub limit: u64,
}
impl Default for ShuffleHotConfig {
    fn default() -> Self {
        Self { limit: 1 }
    }
}
impl ShuffleHotConfig {
    /// Go's slice decoder uses its default unless exactly one argument is supplied.
    pub fn from_args(args: &[&str]) -> Result<Self, ShuffleHotError> {
        if args.len() != 1 {
            return Ok(Self::default());
        }
        if args[0].is_empty() || !args[0].bytes().all(|c| c.is_ascii_digit()) {
            return Err(ShuffleHotError::InvalidLimit);
        }
        Ok(Self {
            limit: args[0].parse().map_err(|_| ShuffleHotError::InvalidLimit)?,
        })
    }
}
#[derive(Debug, thiserror::Error)]
pub enum ShuffleHotError {
    #[error("invalid unsigned limit")]
    InvalidLimit,
    #[error("hot region has no source store or leader peer")]
    Source,
    #[error(transparent)]
    Builder(#[from] BuilderError),
    #[error(transparent)]
    Storage(#[from] SchedulerError),
}
/// Clock and allocation are external boundaries; cluster, rules and configuration are canonical.
pub struct ShuffleHotContext<'a, C: SharedConfigProvider> {
    pub cluster: &'a BasicCluster,
    pub config: &'a C,
    pub rules: &'a mut RuleManager,
    pub now: i128,
    pub supports_conf_change_v2: bool,
    pub allocate: &'a mut dyn FnMut() -> Result<u64, BuilderError>,
}
#[derive(Default)]
pub struct ShuffleHotResult {
    pub operators: Vec<Arc<Operator>>,
    /// Preserve failures for host diagnostics; no operator is submitted by this library.
    pub errors: Vec<ShuffleHotError>,
}
pub struct ShuffleHotScheduler {
    config: ShuffleHotConfig,
}
impl ShuffleHotScheduler {
    pub fn new(config: ShuffleHotConfig) -> Self {
        Self { config }
    }
    pub fn config(&self) -> &ShuffleHotConfig {
        &self.config
    }
    /// Counts are hot-region, region and leader; all three comparisons are strict.
    pub fn is_schedule_allowed(
        &self,
        counts: [u64; 3],
        region_limit: u64,
        leader_limit: u64,
    ) -> bool {
        counts[0] < self.config.limit && counts[1] < region_limit && counts[2] < leader_limit
    }
    pub fn encode_config(&self) -> Result<Vec<u8>, SchedulerError> {
        encode_config(&self.config)
    }
    pub async fn persist(
        &self,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), ShuffleHotError> {
        storage
            .save_scheduler_config(paths, NAME, &self.encode_config()?)
            .await
            .map_err(SchedulerError::from)?;
        Ok(())
    }
    /// Publish after a successful write, preserving the handler's rollback outcome.
    /// The HTTP adapter owns JSON-number conversion; this domain API takes a typed limit.
    pub async fn update(
        &mut self,
        candidate: ShuffleHotConfig,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), ShuffleHotError> {
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
    ) -> Result<(), ShuffleHotError> {
        let bytes = storage
            .load_scheduler_config(paths, NAME)
            .await
            .map_err(SchedulerError::from)?;
        self.config = decode_config(&bytes)?;
        Ok(())
    }
    /// Host prepares fresh canonical resource snapshots. Words control independent
    /// resource, peer and destination draws. Slice order freezes Go's map traversal
    /// and cluster store order for reproducible tests; no load ranking is applied.
    pub async fn schedule<C: SharedConfigProvider>(
        &self,
        resources: &[Vec<StoreLoadDetail>; 4],
        random_words: [usize; 3],
        ctx: &mut ShuffleHotContext<'_, C>,
    ) -> ShuffleHotResult {
        let resource = HotResource::ALL[random_words[0] % 4];
        if !matches!(resource, HotResource::ReadLeader | HotResource::WriteLeader) {
            return ShuffleHotResult::default();
        }
        self.dispatch(
            &resources[resource as usize],
            random_words[1],
            random_words[2],
            ctx,
        )
        .await
    }
    pub async fn dispatch<C: SharedConfigProvider>(
        &self,
        details: &[StoreLoadDetail],
        peer_word: usize,
        destination_word: usize,
        ctx: &mut ShuffleHotContext<'_, C>,
    ) -> ShuffleHotResult {
        let mut out = ShuffleHotResult::default();
        for detail in details {
            if detail.hot_peers.is_empty() {
                continue;
            }
            let peer = &detail.hot_peers[peer_word % detail.hot_peers.len()];
            let Some(region) = ctx.cluster.regions.get_region(peer.region_id) else {
                continue;
            };
            if !region.down_peers().is_empty() || !region.pending_peers().is_empty() {
                continue;
            }
            let Some(leader) = region.leader() else {
                out.errors.push(ShuffleHotError::Source);
                return out;
            };
            let source = leader.store_id;
            let Some(src) = ctx.cluster.stores.get_store(source) else {
                out.errors.push(ShuffleHotError::Source);
                return out;
            };
            let stores = ctx.cluster.stores.get_stores();
            let mut guard = PlacementGuardContext {
                conf: ctx.config,
                stores: &stores,
                manager: ctx.rules,
                region: region.clone(),
                now: ctx.now,
            };
            let placement = guard.safeguard(NAME, &src, None);
            let state = StoreStateFilter {
                action_scope: NAME.into(),
                move_region: true,
                operator_level: PriorityLevel::LOW,
                now_unix_nanos: ctx.now,
                ..Default::default()
            };
            let evicted = HotRegionEvictedTargetFilter::new(NAME);
            let mut destinations = Vec::new();
            for store in &stores {
                if region.get_store_peer(store.id()).is_none()
                    && state.target(ctx.config, store).await.is_ok()
                    && evicted.target(ctx.config, store).is_ok()
                    && placement.target(ctx.config, store).is_ok()
                {
                    destinations.push(store.id());
                }
            }
            if destinations.is_empty() {
                return out;
            }
            // Normalize unspecified map traversal before applying the explicit random word.
            destinations.sort_unstable();
            let target = destinations[destination_word % destinations.len()];
            if target == 0 {
                return out;
            }
            if region.get_store_peer(source).is_none() {
                out.errors.push(ShuffleHotError::Source);
                return out;
            }
            let rules = ctx.rules.get_rules_for_apply_region(&region);
            let factory = PeerOperatorFactory::new(
                "random-move-hot-leader",
                BuilderContext {
                    stores: &ctx.cluster.stores,
                    config: ctx.config,
                    rules: &rules,
                    now_unix_nanos: ctx.now,
                },
                &region,
                ctx.supports_conf_change_v2,
            );
            match factory
                .create_move_leader(
                    source,
                    Some(Peer {
                        store_id: target,
                        ..Default::default()
                    }),
                    OpKind::REGION | OpKind::LEADER,
                    &mut *ctx.allocate,
                )
                .await
            {
                Ok(op) => {
                    op.set_priority_level(PriorityLevel::LOW);
                    out.operators.push(Arc::new(op));
                }
                Err(e) => out.errors.push(e.into()),
            }
            return out;
        }
        out
    }
}
