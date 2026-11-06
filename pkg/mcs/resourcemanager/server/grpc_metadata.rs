// Copyright 2026 AsterSQL.
//! Metadata codec and snapshot/watch adapter. Domain mutation stays in the manager.
use crate::{ControllerConfig, Group, GroupState, Manager, WriteRole, grpc_codec};
use astersql_pd_keypath::*;
use astersql_pd_proto::resource_manager::ResourceGroup;
use astersql_pd_storage_domain::{Consistency, Event, MetaStore, MetaWatch, Read};
use prost::Message;
use std::sync::Arc;
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Target<'a> {
    Controller,
    Setting(u32, &'a str),
    State(u32, &'a str),
    Limit(u32),
}
pub(super) fn target(key: &str) -> Option<Target<'_>> {
    if key == controller_config_path() {
        return Some(Target::Controller);
    }
    for (prefix, state, scoped) in [
        (resource_group_setting_prefix(), false, false),
        (resource_group_state_prefix(), true, false),
        (keyspace_resource_group_setting_prefix(), false, true),
        (keyspace_resource_group_state_prefix(), true, true),
    ] {
        if let Some(suffix) = key.strip_prefix(prefix) {
            let (id, name) = if scoped {
                parse_keyspace_resource_group_path(suffix).ok()?
            } else {
                (u32::MAX, suffix)
            };
            if name.is_empty() {
                return None;
            }
            return Some(if state {
                Target::State(id, name)
            } else {
                Target::Setting(id, name)
            });
        }
    }
    key.strip_prefix(keyspace_service_limit_prefix())
        .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|id| id.parse().ok())
        .map(Target::Limit)
}
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as i64
}
impl Manager {
    /// One linearizable snapshot followed by an inclusive revision+1 watch.
    pub(crate) async fn open_watched(
        store: Arc<dyn MetaStore>,
        controller: ControllerConfig,
    ) -> Result<(Self, Box<dyn MetaWatch>), String> {
        let snapshot = store
            .read(Read::prefix(
                resource_group_prefix().as_bytes(),
                Consistency::Linearizable,
            ))
            .await
            .map_err(|e| e.to_string())?;
        let mut manager = Self::empty(
            Arc::new(crate::grpc_storage::Storage(
                store.clone(),
                controller.clone(),
            )),
            WriteRole::LegacyAll,
        );
        manager.controller = Some(controller);
        for entry in snapshot.entries {
            manager.metadata_put(&entry.key, &entry.value)?;
        }
        let mut ids: Vec<_> = manager.groups.keys().copied().collect();
        if !ids.contains(&u32::MAX) {
            ids.push(u32::MAX);
        }
        for id in ids {
            manager.ensure(id).await?;
        }
        let watch = store
            .watch(
                resource_group_prefix().as_bytes().to_vec(),
                snapshot.revision + 1,
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok((manager, watch))
    }
    pub(crate) fn metadata_event(&mut self, event: Event) -> Result<(), String> {
        match event {
            Event::Put(entry) => self.metadata_put(&entry.key, &entry.value),
            Event::Delete { key, .. } => {
                let Ok(key) = std::str::from_utf8(&key) else {
                    return Ok(());
                };
                match target(key) {
                    Some(Target::Setting(id, "default")) => {
                        self.cache_default(id, true);
                    }
                    Some(Target::Setting(id, name)) => {
                        if let Some(groups) = self.groups.get_mut(&id) {
                            groups.remove(name);
                        }
                        self.buckets.remove(&(id, name.into()));
                    }
                    Some(Target::Limit(id)) if self.groups.contains_key(&id) => {
                        self.set_limit_from_storage(id, 0.0, now())
                    }
                    _ => {}
                }
                Ok(())
            }
        }
    }
    fn cache_default(&mut self, id: u32, replace: bool) {
        if replace
            || !self
                .groups
                .get(&id)
                .is_some_and(|g| g.contains_key("default"))
        {
            self.groups
                .entry(id)
                .or_default()
                .insert("default".into(), Group::reserved_default());
            let previous = self
                .buckets
                .remove(&(id, "default".into()))
                .unwrap_or_default();
            // Trackers belong to the keyspace manager, surviving replacement of a group bucket.
            self.buckets.insert(
                (id, "default".into()),
                crate::BucketRuntime {
                    demands: previous.demands,
                    has_tracker: previous.has_tracker,
                    ..Default::default()
                },
            );
            self.sync_burstability(id, "default");
        }
    }
    fn metadata_put(&mut self, key: &[u8], value: &[u8]) -> Result<(), String> {
        let Ok(key) = std::str::from_utf8(key) else {
            return Ok(());
        };
        match target(key) {
            Some(Target::Controller) => {
                // Watcher decoding targets a zero value struct, including JSON null.
                self.controller = Some(
                    serde_json::from_slice::<Option<ControllerConfig>>(value)
                        .map_err(|e| e.to_string())?
                        .unwrap_or_default(),
                );
            }
            Some(Target::Setting(id, name)) => {
                self.cache_default(id, false);
                let mut group =
                    grpc_codec::decode(ResourceGroup::decode(value).map_err(|e| e.to_string())?);
                if group.name != name {
                    return Err("resource group name does not match metadata key".into());
                }
                group.validate()?;
                let changed = group.ru.as_ref().is_some_and(|b| b.settings.is_some());
                if let Some(current) = self.groups.get_mut(&id).and_then(|g| g.get_mut(name)) {
                    if let Some(bucket) = &mut group.ru {
                        bucket.tokens = 0.0;
                    }
                    current.patch(group)?;
                    if changed {
                        self.buckets
                            .entry((id, name.into()))
                            .or_default()
                            .setting_changed = true;
                    }
                } else {
                    group.initialize();
                    self.groups
                        .entry(id)
                        .or_default()
                        .insert(name.into(), group);
                }
                self.sync_burstability(id, name);
            }
            Some(Target::State(id, name)) => {
                if let Some(group) = self.groups.get_mut(&id).and_then(|g| g.get_mut(name)) {
                    let state = serde_json::from_slice::<Option<GroupState>>(value)
                        .map_err(|e| e.to_string())?
                        .unwrap_or_default();
                    if let Some(state) = state.ru {
                        if let Some(bucket) = &mut group.ru {
                            bucket.tokens = state.tokens;
                        }
                        let restored = crate::BucketRuntime::from_state(&state);
                        let runtime = self.buckets.entry((id, name.into())).or_default();
                        runtime.initialized = restored.initialized;
                        runtime.last_update = restored.last_update;
                    }
                    if let Some(stats) = state.consumption {
                        group
                            .consumption
                            .get_or_insert_with(Default::default)
                            .accumulate(&stats);
                    }
                }
            }
            Some(Target::Limit(id)) => {
                let rate = serde_json::from_slice::<Option<f64>>(value)
                    .map_err(|e| e.to_string())?
                    .unwrap_or_default();
                self.cache_default(id, false);
                self.set_limit_from_storage(id, rate, now());
            }
            None => {}
        }
        Ok(())
    }
}
