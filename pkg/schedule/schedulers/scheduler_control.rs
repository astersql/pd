// Copyright 2026 AsterSQL.
//! Exclusive host-owned management state. The scheduling loop is a separate owner.
use crate::*;
use astersql_pd_core::{BasicCluster, StoreControlError};
use astersql_pd_schedule_types::scheduler_type_compatible;
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Debug, thiserror::Error)]
pub enum ControlError {
    #[error("cluster is not bootstrapped")]
    NotBootstrapped,
    #[error("scheduler already exists: {0}")]
    Exists(String),
    #[error("scheduler not found: {0}")]
    NotFound(String),
    #[error(transparent)]
    Factory(#[from] FactoryError),
    #[error(transparent)]
    Scheduler(#[from] SchedulerError),
    #[error("scheduler preparation failed: {0:?}")]
    Prepare(Vec<StoreControlError>),
    #[error(transparent)]
    GrantHot(#[from] GrantHotError),
    #[error(transparent)]
    ShuffleHot(#[from] ShuffleHotError),
    #[error(transparent)]
    ShuffleRegion(#[from] ShuffleRegionError),
    #[error(transparent)]
    RangeJob(#[from] RangeJobError),
    #[error(transparent)]
    Hot(#[from] HotScheduleError),
}
impl From<astersql_pd_storage_endpoint::EndpointError> for ControlError {
    fn from(e: astersql_pd_storage_endpoint::EndpointError) -> Self {
        Self::Scheduler(e.into())
    }
}
/// The same policy instance is retained across pause, resume and config reload.
pub struct ScheduleControl {
    pub scheduler: RegisteredScheduler,
    delay_at: i64,
    delay_until: i64,
}
impl ScheduleControl {
    pub fn is_paused(&self, now: i64) -> bool {
        now < self.delay_until
    }
    pub fn delay(&self, now: i64) -> (i64, i64) {
        if self.is_paused(now) {
            (self.delay_at, self.delay_until)
        } else {
            (0, 0)
        }
    }
}
/// Mutations require exclusive access; hosts serialize commands with their own lock.
/// Handler entries hold policies, never transport-specific HTTP objects.
pub struct SchedulerController {
    cluster: Option<Arc<BasicCluster>>,
    context: FactoryContext,
    schedulers: BTreeMap<String, ScheduleControl>,
    handlers: BTreeMap<String, RegisteredScheduler>,
    configuration: Value,
    pub cleanup_errors: Vec<String>,
}
impl SchedulerController {
    pub fn new(
        cluster: Option<Arc<BasicCluster>>,
        context: FactoryContext,
        configuration: Value,
    ) -> Self {
        Self {
            cluster,
            context,
            schedulers: BTreeMap::new(),
            handlers: BTreeMap::new(),
            configuration,
            cleanup_errors: vec![],
        }
    }
    pub fn context(&self) -> &FactoryContext {
        &self.context
    }
    pub fn configuration(&self) -> &Value {
        &self.configuration
    }
    fn ready(&self) -> Result<&BasicCluster, ControlError> {
        self.cluster.as_deref().ok_or(ControlError::NotBootstrapped)
    }
    pub fn get(&self, name: &str) -> Option<&ScheduleControl> {
        self.schedulers.get(name)
    }
    pub fn names(&self) -> Vec<String> {
        self.schedulers.keys().cloned().collect()
    }
    pub fn handler_names(&self) -> Vec<String> {
        if self.handlers.is_empty() {
            self.names()
        } else {
            self.handlers.keys().cloned().collect()
        }
    }
    pub fn handler(&self, name: &str) -> Option<&RegisteredScheduler> {
        if self.handlers.is_empty() {
            self.schedulers.get(name).map(|s| &s.scheduler)
        } else {
            self.handlers.get(name)
        }
    }
    pub fn exists(&self, name: &str) -> Result<bool, ControlError> {
        self.ready()?;
        if self.schedulers.contains_key(name) || self.handlers.contains_key(name) {
            Ok(true)
        } else {
            Err(ControlError::NotFound(name.into()))
        }
    }
    fn entry(&self, name: &str) -> Result<&ScheduleControl, ControlError> {
        self.ready()?;
        self.schedulers
            .get(name)
            .ok_or_else(|| ControlError::NotFound(name.into()))
    }
    pub fn is_paused(&self, name: &str, now: i64) -> Result<bool, ControlError> {
        Ok(self.entry(name)?.is_paused(now))
    }
    pub fn delay(&self, name: &str, now: i64) -> Result<(i64, i64), ControlError> {
        Ok(self.entry(name)?.delay(now))
    }
    pub fn is_disabled(&self, name: &str) -> Result<bool, ControlError> {
        let typ = self.entry(name)?.scheduler.scheduler_type();
        Ok(self.configuration["schedule"]["schedulers-v2"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|v| {
                v["type"] == scheduler_type_compatible(&typ).unwrap_or(typ.as_str())
                    && v["disable"] == true
            }))
    }
    pub fn pause_or_resume(&mut self, name: &str, ttl: i64, now: i64) -> Result<(), ControlError> {
        self.ready()?;
        if name != "all" && !self.schedulers.contains_key(name) {
            return Err(ControlError::NotFound(name.into()));
        }
        for (key, s) in &mut self.schedulers {
            if name == "all" || key == name {
                (s.delay_at, s.delay_until) = if ttl > 0 {
                    (now, now.wrapping_add(ttl))
                } else {
                    (0, 0)
                };
            }
        }
        Ok(())
    }
    fn configs(&mut self) -> &mut Vec<Value> {
        if !self.configuration.is_object() {
            self.configuration = json!({});
        }
        if !self.configuration["schedule"].is_object() {
            self.configuration["schedule"] = json!({});
        }
        if !self.configuration["schedule"]["schedulers-v2"].is_array() {
            self.configuration["schedule"]["schedulers-v2"] = json!([]);
        }
        self.configuration["schedule"]["schedulers-v2"]
            .as_array_mut()
            .unwrap()
    }
    pub async fn add(
        &mut self,
        mut scheduler: RegisteredScheduler,
        args: &[&str],
        handler_only: bool,
    ) -> Result<(), ControlError> {
        self.ready()?;
        let cluster = self.cluster.as_ref().unwrap().clone();
        let name = scheduler.name();
        if if handler_only {
            self.handlers.contains_key(&name)
        } else {
            self.schedulers.contains_key(&name)
        } {
            return Err(ControlError::Exists(name));
        }
        let existing = if handler_only {
            self.context
                .storage
                .load_scheduler_config(&self.context.paths, &name)
                .await?
        } else {
            vec![]
        };
        if !handler_only {
            let errors = prepare(&mut scheduler, &cluster);
            if !errors.is_empty() {
                self.cleanup_errors
                    .extend(clean(&mut scheduler, &cluster, &self.context).await);
                return Err(ControlError::Prepare(errors));
            }
        }
        scheduler.set_disabled(false);
        if let Err(e) = scheduler.save_config(&self.context).await {
            if !handler_only {
                self.cleanup_errors
                    .extend(clean(&mut scheduler, &cluster, &self.context).await);
            }
            return Err(e.into());
        }
        if handler_only {
            let errors = prepare(&mut scheduler, &cluster);
            if !errors.is_empty() {
                self.cleanup_errors
                    .extend(clean(&mut scheduler, &cluster, &self.context).await);
                if existing.is_empty() {
                    if let Err(e) = self
                        .context
                        .storage
                        .remove_scheduler_config(&self.context.paths, &name)
                        .await
                    {
                        self.cleanup_errors.push(e.to_string());
                    }
                }
                return Err(ControlError::Prepare(errors));
            }
        }
        let typ = scheduler.scheduler_type();
        let legacy = scheduler_type_compatible(&typ).unwrap_or(typ.as_str());
        let configs = self.configs();
        if let Some(v) = configs
            .iter_mut()
            .find(|v| v["type"] == legacy && v["args"] == json!(args))
        {
            v["disable"] = json!(false);
        } else {
            configs.push(json!({"type":legacy,"args":args,"disable":false}));
        }
        if handler_only {
            self.handlers.insert(name, scheduler);
        } else {
            self.schedulers.insert(
                name,
                ScheduleControl {
                    scheduler,
                    delay_at: 0,
                    delay_until: 0,
                },
            );
        }
        Ok(())
    }
    pub async fn remove(&mut self, name: &str, handler_only: bool) -> Result<(), ControlError> {
        self.ready()?;
        let cluster = self.cluster.as_ref().unwrap().clone();
        let scheduler = if handler_only {
            self.handlers.get(name)
        } else {
            self.schedulers.get(name).map(|s| &s.scheduler)
        }
        .ok_or_else(|| ControlError::NotFound(name.into()))?;
        let typ = scheduler.scheduler_type();
        let default = scheduler.is_default();
        let legacy = scheduler_type_compatible(&typ).unwrap_or(typ.as_str());
        let configs = self.configs();
        if let Some(i) = configs.iter().position(|v| v["type"] == legacy) {
            if default {
                configs[i]["disable"] = json!(true);
            } else {
                configs.remove(i);
            }
        }
        // Go retains the modified cluster options if persistence fails.
        self.context
            .storage
            .save_config(&self.context.paths, &self.configuration)
            .await?;
        let scheduler = if handler_only {
            self.handlers.get_mut(name).unwrap()
        } else {
            &mut self.schedulers.get_mut(name).unwrap().scheduler
        };
        scheduler.set_disabled(true);
        if scheduler.is_disabled() {
            if let Err(error) = scheduler.save_config(&self.context).await {
                self.cleanup_errors.push(error.to_string());
            }
        }
        self.context
            .storage
            .remove_scheduler_config(&self.context.paths, name)
            .await?;
        self.cleanup_errors
            .extend(clean(scheduler, &cluster, &self.context).await);
        if handler_only {
            self.handlers.remove(name);
        } else {
            self.schedulers.remove(name);
        }
        Ok(())
    }
    pub async fn reload(&mut self, name: &str) -> Result<Vec<StoreControlError>, ControlError> {
        self.ready()?;
        let cluster = self.cluster.as_ref().unwrap().clone();
        let s = &mut self
            .schedulers
            .get_mut(name)
            .ok_or_else(|| ControlError::NotFound(name.into()))?
            .scheduler;
        let storage = &self.context.storage;
        let paths = &self.context.paths;
        let errors = match &mut s.policy {
            BuiltinScheduler::EvictLeader(p) => {
                p.reload_evict_config(&cluster, storage, paths).await?
            }
            BuiltinScheduler::GrantLeader(p) => {
                p.reload_grant_config(&cluster, storage, paths).await?
            }
            BuiltinScheduler::BalanceLeader(p) => {
                p.reload_leader_config(storage, paths).await?;
                vec![]
            }
            BuiltinScheduler::BalanceWitness(p) => {
                p.reload_witness_config(storage, paths).await?;
                vec![]
            }
            BuiltinScheduler::GrantHot(p) => {
                p.reload(storage, paths).await?;
                vec![]
            }
            BuiltinScheduler::ScatterRange(p) => {
                p.reload(storage, paths).await?;
                vec![]
            }
            BuiltinScheduler::ShuffleHot(p) => {
                p.reload(storage, paths).await?;
                vec![]
            }
            BuiltinScheduler::ShuffleRegion(p) => {
                p.reload(storage, paths).await?;
                vec![]
            }
            BuiltinScheduler::BalanceRange { jobs, .. } => {
                jobs.reload().await?;
                vec![]
            }
            BuiltinScheduler::EvictStoppingStore(p) => {
                let bytes = storage.load_scheduler_config(paths, name).await?;
                p.reload(decode_config(&bytes)?);
                vec![]
            }
            BuiltinScheduler::SlowTrend(p) => {
                let bytes = storage.load_scheduler_config(paths, name).await?;
                p.reload(decode_config(&bytes)?, &cluster)
            }
            BuiltinScheduler::EvictSlowStore { disk, network } => {
                let bytes = storage.load_scheduler_config(paths, name).await?;
                let next: EvictNetworkConfig = decode_config(&bytes)?;
                let disk_next = next.disk.clone();
                let errors = network.reload(next, &cluster);
                *disk = EvictDiskScheduler::new(disk_next);
                errors
            }
            BuiltinScheduler::Hot { scheduler, config } => {
                let bytes = storage.load_scheduler_config(paths, name).await?;
                let next: HotRegionParams = decode_config(&bytes)?;
                let candidate = HotScheduler::new(
                    next.clone(),
                    self.context.query_supported,
                    self.context.cpu_supported,
                    self.context.hot_inputs.clone(),
                )?;
                *scheduler = Box::new(candidate);
                *config = next;
                vec![]
            }
            // These Go policies inherit BaseScheduler.ReloadConfig (a no-op).
            BuiltinScheduler::BalanceRegion(_)
            | BuiltinScheduler::Label(_)
            | BuiltinScheduler::RandomMerge(_)
            | BuiltinScheduler::ShuffleLeader(_)
            | BuiltinScheduler::TransferWitness(_) => vec![],
        };
        Ok(errors)
    }
}
fn prepare(s: &mut RegisteredScheduler, c: &BasicCluster) -> Vec<StoreControlError> {
    match &mut s.policy {
        BuiltinScheduler::EvictLeader(p) => p.prepare(c).err().into_iter().collect(),
        BuiltinScheduler::GrantLeader(p) => p.prepare(c).err().into_iter().collect(),
        BuiltinScheduler::EvictStoppingStore(p) => p.prepare(c).err().into_iter().collect(),
        BuiltinScheduler::SlowTrend(p) => p.prepare(c).err().into_iter().collect(),
        BuiltinScheduler::EvictSlowStore { disk, network } => {
            let mut e: Vec<_> = disk.prepare(c).err().into_iter().collect();
            e.extend(network.prepare(c));
            e
        }
        _ => vec![],
    }
}
async fn clean(s: &mut RegisteredScheduler, c: &BasicCluster, ctx: &FactoryContext) -> Vec<String> {
    match &mut s.policy {
        BuiltinScheduler::EvictLeader(p) => {
            p.clean(c);
            vec![]
        }
        BuiltinScheduler::GrantLeader(p) => {
            p.clean(c);
            vec![]
        }
        BuiltinScheduler::EvictStoppingStore(p) => p
            .clean(c, &ctx.storage, &ctx.paths)
            .await
            .err()
            .map(|e| e.to_string())
            .into_iter()
            .collect(),
        BuiltinScheduler::SlowTrend(p) => p
            .clean(
                c,
                &ctx.storage,
                &ctx.paths,
                (ctx.now.0 / 1_000_000_000) as i64,
            )
            .await
            .err()
            .map(|e| e.to_string())
            .into_iter()
            .collect(),
        BuiltinScheduler::EvictSlowStore { disk, network } => {
            let mut e: Vec<_> = disk
                .clean(c, &ctx.storage, &ctx.paths)
                .await
                .err()
                .map(|e| e.to_string())
                .into_iter()
                .collect();
            e.extend(
                network
                    .clean(c, &ctx.storage, &ctx.paths)
                    .await
                    .into_iter()
                    .map(|e| e.to_string()),
            );
            e
        }
        _ => vec![],
    }
}
