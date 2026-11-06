// Copyright 2026 AsterSQL.
use crate::{Bucket, BucketRuntime, BucketState, Consumption, Group, GroupState, WriteRole};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
/// Storage codec adapter, supplied by the host. Domain values never contain protobuf.
#[async_trait::async_trait]
pub trait Persistence: Send + Sync {
    async fn limits(&self) -> Result<Vec<(u32, f64)>, String>;
    async fn save_limit(&self, id: u32, rate: f64) -> Result<(), String>;
    async fn load(&self, role: WriteRole) -> Result<Vec<(u32, Group, Option<GroupState>)>, String>;
    async fn save_setting(&self, keyspace: u32, group: &Group) -> Result<(), String>;
    async fn save_state(&self, keyspace: u32, name: &str, state: &GroupState)
    -> Result<(), String>;
    async fn delete(&self, keyspace: u32, name: &str) -> Result<(), String>;
}
/// Resource groups for one active host term, with a caller-owned store and executor.
pub struct Manager {
    pub(crate) controller: Option<crate::ControllerConfig>,
    pub(crate) limits: BTreeMap<u32, crate::ServiceLimit>,
    pub(crate) consumption_keyspaces: BTreeSet<u32>,
    pub(crate) groups: BTreeMap<u32, BTreeMap<String, Group>>,
    pub(crate) buckets: BTreeMap<(u32, String), BucketRuntime>,
    storage: Arc<dyn Persistence>,
    role: WriteRole,
}
fn missing_keyspace(id: u32) -> String {
    format!("[PD:resourcemanager:ErrKeyspaceNotExists]the keyspace does not exist with id {id}")
}
fn missing_group(name: &str) -> String {
    format!("[PD:resourcemanager:ErrGroupNotExists]the {name} resource group does not exist")
}
impl Manager {
    pub(crate) fn empty(storage: Arc<dyn Persistence>, role: WriteRole) -> Self {
        Self {
            controller: None,
            limits: BTreeMap::new(),
            buckets: BTreeMap::new(),
            consumption_keyspaces: BTreeSet::new(),
            groups: BTreeMap::new(),
            storage,
            role,
        }
    }
    /// Open the canonical metadata/state adapter over a host-owned, term-fenced store.
    pub async fn open(
        store: Arc<dyn astersql_pd_storage_domain::MetaStore>,
        role: WriteRole,
    ) -> Result<Self, String> {
        Self::load(Arc::new(crate::grpc_storage::Storage::new(store)), role).await
    }

    /// Use the host's controller values before overlaying existing persisted JSON.
    pub async fn open_with_controller(
        store: Arc<dyn astersql_pd_storage_domain::MetaStore>,
        role: WriteRole,
        controller: crate::ControllerConfig,
    ) -> Result<Self, String> {
        Self::load(
            Arc::new(crate::grpc_storage::Storage(store, controller)),
            role,
        )
        .await
    }
    pub async fn load(storage: Arc<dyn Persistence>, role: WriteRole) -> Result<Self, String> {
        let mut this = Self::empty(storage, role);
        for (id, mut group, state) in this.storage.load(role).await? {
            if group.validate().is_err() {
                continue;
            }
            group.initialize();
            if let Some(state) = state {
                if let Some(bucket) = &state.ru {
                    if let Some(ru) = &mut group.ru {
                        ru.tokens = bucket.tokens;
                    }
                }
                if let Some(stats) = &state.consumption {
                    group
                        .consumption
                        .get_or_insert_with(Default::default)
                        .accumulate(stats);
                }
                if let Some(bucket) = &state.ru {
                    this.buckets
                        .insert((id, group.name.clone()), BucketRuntime::from_state(bucket));
                }
            }
            this.groups
                .entry(id)
                .or_default()
                .insert(group.name.clone(), group);
        }
        let mut ids: Vec<_> = this.groups.keys().copied().collect();
        if !ids.contains(&u32::MAX) {
            ids.push(u32::MAX);
        }
        for id in ids {
            this.ensure(id).await?;
        }
        let limits = this.storage.limits().await?;
        let now = now_nanos();
        for (id, rate) in limits {
            this.set_limit_from_storage(id, rate, now);
        }
        Ok(this)
    }
    pub(crate) async fn ensure(&mut self, id: u32) -> Result<(), String> {
        if !self
            .groups
            .get(&id)
            .is_some_and(|g| g.contains_key("default"))
        {
            self.store_new(id, Group::reserved_default()).await?;
        }
        Ok(())
    }
    pub async fn list(&mut self, id: u32, stats: bool) -> Result<Vec<Group>, String> {
        self.ensure(id).await?;
        Ok(self.groups[&id]
            .values()
            .map(|g| Self::cloned(g, stats))
            .collect())
    }
    fn cloned(g: &Group, stats: bool) -> Group {
        let mut g = g.clone();
        if !stats {
            g.consumption = None;
        }
        g
    }
    pub async fn get(&mut self, id: u32, name: &str, stats: bool) -> Result<Group, String> {
        if name == "default" {
            self.ensure(id).await?;
        }
        let groups = self.groups.get(&id).ok_or_else(|| missing_keyspace(id))?;
        groups
            .get(name)
            .map(|g| Self::cloned(g, stats))
            .ok_or_else(|| missing_group(name))
    }
    pub async fn add(&mut self, id: u32, group: Group) -> Result<(), String> {
        if !self.role.metadata() {
            return Err("metadata write is disabled".into());
        }
        self.ensure(id).await?;
        group.validate()?;
        self.store_new(id, group).await
    }
    async fn store_new(&mut self, id: u32, mut group: Group) -> Result<(), String> {
        group.initialize();
        let state = GroupState {
            ru: Some(BucketState {
                tokens: group.ru.as_ref().map_or(0.0, |b| b.tokens),
                ..BucketState::default()
            }),
            consumption: group.consumption.clone(),
        };
        if self.role.metadata() {
            self.storage.save_setting(id, &group).await?;
        }
        if self.role.state() {
            self.storage.save_state(id, &group.name, &state).await?;
        }
        let previous = self
            .buckets
            .remove(&(id, group.name.clone()))
            .unwrap_or_default();
        self.buckets.insert(
            (id, group.name.clone()),
            BucketRuntime {
                has_tracker: previous.has_tracker,
                demands: previous.demands,
                ..Default::default()
            },
        );
        let name = group.name.clone();
        self.groups
            .entry(id)
            .or_default()
            .insert(group.name.clone(), group);
        self.sync_burstability(id, &name);
        Ok(())
    }
    pub async fn modify(&mut self, id: u32, group: Group) -> Result<(), String> {
        if !self.role.metadata() {
            return Err("metadata write is disabled".into());
        }
        if group.name == "default" {
            self.ensure(id).await?;
        }
        let groups = self
            .groups
            .get_mut(&id)
            .ok_or_else(|| missing_keyspace(id))?;
        if group.name.is_empty() {
            return group.validate();
        }
        let current = groups
            .get_mut(&group.name)
            .ok_or_else(|| missing_group(&group.name))?;
        let settings_changed = group.ru.as_ref().is_some_and(|b| b.settings.is_some());
        current.patch(group)?;
        if settings_changed {
            self.buckets
                .entry((id, current.name.clone()))
                .or_default()
                .setting_changed = true;
        }
        self.storage.save_setting(id, current).await
    }
    pub async fn delete(&mut self, id: u32, name: &str) -> Result<(), String> {
        if !self.role.metadata() {
            return Err("metadata write is disabled".into());
        }
        let groups = self
            .groups
            .get_mut(&id)
            .ok_or_else(|| missing_keyspace(id))?;
        if name == "default" {
            return Err(
                "[PD:resourcemanager:ErrDeleteReservedGroup]cannot delete reserved group".into(),
            );
        }
        if !groups.contains_key(name) {
            return Err(missing_group(name));
        }
        self.storage.delete(id, name).await?;
        groups.remove(name);
        self.buckets.remove(&(id, name.into()));
        Ok(())
    }
}

impl Manager {
    pub async fn request_ru(
        &mut self,
        id: u32,
        name: &str,
        now: i64,
        required: f64,
        period_ms: u64,
        client: u64,
    ) -> Option<(Bucket, i64)> {
        self.get(id, name, false).await.ok()?;
        let group = self.groups.get_mut(&id)?.get_mut(name)?;
        let bucket = group.ru.as_mut()?;
        let runtime = self.buckets.entry((id, name.into())).or_default();
        let (mut granted, mut trickle) =
            runtime.request(bucket, now, required, period_ms, client)?;
        if let Some(limit) = self.limits.get_mut(&id) {
            let original = granted.tokens;
            let (tokens, min_trickle) = limit.apply(now, original);
            if tokens < original {
                granted.tokens = tokens;
                let burst = runtime
                    .override_burst_limit
                    .filter(|v| *v >= 0)
                    .unwrap_or_else(|| bucket.settings.as_ref().map_or(0, |s| s.burst_limit));
                if burst > 0 {
                    runtime.reserved_service += original - tokens;
                }
            }
            trickle = trickle.max(min_trickle);
        }
        Some((granted, trickle))
    }
    pub fn record_consumption(&mut self, id: u32, name: &str, delta: &Consumption) {
        if let Some(group) = self
            .groups
            .get_mut(&id)
            .and_then(|groups| groups.get_mut(name))
        {
            group
                .consumption
                .get_or_insert_with(Default::default)
                .accumulate(delta);
        }
    }
    /// Persist live balances on the host's term; metadata-only roles perform no writes.
    pub async fn persist_states(&mut self) -> Result<(), String> {
        if !self.role.state() {
            return Ok(());
        }
        let mut first_error = None;
        for (id, groups) in &self.groups {
            for (name, group) in groups {
                let state = GroupState {
                    ru: Some(match group.ru.as_ref() {
                        Some(bucket) => self
                            .buckets
                            .get(&(*id, name.clone()))
                            .cloned()
                            .unwrap_or_default()
                            .state(bucket),
                        None => BucketState::default(),
                    }),
                    consumption: group.consumption.clone(),
                };
                if let Err(error) = self.storage.save_state(*id, name, &state).await {
                    first_error.get_or_insert(error);
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }
}

fn now_nanos() -> i64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_nanos().min(i64::MAX as u128) as i64,
        Err(e) => -(e.duration().as_nanos().min(i64::MAX as u128) as i64),
    }
}
impl Manager {
    pub(crate) fn sync_burstability(&mut self, id: u32, name: &str) {
        let Some(limit) = self.limits.get(&id).filter(|v| v.rate > 0.0) else {
            return;
        };
        let Some(group) = self.groups.get(&id).and_then(|g| g.get(name)) else {
            return;
        };
        if group
            .ru
            .as_ref()
            .and_then(|b| b.settings.as_ref())
            .is_some_and(|s| s.burst_limit < 0)
        {
            let bucket = self.buckets.entry((id, name.into())).or_default();
            if bucket.override_burst_limit.is_none_or(|v| v < 0) {
                bucket.override_burst_limit = Some(limit.rate as i64);
            }
        }
    }
    pub fn set_limit_from_storage(&mut self, id: u32, rate: f64, now: i64) {
        self.limits
            .entry(id)
            .or_insert_with(|| crate::ServiceLimit::new(0.0, now))
            .set(rate, now);
        let groups = self.groups.entry(id).or_default();
        for (name, group) in groups {
            let bucket = self.buckets.entry((id, name.clone())).or_default();
            if rate <= 0.0 {
                bucket.override_fill_rate = None;
                bucket.override_burst_limit = None;
            } else if group
                .ru
                .as_ref()
                .and_then(|b| b.settings.as_ref())
                .is_some_and(|s| s.burst_limit < 0)
            {
                bucket.override_burst_limit = Some(rate as i64);
            }
        }
    }
    /// Metadata roles publish settings; runtime quota changes precede persistence as in Go.
    pub async fn set_service_limit(&mut self, id: u32, rate: f64, now: i64) -> Result<(), String> {
        let unchanged = self
            .limits
            .get(&id)
            .is_some_and(|limit| limit.rate == crate::rm_slot::go_max(0.0, rate));
        self.set_limit_from_storage(id, rate, now);
        if self.role.metadata() && !unchanged {
            self.storage
                .save_limit(id, crate::rm_slot::go_max(0.0, rate))
                .await?;
        }
        Ok(())
    }
}
