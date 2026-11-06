// Copyright 2026 AsterSQL.
use crate::{Bucket, BucketRuntime, BucketState, Consumption, Group, GroupState, WriteRole};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
/// Storage codec adapter, supplied by the host. Domain values never contain protobuf.
#[async_trait::async_trait]
pub trait Persistence: Send + Sync {
    async fn load(&self, role: WriteRole) -> Result<Vec<(u32, Group, Option<GroupState>)>, String>;
    async fn save_setting(&self, keyspace: u32, group: &Group) -> Result<(), String>;
    async fn save_state(&self, keyspace: u32, name: &str, state: &GroupState)
    -> Result<(), String>;
    async fn delete(&self, keyspace: u32, name: &str) -> Result<(), String>;
}
/// Resource groups for one active host term, with a caller-owned store and executor.
pub struct Manager {
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
        let mut this = Self {
            buckets: BTreeMap::new(),
            consumption_keyspaces: BTreeSet::new(),
            groups: BTreeMap::new(),
            storage,
            role,
        };
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
        Ok(this)
    }
    async fn ensure(&mut self, id: u32) -> Result<(), String> {
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
        self.buckets
            .insert((id, group.name.clone()), BucketRuntime::default());
        self.groups
            .entry(id)
            .or_default()
            .insert(group.name.clone(), group);
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
        self.buckets
            .entry((id, name.into()))
            .or_default()
            .request(bucket, now, required, period_ms, client)
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
