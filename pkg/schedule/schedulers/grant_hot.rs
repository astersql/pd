// Copyright 2026 AsterSQL.
//! Administrative hot grants with explicit random inputs and host-owned state.
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
use astersql_pd_storage_domain::StorageEndpoint;
use std::{collections::HashSet, sync::Arc};
const NAME: &str = "grant-hot-region-scheduler";
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GrantHotConfig {
    #[serde(rename = "store-id", default)]
    pub store_ids: Vec<u64>,
    #[serde(rename = "store-leader-id", default)]
    pub leader_id: u64,
}
#[derive(Debug, thiserror::Error)]
pub enum GrantHotError {
    #[error("invalid scheduler config")]
    InvalidConfig,
    #[error("invalid unsigned store id")]
    InvalidId,
    #[error("region missing or unhealthy")]
    Region,
    #[error("source store missing")]
    Source,
    #[error("no eligible destination")]
    NoDestination,
    #[error(transparent)]
    Builder(#[from] BuilderError),
    #[error(transparent)]
    Storage(#[from] SchedulerError),
}
impl GrantHotConfig {
    pub fn set_store(&mut self, leader: u64, mut peers: Vec<u64>) {
        if !peers.contains(&leader) {
            peers.push(leader);
        }
        self.leader_id = leader;
        self.store_ids = peers;
    }
    pub fn from_args(args: &[&str]) -> Result<Self, GrantHotError> {
        if args.len() != 2 {
            return Err(GrantHotError::InvalidConfig);
        }
        fn id(s: &str) -> Result<u64, GrantHotError> {
            if s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()) {
                return Err(GrantHotError::InvalidId);
            }
            s.parse().map_err(|_| GrantHotError::InvalidId)
        }
        let mut out = Self::default();
        out.set_store(
            id(args[0])?,
            args[1].split(',').map(id).collect::<Result<_, _>>()?,
        );
        Ok(out)
    }
}
/// Real cluster and filter dependencies; only RNG, allocation and clock are boundaries.
pub struct GrantHotContext<'a, C: SharedConfigProvider> {
    pub cluster: &'a BasicCluster,
    pub config: &'a C,
    pub rules: &'a mut RuleManager,
    pub now: i128,
    pub supports_conf_change_v2: bool,
    pub allocate: &'a mut dyn FnMut() -> Result<u64, BuilderError>,
    pub active_regions: &'a HashSet<u64>,
}
pub struct GrantHotScheduler {
    config: GrantHotConfig,
}
#[derive(Default)]
pub struct GrantHotResult {
    pub operators: Vec<Arc<Operator>>,
    pub errors: Vec<GrantHotError>,
}
impl GrantHotScheduler {
    pub fn new(config: GrantHotConfig) -> Self {
        Self { config }
    }
    pub fn config(&self) -> &GrantHotConfig {
        &self.config
    }
    pub fn is_schedule_allowed(
        regions: u64,
        region_limit: u64,
        leaders: u64,
        leader_limit: u64,
    ) -> bool {
        regions < region_limit && leaders < leader_limit
    }
    pub fn encode_config(&self) -> Result<Vec<u8>, SchedulerError> {
        encode_config(&self.config)
    }
    pub async fn persist(
        &self,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), GrantHotError> {
        storage
            .save_scheduler_config(paths, NAME, &self.encode_config()?)
            .await
            .map_err(SchedulerError::from)?;
        Ok(())
    }
    /// HTTP adapters pass the raw JSON value; reject numeric IDs before mutation.
    pub async fn update_config(
        &mut self,
        input: &serde_json::Value,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), GrantHotError> {
        let leader = input["store-leader-id"]
            .as_str()
            .ok_or(GrantHotError::InvalidConfig)?;
        let stores = input["store-id"]
            .as_str()
            .ok_or(GrantHotError::InvalidConfig)?;
        self.config = GrantHotConfig::from_args(&[leader, stores])?;
        if let Err(e) = self.persist(storage, paths).await {
            self.config.leader_id = 0;
            return Err(e);
        }
        Ok(())
    }
    pub async fn reload(
        &mut self,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), GrantHotError> {
        let bytes = storage
            .load_scheduler_config(paths, NAME)
            .await
            .map_err(SchedulerError::from)?;
        self.config = decode_config(&bytes)?;
        Ok(())
    }
    /// Host prepares the four canonical resource snapshots before sampling.
    /// Separate words preserve independent resource, branch and target draws.
    pub async fn schedule<C: SharedConfigProvider>(
        &self,
        resources: &[Vec<StoreLoadDetail>; 4],
        random_words: [usize; 3],
        ctx: &mut GrantHotContext<'_, C>,
    ) -> GrantHotResult {
        let resource = HotResource::ALL[random_words[0] % HotResource::ALL.len()];
        self.dispatch(
            &resources[resource as usize],
            random_words[1] % 2 == 1,
            random_words[2],
            ctx,
        )
        .await
    }
    /// Go's isLeader=true visits unconfigured source stores; false visits configured
    /// nonleader stores. The sampled branch is independent of the resource type.
    pub async fn dispatch<C: SharedConfigProvider>(
        &self,
        details: &[StoreLoadDetail],
        is_leader: bool,
        random_word: usize,
        ctx: &mut GrantHotContext<'_, C>,
    ) -> GrantHotResult {
        let mut sorted: Vec<_> = details.iter().collect();
        sorted
            .sort_by(|a, b| b.load_pred.current.loads[0].total_cmp(&a.load_pred.current.loads[0]));
        let mut out = GrantHotResult::default();
        for detail in sorted {
            let src = detail.store_summary_info.store_info.id();
            if if is_leader {
                self.config.store_ids.contains(&src)
            } else {
                !self.config.store_ids.contains(&src) || src == self.config.leader_id
            } {
                continue;
            }
            for peer in &detail.hot_peers {
                if ctx.active_regions.contains(&peer.region_id) {
                    continue;
                }
                match self
                    .transfer(peer.region_id, src, is_leader, random_word, ctx)
                    .await
                {
                    Ok(op) => {
                        out.operators.push(Arc::new(op));
                        return out;
                    }
                    Err(e) => out.errors.push(e),
                }
            }
        }
        out
    }
    pub async fn transfer<C: SharedConfigProvider>(
        &self,
        region_id: u64,
        source: u64,
        is_leader: bool,
        random_word: usize,
        ctx: &mut GrantHotContext<'_, C>,
    ) -> Result<Operator, GrantHotError> {
        let region = ctx
            .cluster
            .regions
            .get_region(region_id)
            .ok_or(GrantHotError::Region)?;
        if !region.down_peers().is_empty() || !region.pending_peers().is_empty() {
            return Err(GrantHotError::Region);
        }
        let src = ctx
            .cluster
            .stores
            .get_store(source)
            .ok_or(GrantHotError::Source)?;
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
            transfer_leader: is_leader,
            move_region: !is_leader,
            operator_level: PriorityLevel::HIGH,
            now_unix_nanos: ctx.now,
            ..Default::default()
        };
        let evicted = HotRegionEvictedTargetFilter::new(NAME);
        let candidate = if is_leader {
            vec![self.config.leader_id]
        } else {
            self.config.store_ids.clone()
        };
        let mut destinations = vec![];
        for id in candidate {
            let Some(store) = ctx.cluster.stores.get_store(id) else {
                continue;
            };
            if !placement.target(ctx.config, &store).is_ok()
                || !state.target(ctx.config, &store).await.is_ok()
            {
                continue;
            }
            if !is_leader
                && (region.get_store_peer(id).is_some()
                    || !evicted.target(ctx.config, &store).is_ok())
            {
                continue;
            }
            destinations.push(id);
        }
        if destinations.is_empty() {
            return Err(GrantHotError::NoDestination);
        }
        let dst = destinations[random_word % destinations.len()];
        let rules = ctx.rules.get_rules_for_apply_region(&region);
        let factory = PeerOperatorFactory::new(
            if is_leader {
                "grant-hot-region-scheduler-leader"
            } else {
                "grant-hot-region-scheduler-move"
            },
            BuilderContext {
                stores: &ctx.cluster.stores,
                config: ctx.config,
                rules: &rules,
                now_unix_nanos: ctx.now,
            },
            &region,
            ctx.supports_conf_change_v2,
        );
        let op = if is_leader {
            factory
                .create_transfer_leader(dst, vec![], OpKind::LEADER, &mut *ctx.allocate)
                .await?
        } else {
            factory
                .create_move_peer(
                    source,
                    Some(Peer {
                        store_id: dst,
                        ..Default::default()
                    }),
                    OpKind::REGION | OpKind::LEADER,
                    &mut *ctx.allocate,
                )
                .await?
        };
        op.set_priority_level(PriorityLevel::HIGH);
        Ok(op)
    }
}
