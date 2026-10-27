// Copyright 2026 AsterSQL.
//! Host-owned built-in factory table. Policies retain their canonical typed APIs.
use crate::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_types::*;
use astersql_pd_storage_domain::StorageEndpoint;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{collections::BTreeSet, sync::Arc};

#[derive(Debug, thiserror::Error)]
pub enum FactoryError {
    #[error(transparent)]
    Scheduler(#[from] SchedulerError),
    #[error("duplicated scheduler: {0}")]
    Duplicate(CheckerSchedulerType),
    #[error("invalid scheduler configuration: {0}")]
    Config(String),
    #[error("scheduler name not registered: {0}")]
    UnknownName(String),
    #[error("Go plugins are unsupported")]
    Plugin,
}
fn invalid(error: impl std::fmt::Display) -> FactoryError {
    FactoryError::Config(error.to_string())
}
fn value(config: impl Serialize) -> Result<Value, FactoryError> {
    serde_json::to_value(config).map_err(invalid)
}
fn decode<T: DeserializeOwned>(config: Value) -> Result<T, FactoryError> {
    serde_json::from_value(config).map_err(invalid)
}
/// Only external capabilities and I/O are supplied by the host. No private runtime.
pub struct FactoryContext {
    pub storage: Arc<StorageEndpoint>,
    pub paths: ClusterPaths,
    pub now: JobTime,
    pub query_supported: bool,
    pub cpu_supported: bool,
    pub hot_inputs: [HotRankInputs; 4],
}
impl FactoryContext {
    pub fn new<S: AsRef<StorageEndpoint> + ?Sized>(
        storage: Arc<S>,
        paths: ClusterPaths,
        now: JobTime,
    ) -> Self {
        let storage = Arc::new(storage.as_ref().as_ref().clone());
        Self {
            storage,
            paths,
            now,
            query_supported: true,
            cpu_supported: true,
            hot_inputs: std::array::from_fn(|_| HotRankInputs::default()),
        }
    }
}
/// Exhaustive concrete policies, never BaseScheduler or an empty scheduler.
/// The host matches the variant to supply the policy's typed scheduling context.
pub enum BuiltinScheduler {
    BalanceLeader(BalanceLeaderScheduler),
    BalanceRegion(BalanceRegionScheduler),
    BalanceWitness(BalanceWitnessScheduler),
    EvictLeader(EvictLeaderScheduler),
    EvictSlowStore {
        disk: EvictDiskScheduler,
        network: EvictNetworkScheduler,
    },
    EvictStoppingStore(EvictStoppingScheduler),
    GrantHot(GrantHotScheduler),
    Hot {
        scheduler: Box<HotScheduler>,
        config: HotRegionParams,
    },
    GrantLeader(GrantLeaderScheduler),
    Label(LabelScheduler),
    RandomMerge(RandomMergeScheduler),
    ScatterRange(ScatterRangeScheduler),
    ShuffleHot(ShuffleHotScheduler),
    ShuffleLeader(ShuffleLeaderScheduler),
    ShuffleRegion(ShuffleRegionScheduler),
    TransferWitness(TransferWitnessLeaderScheduler),
    SlowTrend(SlowTrendScheduler),
    BalanceRange {
        scheduler: BalanceRangeScheduler,
        jobs: BalanceRangeJobs,
    },
}
/// Factory output with default-scheduler disabled state, independent of HTTP.
pub struct RegisteredScheduler {
    pub policy: BuiltinScheduler,
    disabled: bool,
}
impl RegisteredScheduler {
    pub fn scheduler_type(&self) -> CheckerSchedulerType {
        match &self.policy {
            BuiltinScheduler::BalanceLeader(_) => BALANCE_LEADER_SCHEDULER,
            BuiltinScheduler::BalanceRegion(_) => BALANCE_REGION_SCHEDULER,
            BuiltinScheduler::BalanceWitness(_) => BALANCE_WITNESS_SCHEDULER,
            BuiltinScheduler::EvictLeader(_) => EVICT_LEADER_SCHEDULER,
            BuiltinScheduler::EvictSlowStore { .. } => EVICT_SLOW_STORE_SCHEDULER,
            BuiltinScheduler::EvictStoppingStore(_) => EVICT_STOPPING_STORE_SCHEDULER,
            BuiltinScheduler::GrantHot(_) => GRANT_HOT_REGION_SCHEDULER,
            BuiltinScheduler::Hot { .. } => BALANCE_HOT_REGION_SCHEDULER,
            BuiltinScheduler::GrantLeader(_) => GRANT_LEADER_SCHEDULER,
            BuiltinScheduler::Label(_) => LABEL_SCHEDULER,
            BuiltinScheduler::RandomMerge(_) => RANDOM_MERGE_SCHEDULER,
            BuiltinScheduler::ScatterRange(_) => SCATTER_RANGE_SCHEDULER,
            BuiltinScheduler::ShuffleHot(_) => SHUFFLE_HOT_REGION_SCHEDULER,
            BuiltinScheduler::ShuffleLeader(_) => SHUFFLE_LEADER_SCHEDULER,
            BuiltinScheduler::ShuffleRegion(_) => SHUFFLE_REGION_SCHEDULER,
            BuiltinScheduler::TransferWitness(_) => TRANSFER_WITNESS_LEADER_SCHEDULER,
            BuiltinScheduler::SlowTrend(_) => EVICT_SLOW_TREND_SCHEDULER,
            BuiltinScheduler::BalanceRange { .. } => BALANCE_RANGE_SCHEDULER,
        }
    }
    pub fn name(&self) -> String {
        match &self.policy {
            BuiltinScheduler::ScatterRange(s) => s.name().into(),
            _ => self.scheduler_type().as_str().into(),
        }
    }
    pub fn is_default(&self) -> bool {
        current_kernel_default_schedulers().contains(&self.scheduler_type())
    }
    pub fn is_disabled(&self) -> bool {
        self.supports_disabled() && self.disabled
    }
    pub fn set_disabled(&mut self, disabled: bool) {
        if self.supports_disabled() {
            self.disabled = disabled;
        }
    }
    fn supports_disabled(&self) -> bool {
        matches!(
            self.policy,
            BuiltinScheduler::BalanceLeader(_)
                | BuiltinScheduler::BalanceRegion(_)
                | BuiltinScheduler::Hot { .. }
                | BuiltinScheduler::EvictSlowStore { .. }
                | BuiltinScheduler::EvictStoppingStore(_)
                | BuiltinScheduler::BalanceRange { .. }
        )
    }
    pub fn encode_config(&self) -> Result<Vec<u8>, FactoryError> {
        let mut config = match &self.policy {
            BuiltinScheduler::BalanceLeader(s) => value(&s.config)?,
            BuiltinScheduler::BalanceRegion(s) => value(&s.config)?,
            BuiltinScheduler::BalanceWitness(s) => value(&s.config)?,
            BuiltinScheduler::EvictLeader(s) => value(s.config())?,
            BuiltinScheduler::EvictSlowStore { disk, network } => {
                let mut config = network.config().clone();
                config.disk = disk.config().clone();
                value(config)?
            }
            BuiltinScheduler::EvictStoppingStore(s) => value(s.config())?,
            BuiltinScheduler::GrantHot(s) => value(s.config())?,
            BuiltinScheduler::Hot { config, .. } => value(config)?,
            BuiltinScheduler::GrantLeader(s) => value(s.config())?,
            BuiltinScheduler::Label(s) => value(s.config())?,
            BuiltinScheduler::RandomMerge(s) => value(s.config())?,
            BuiltinScheduler::ScatterRange(s) => value(s.config())?,
            BuiltinScheduler::ShuffleHot(s) => value(s.config())?,
            BuiltinScheduler::ShuffleLeader(s) => value(s.config())?,
            BuiltinScheduler::ShuffleRegion(s) => value(s.config())?,
            BuiltinScheduler::TransferWitness(_) => Value::Null,
            BuiltinScheduler::SlowTrend(s) => value(s.config())?,
            BuiltinScheduler::BalanceRange { jobs, .. } => {
                return jobs.encode_config().map_err(invalid);
            }
        };
        if self.supports_disabled() {
            config["disabled"] = Value::Bool(self.disabled);
        }
        Ok(encode_config(&config)?)
    }
    pub async fn save_config(&self, ctx: &FactoryContext) -> Result<(), FactoryError> {
        let data = self.encode_config()?;
        ctx.storage
            .save_scheduler_config(&ctx.paths, &self.name(), &data)
            .await
            .map_err(SchedulerError::from)?;
        Ok(())
    }
}
/// Explicit registration avoids global state. Default is empty, Register is idempotent.
#[derive(Default)]
pub struct SchedulerRegistry {
    types: BTreeSet<CheckerSchedulerType>,
    builtins_registered: bool,
}
impl SchedulerRegistry {
    pub fn registered_types(&self) -> impl Iterator<Item = &CheckerSchedulerType> {
        self.types.iter()
    }
    pub fn register_type(&mut self, typ: CheckerSchedulerType) -> Result<(), FactoryError> {
        if typ.as_str().contains("plugin") {
            return Err(FactoryError::Plugin);
        }
        if !SCHEDULERS.contains(&typ) {
            return Err(SchedulerError::NotRegistered(typ).into());
        }
        if !self.types.insert(typ.clone()) {
            return Err(FactoryError::Duplicate(typ));
        }
        Ok(())
    }
    pub fn register_builtins(&mut self) -> Result<(), FactoryError> {
        if self.builtins_registered {
            return Ok(());
        }
        if let Some(typ) = SCHEDULERS.iter().find(|t| self.types.contains(*t)) {
            return Err(FactoryError::Duplicate(typ.clone()));
        }
        for typ in SCHEDULERS {
            self.register_type(typ)?;
        }
        self.builtins_registered = true;
        Ok(())
    }
    pub fn lookup(&self, typ: &CheckerSchedulerType) -> Result<(), SchedulerError> {
        self.types
            .contains(typ)
            .then_some(())
            .ok_or_else(|| SchedulerError::NotRegistered(typ.clone()))
    }
    pub fn find_type_by_name(&self, name: &str) -> Option<CheckerSchedulerType> {
        self.types
            .iter()
            .filter(|t| name.contains(t.as_str()))
            .max_by_key(|t| t.as_str().len())
            .cloned()
            .or_else(|| {
                name.contains("scatter-range")
                    .then_some(SCATTER_RANGE_SCHEDULER)
            })
    }
    pub fn decode_slice<T: DeserializeOwned>(
        &self,
        typ: &CheckerSchedulerType,
        args: &[String],
    ) -> Result<T, SchedulerError> {
        if !self.types.contains(typ) {
            return Err(SchedulerError::DecoderNotRegistered(typ.clone()));
        }
        let args: Vec<_> = args.iter().map(String::as_str).collect();
        let data = self
            .args_config(typ, &args, JobTime(0))
            .map_err(|e| SchedulerError::InvalidConfig(e.to_string()))?;
        serde_json::from_value(data).map_err(SchedulerError::Decode)
    }
    pub fn create_args(
        &self,
        typ: &CheckerSchedulerType,
        args: &[&str],
        ctx: &FactoryContext,
    ) -> Result<RegisteredScheduler, FactoryError> {
        self.lookup(typ)?;
        let config = self.args_config(typ, args, ctx.now)?;
        self.build(typ, config, ctx)
    }
    pub fn create_json(
        &self,
        typ: &CheckerSchedulerType,
        bytes: &[u8],
        ctx: &FactoryContext,
    ) -> Result<RegisteredScheduler, FactoryError> {
        self.lookup(typ)?;
        // Go's transfer-witness factory never invokes its supplied decoder.
        if *typ == TRANSFER_WITNESS_LEADER_SCHEDULER {
            return self.build(typ, json!({}), ctx);
        }
        let patch: Value = decode_config(bytes)?;
        if *typ == BALANCE_RANGE_SCHEDULER {
            return self.build(typ, patch, ctx);
        }
        let mut config = self.defaults(typ)?;
        if !patch.is_null() {
            let fields = patch
                .as_object()
                .ok_or_else(|| invalid("expected configuration object"))?;
            // Go uses the legacy priority and ranking baseline for nonempty persisted hot config.
            if *typ == BALANCE_HOT_REGION_SCHEDULER && !fields.is_empty() {
                config["read-priorities"] = json!(["byte", "key"]);
                config["write-leader-priorities"] = json!(["key", "byte"]);
                config["write-peer-priorities"] = json!(["byte", "key"]);
                config["rank-formula-version"] = json!("");
            }
            for (key, val) in fields {
                // encoding/json leaves scalar fields unchanged on null.
                if val.is_null()
                    && (config[key].is_number()
                        || config[key].is_string()
                        || config[key].is_boolean())
                {
                    continue;
                }
                config[key] = val.clone();
            }
        }
        if matches!(
            typ.as_str(),
            "balance-leader-scheduler" | "balance-witness-scheduler" | "evict-leader-scheduler"
        ) && config["batch"] == 0
        {
            config["batch"] = self.defaults(typ)?["batch"].clone();
        }
        self.build(typ, config, ctx)
    }
    pub async fn restore(
        &self,
        name: &str,
        ctx: &FactoryContext,
    ) -> Result<RegisteredScheduler, FactoryError> {
        let typ = self
            .find_type_by_name(name)
            .ok_or_else(|| FactoryError::UnknownName(name.into()))?;
        let data = ctx
            .storage
            .load_scheduler_config(&ctx.paths, name)
            .await
            .map_err(SchedulerError::from)?;
        self.create_json(&typ, &data, ctx)
    }
    fn defaults(&self, typ: &CheckerSchedulerType) -> Result<Value, FactoryError> {
        match typ.as_str() {
            "balance-leader-scheduler" => value(BalanceLeaderConfig::default()),
            "balance-region-scheduler" => value(BalanceRegionConfig::default()),
            "balance-witness-scheduler" => value(BalanceWitnessConfig::default()),
            "evict-leader-scheduler" => {
                value(EvictLeaderConfig::new(Default::default(), 3).map_err(invalid)?)
            }
            "evict-slow-store-scheduler" => value(EvictNetworkConfig::default()),
            "evict-stopping-store-scheduler" => value(EvictStoppingConfig::default()),
            "grant-hot-region-scheduler" => value(GrantHotConfig::default()),
            "balance-hot-region-scheduler" => value(HotRegionParams::default()),
            "grant-leader-scheduler" => value(GrantLeaderConfig::new(Default::default())),
            "label-scheduler" => value(LabelConfig::default()),
            "random-merge-scheduler" => value(RandomMergeConfig::default()),
            "scatter-range-scheduler" => value(ScatterRangeConfig::default()),
            "shuffle-hot-region-scheduler" => value(ShuffleHotConfig::default()),
            "shuffle-leader-scheduler" => value(ShuffleLeaderConfig::default()),
            "shuffle-region-scheduler" => value(ShuffleRegionConfig::default()),
            "transfer-witness-leader-scheduler" => Ok(json!({})),
            "evict-slow-trend-scheduler" => value(SlowTrendConfig::default()),
            "balance-range-scheduler" => Ok(json!([])),
            _ => Err(SchedulerError::NotRegistered(typ.clone()).into()),
        }
    }
    fn args_config(
        &self,
        typ: &CheckerSchedulerType,
        args: &[&str],
        now: JobTime,
    ) -> Result<Value, FactoryError> {
        match typ.as_str() {
            "balance-leader-scheduler" => value(
                BalanceLeaderConfig::new(get_key_ranges(args).map_err(invalid)?, 4)
                    .map_err(invalid)?,
            ),
            "balance-region-scheduler" => value(BalanceRegionConfig::new(
                get_key_ranges(args).map_err(invalid)?,
            )),
            "balance-witness-scheduler" => value(
                BalanceWitnessConfig::new(get_key_ranges(args).map_err(invalid)?, 4)
                    .map_err(invalid)?,
            ),
            "evict-leader-scheduler" => value(EvictLeaderConfig::from_args(args).map_err(invalid)?),
            "grant-leader-scheduler" => value(GrantLeaderConfig::from_args(args).map_err(invalid)?),
            "grant-hot-region-scheduler" => {
                value(GrantHotConfig::from_args(args).map_err(invalid)?)
            }
            "label-scheduler" => value(LabelConfig::from_args(args).map_err(invalid)?),
            "random-merge-scheduler" => value(RandomMergeConfig::from_args(args).map_err(invalid)?),
            "scatter-range-scheduler" => {
                if args.len() != 3 {
                    return Err(invalid("ranges and name require exactly three arguments"));
                }
                value(ScatterRangeConfig::from_args(&[args[2], args[0], args[1]]).map_err(invalid)?)
            }
            "shuffle-hot-region-scheduler" => {
                value(ShuffleHotConfig::from_args(args).map_err(invalid)?)
            }
            "shuffle-leader-scheduler" => {
                value(ShuffleLeaderConfig::from_args(args).map_err(invalid)?)
            }
            "shuffle-region-scheduler" => {
                value(ShuffleRegionConfig::from_args(args).map_err(invalid)?)
            }
            "balance-range-scheduler" => value(vec![
                BalanceRangeJob::from_args(args, now).map_err(invalid)?,
            ]),
            _ => self.defaults(typ), // Go deliberately ignores slice args for these types.
        }
    }
    fn build(
        &self,
        typ: &CheckerSchedulerType,
        config: Value,
        ctx: &FactoryContext,
    ) -> Result<RegisteredScheduler, FactoryError> {
        let has_disabled = matches!(
            typ.as_str(),
            "balance-leader-scheduler"
                | "balance-region-scheduler"
                | "balance-hot-region-scheduler"
                | "evict-slow-store-scheduler"
                | "evict-stopping-store-scheduler"
        );
        let disabled = match config.get("disabled").filter(|_| has_disabled) {
            None | Some(Value::Null) => false,
            Some(Value::Bool(b)) => *b,
            _ => return Err(invalid("disabled must be a boolean")),
        };
        let policy = match typ.as_str() {
            "balance-leader-scheduler" => {
                let mut s = BalanceLeaderScheduler::default();
                s.config = decode(config)?;
                BuiltinScheduler::BalanceLeader(s)
            }
            "balance-region-scheduler" => {
                let mut s = BalanceRegionScheduler::default();
                s.config = decode(config)?;
                BuiltinScheduler::BalanceRegion(s)
            }
            "balance-witness-scheduler" => {
                let mut s = BalanceWitnessScheduler::default();
                s.config = decode(config)?;
                BuiltinScheduler::BalanceWitness(s)
            }
            "evict-leader-scheduler" => {
                BuiltinScheduler::EvictLeader(EvictLeaderScheduler::new(decode(config)?))
            }
            "evict-slow-store-scheduler" => {
                let config: EvictNetworkConfig = decode(config)?;
                BuiltinScheduler::EvictSlowStore {
                    disk: EvictDiskScheduler::new(config.disk.clone()),
                    network: EvictNetworkScheduler::new(config),
                }
            }
            "evict-stopping-store-scheduler" => {
                BuiltinScheduler::EvictStoppingStore(EvictStoppingScheduler::new(decode(config)?))
            }
            "grant-hot-region-scheduler" => {
                BuiltinScheduler::GrantHot(GrantHotScheduler::new(decode(config)?))
            }
            "balance-hot-region-scheduler" => {
                let config: HotRegionParams = decode(config)?;
                let scheduler = Box::new(
                    HotScheduler::new(
                        config.clone(),
                        ctx.query_supported,
                        ctx.cpu_supported,
                        ctx.hot_inputs.clone(),
                    )
                    .map_err(invalid)?,
                );
                BuiltinScheduler::Hot { scheduler, config }
            }
            "grant-leader-scheduler" => {
                BuiltinScheduler::GrantLeader(GrantLeaderScheduler::new(decode(config)?))
            }
            "label-scheduler" => BuiltinScheduler::Label(LabelScheduler::new(decode(config)?)),
            "random-merge-scheduler" => {
                BuiltinScheduler::RandomMerge(RandomMergeScheduler::new(decode(config)?))
            }
            "scatter-range-scheduler" => {
                let config: ScatterRangeConfig = decode(config)?;
                if config.range_name.is_empty() {
                    return Err(invalid("missing range name"));
                }
                BuiltinScheduler::ScatterRange(ScatterRangeScheduler::new(config))
            }
            "shuffle-hot-region-scheduler" => {
                BuiltinScheduler::ShuffleHot(ShuffleHotScheduler::new(decode(config)?))
            }
            "shuffle-leader-scheduler" => {
                BuiltinScheduler::ShuffleLeader(ShuffleLeaderScheduler::new(decode(config)?))
            }
            "shuffle-region-scheduler" => {
                BuiltinScheduler::ShuffleRegion(ShuffleRegionScheduler::new(decode(config)?))
            }
            "transfer-witness-leader-scheduler" => {
                BuiltinScheduler::TransferWitness(TransferWitnessLeaderScheduler::new())
            }
            "evict-slow-trend-scheduler" => {
                BuiltinScheduler::SlowTrend(SlowTrendScheduler::new(decode(config)?))
            }
            "balance-range-scheduler" => BuiltinScheduler::BalanceRange {
                scheduler: BalanceRangeScheduler::default(),
                jobs: BalanceRangeJobs::from_json(
                    ctx.storage.clone(),
                    ctx.paths.clone(),
                    &encode_config(&config)?,
                )
                .map_err(invalid)?,
            },
            _ => return Err(SchedulerError::NotRegistered(typ.clone()).into()),
        };
        Ok(RegisteredScheduler { policy, disabled })
    }
}
