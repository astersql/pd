// Copyright 2026 AsterSQL.
use crate::{
    GroupCatalogError, GroupNodesError, KeyspaceGroupManager,
    group_catalog::{empty_index, put_index},
    group_nodes::unique_tso_nodes,
};
use astersql_pd_election::{ElectionError, Leadership};
use astersql_pd_etcdutil::ReconciledWatch;
use astersql_pd_keypath::keyspace_group_id_from_path;
use astersql_pd_mcs_constant::DEFAULT_KEYSPACE_GROUP_REPLICA_COUNT;
use astersql_pd_storage_domain::{
    EndpointError, Event, KeyspaceGroup, KeyspaceGroupMember, KeyspaceGroupStoreError, MergeState,
    SplitState, StoreError, WatchBatch,
};
use astersql_pd_typeutil::trim_scheme;
use serde::de::{DeserializeOwned, IgnoredAny};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};
use tokio_util::sync::CancellationToken;
/// Minimal membership fields needed for allocation selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupReconcileEntry {
    pub id: u32,
    pub members: Option<Vec<KeyspaceGroupMember>>,
    pub transitioning: bool,
}
#[derive(Debug, thiserror::Error)]
pub enum GroupReconcileError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Leadership(#[from] ElectionError),
}
/// Mutable observations belong to one immutable leadership term.
pub struct GroupReconcileTerm {
    pub(crate) leadership: Arc<Leadership>,
    pub(crate) cancel: CancellationToken,
    entries: Mutex<BTreeMap<u32, GroupReconcileEntry>>,
    registry: Mutex<BTreeMap<Vec<u8>, String>>,
}
impl GroupReconcileTerm {
    pub fn entries(&self) -> Vec<GroupReconcileEntry> {
        self.entries.lock().unwrap().values().cloned().collect()
    }
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }
    pub fn groups_needing_allocation(&self, nodes: &[String]) -> Vec<u32> {
        let nodes = unique_tso_nodes(nodes);
        self.entries
            .lock()
            .unwrap()
            .values()
            .filter(|g| {
                !g.transitioning
                    && needs_allocation(g.members.as_deref().unwrap_or_default(), &nodes)
            })
            .map(|g| g.id)
            .collect()
    }
}
impl Drop for GroupReconcileTerm {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
pub(crate) fn needs_allocation(
    members: &[KeyspaceGroupMember],
    nodes: &BTreeMap<String, String>,
) -> bool {
    let mut existing = BTreeSet::new();
    for member in members {
        let address = trim_scheme(&member.address);
        if nodes.contains_key(address) {
            existing.insert(address);
            if existing.len() == DEFAULT_KEYSPACE_GROUP_REPLICA_COUNT {
                return false;
            }
        }
    }
    existing.is_empty() || existing.len() != members.len() || existing.len() != nodes.len()
}
fn whitespace(data: &[u8], offset: &mut usize) {
    while data.get(*offset).is_some_and(u8::is_ascii_whitespace) {
        *offset += 1;
    }
}
fn punctuation(data: &[u8], offset: &mut usize, expected: u8) -> Result<(), StoreError> {
    whitespace(data, offset);
    if data.get(*offset) != Some(&expected) {
        return Err(StoreError::Backend);
    }
    *offset += 1;
    Ok(())
}
fn value<T: DeserializeOwned>(data: &[u8], offset: &mut usize) -> Result<T, StoreError> {
    let mut stream = serde_json::Deserializer::from_slice(&data[*offset..]).into_iter::<T>();
    let value = stream
        .next()
        .ok_or(StoreError::Backend)?
        .map_err(|_| StoreError::Backend)?;
    *offset += stream.byte_offset();
    Ok(value)
}
/// Match Go's prefix decoder: stop once id and members are present, before keyspaces.
pub fn decode_group_reconcile_entry(data: &[u8]) -> Result<GroupReconcileEntry, StoreError> {
    let mut offset = 0;
    punctuation(data, &mut offset, b'{')?;
    let mut entry = GroupReconcileEntry {
        id: 0,
        members: None,
        transitioning: false,
    };
    let (mut got_id, mut got_members) = (false, false);
    loop {
        let field: String = value(data, &mut offset)?;
        punctuation(data, &mut offset, b':')?;
        match field.as_str() {
            "id" => {
                if let Some(id) = value::<Option<u32>>(data, &mut offset)? {
                    entry.id = id;
                }
                got_id = true;
            }
            "members" => {
                entry.members = value(data, &mut offset)?;
                got_members = true;
            }
            "split-state" => {
                entry.transitioning |= value::<Option<SplitState>>(data, &mut offset)?.is_some();
            }
            "merge-state" => {
                entry.transitioning |= value::<Option<MergeState>>(data, &mut offset)?.is_some();
            }
            _ => {
                value::<IgnoredAny>(data, &mut offset)?;
            }
        }
        if got_id && got_members {
            return Ok(entry);
        }
        punctuation(data, &mut offset, b',')?;
    }
}
fn path_id(key: &[u8]) -> Result<u32, StoreError> {
    keyspace_group_id_from_path(std::str::from_utf8(key).map_err(|_| StoreError::Backend)?)
        .and_then(|id| id.parse().ok())
        .ok_or(StoreError::Backend)
}
pub(crate) fn registry_address(data: &[u8]) -> Result<String, StoreError> {
    let value: serde_json::Value = serde_json::from_slice(data).map_err(|_| StoreError::Backend)?;
    if value.is_null() {
        return Ok(String::new());
    }
    let object = value.as_object().ok_or(StoreError::Backend)?;
    match object.get("service-addr") {
        None | Some(serde_json::Value::Null) => Ok(String::new()),
        Some(serde_json::Value::String(addr)) => Ok(addr.clone()),
        _ => Err(StoreError::Backend),
    }
}
/// Independent pull drivers, owned and scheduled by the host on its runtime.
pub struct GroupReconcile {
    pub term: Arc<GroupReconcileTerm>,
    pub groups: GroupMetadataObserver,
    pub nodes: GroupNodeObserver,
}
pub struct GroupMetadataObserver {
    manager: Arc<KeyspaceGroupManager>,
    term: Arc<GroupReconcileTerm>,
    watch: ReconciledWatch,
}
pub struct GroupNodeObserver {
    manager: Arc<KeyspaceGroupManager>,
    term: Arc<GroupReconcileTerm>,
    watch: ReconciledWatch,
}
impl GroupMetadataObserver {
    /// Force a complete replacement after the host repairs invalid source metadata.
    pub async fn reload(&mut self) -> Result<i64, StoreError> {
        let (snapshot, watch) = ReconciledWatch::load(
            self.manager.store.clone(),
            self.manager.paths.keyspace_group_id_prefix().into_bytes(),
            true,
            100,
            &self.term.cancel,
        )
        .await?;
        let mut entries = BTreeMap::new();
        for row in snapshot.entries {
            let id = path_id(&row.key)?;
            let entry = decode_group_reconcile_entry(&row.value)?;
            if entry.id != id {
                return Err(StoreError::Backend);
            }
            entries.insert(id, entry);
        }
        self.manager
            .with_reconcile_term(&self.term, || *self.term.entries.lock().unwrap() = entries)?;
        self.watch = watch;
        Ok(self.watch.next_revision())
    }
    pub fn next_revision(&self) -> i64 {
        self.watch.next_revision()
    }
    pub async fn next(&mut self) -> Result<i64, StoreError> {
        let manager = &self.manager;
        let term = &self.term;
        self.watch
            .apply_next(&term.cancel, |batch| {
                let mut entries = term.entries.lock().unwrap().clone();
                for event in &batch.events {
                    let id = path_id(event.key())?;
                    match event {
                        Event::Put(row) => {
                            let entry = decode_group_reconcile_entry(&row.value)?;
                            if entry.id != id {
                                return Err(StoreError::Backend);
                            }
                            entries.insert(id, entry);
                        }
                        Event::Delete { .. } => {
                            entries.remove(&id);
                        }
                    }
                }
                manager.with_reconcile_term(term, || *term.entries.lock().unwrap() = entries)
            })
            .await
    }
}
impl GroupNodeObserver {
    pub async fn reload(&mut self) -> Result<i64, StoreError> {
        let (snapshot, watch) = ReconciledWatch::load(
            self.manager.store.clone(),
            self.manager.paths.service_path("tso").into_bytes(),
            true,
            100,
            &self.term.cancel,
        )
        .await?;
        let keys: BTreeSet<_> = snapshot.entries.iter().map(|row| row.key.clone()).collect();
        let mut events: Vec<_> = snapshot.entries.into_iter().map(Event::Put).collect();
        events.extend(
            self.term
                .registry
                .lock()
                .unwrap()
                .keys()
                .filter(|key| !keys.contains(*key))
                .cloned()
                .map(|key| Event::Delete {
                    key,
                    revision: snapshot.revision,
                }),
        );
        publish_nodes(
            &self.manager,
            &self.term,
            &WatchBatch {
                events,
                revision: snapshot.revision,
            },
        )?;
        self.watch = watch;
        Ok(self.watch.next_revision())
    }
    pub fn next_revision(&self) -> i64 {
        self.watch.next_revision()
    }
    pub async fn next(&mut self) -> Result<i64, StoreError> {
        let manager = &self.manager;
        let term = &self.term;
        self.watch
            .apply_next(&term.cancel, |batch| publish_nodes(manager, term, batch))
            .await
    }
}
fn publish_nodes(
    manager: &KeyspaceGroupManager,
    term: &GroupReconcileTerm,
    batch: &WatchBatch,
) -> Result<(), StoreError> {
    let mut registry = term.registry.lock().unwrap().clone();
    let mut updates = Vec::new();
    for event in &batch.events {
        match event {
            Event::Put(row) => {
                let address = registry_address(&row.value)?;
                registry.insert(row.key.clone(), address.clone());
                updates.push((true, address));
            }
            Event::Delete { key, .. } => {
                let address = registry.remove(key).ok_or(StoreError::Backend)?;
                updates.push((false, address));
            }
        }
    }
    manager.with_reconcile_term(term, || {
        for (put, address) in updates {
            if put {
                manager.nodes.put(address);
            } else {
                manager.nodes.delete(address);
            }
        }
        *term.registry.lock().unwrap() = registry
    })
}
/// Per-group errors are retained; backend/term failures stop this pass like Go.
#[derive(Debug, Default)]
pub struct GroupReconcileReport {
    pub attempted: Vec<u32>,
    pub failures: Vec<(u32, GroupNodesError)>,
    pub aborted: bool,
}
pub fn abort_group_reconcile(error: &GroupNodesError) -> bool {
    let fatal = |error: &StoreError| {
        matches!(
            error,
            StoreError::Cancelled
                | StoreError::Unavailable
                | StoreError::PermissionDenied
                | StoreError::Backend
        )
    };
    match error {
        GroupNodesError::Cancelled
        | GroupNodesError::Deadline
        | GroupNodesError::NoAvailableNode => true,
        GroupNodesError::Leadership(ElectionError::Store(error)) => fatal(error),
        GroupNodesError::Leadership(
            ElectionError::InvalidArgument | ElectionError::IdentityConflict,
        ) => false,
        GroupNodesError::Leadership(_) => true,
        GroupNodesError::Catalog(GroupCatalogError::Storage(
            KeyspaceGroupStoreError::Endpoint(EndpointError::Store(_)),
        )) => true,
        GroupNodesError::Catalog(GroupCatalogError::Store(error)) => fatal(error),
        _ => false,
    }
}

impl KeyspaceGroupManager {
    pub(crate) fn with_reconcile_term<T>(
        &self,
        term: &GroupReconcileTerm,
        publish: impl FnOnce() -> T,
    ) -> Result<T, StoreError> {
        let current = self.reconcile.lock().unwrap();
        if term.cancel.is_cancelled()
            || !current
                .as_ref()
                .and_then(std::sync::Weak::upgrade)
                .is_some_and(|current| std::ptr::eq(current.as_ref(), term))
        {
            return Err(StoreError::Cancelled);
        }
        Ok(publish())
    }
    /// Load both directories consistently, replace the previous observation term,
    /// and return pull drivers. The host owns cancellation, retries and scheduling.
    pub async fn start_reconcile(
        self: &Arc<Self>,
        leadership: Arc<Leadership>,
        cancel: &CancellationToken,
    ) -> Result<GroupReconcile, GroupReconcileError> {
        if !leadership.uses_store(&self.store) || leadership.cluster_id() != self.paths.cluster_id()
        {
            return Err(StoreError::InvalidArgument.into());
        }
        leadership.validate().await?;
        let term = Arc::new(GroupReconcileTerm {
            leadership,
            cancel: cancel.child_token(),
            entries: Mutex::new(BTreeMap::new()),
            registry: Mutex::new(BTreeMap::new()),
        });
        {
            let mut current = self.reconcile.lock().unwrap();
            if let Some(previous) = current
                .replace(Arc::downgrade(&term))
                .and_then(|old| old.upgrade())
            {
                previous.cancel();
            }
        }
        let (group_snapshot, groups) = ReconciledWatch::load(
            self.store.clone(),
            self.paths.keyspace_group_id_prefix().into_bytes(),
            true,
            100,
            &term.cancel,
        )
        .await?;
        let (node_snapshot, nodes) = ReconciledWatch::load(
            self.store.clone(),
            self.paths.service_path("tso").into_bytes(),
            true,
            100,
            &term.cancel,
        )
        .await?;
        let mut cache = empty_index();
        let mut entries = BTreeMap::new();
        for row in group_snapshot.entries {
            let id = path_id(&row.key)?;
            let group: KeyspaceGroup =
                serde_json::from_slice(&row.value).map_err(|_| StoreError::Backend)?;
            if group.id != id {
                return Err(StoreError::Backend.into());
            }
            entries.insert(
                id,
                GroupReconcileEntry {
                    id,
                    members: group.members.clone(),
                    transitioning: group.is_splitting() || group.is_merging(),
                },
            );
            put_index(&mut cache, group);
        }
        let mut registry = BTreeMap::new();
        for row in node_snapshot.entries {
            registry.insert(row.key, registry_address(&row.value)?);
        }
        let mut index = self.groups.write().await;
        term.leadership.validate().await?;
        self.with_reconcile_term(&term, || {
            *index = cache;
            *term.entries.lock().unwrap() = entries;
            for old in self.nodes.get_all() {
                self.nodes.delete(old);
            }
            for address in registry.values() {
                self.nodes.put(address.clone());
            }
            *term.registry.lock().unwrap() = registry;
        })?;
        Ok(GroupReconcile {
            groups: GroupMetadataObserver {
                manager: self.clone(),
                term: term.clone(),
                watch: groups,
            },
            nodes: GroupNodeObserver {
                manager: self.clone(),
                term: term.clone(),
                watch: nodes,
            },
            term,
        })
    }
    /// Revalidate only indexed candidates against storage and fence each commit.
    pub async fn reconcile_once(
        &self,
        term: &GroupReconcileTerm,
    ) -> Result<GroupReconcileReport, GroupReconcileError> {
        self.with_reconcile_term(term, || ())?;
        if self.get_nodes_count() == 0 {
            return Ok(GroupReconcileReport::default());
        }
        Ok(self
            .reconcile_candidate_ids(
                term.groups_needing_allocation(&self.nodes.get_all()),
                &term.cancel,
                Some(term),
            )
            .await)
    }
    pub(crate) async fn reconcile_candidate_ids(
        &self,
        ids: Vec<u32>,
        cancel: &CancellationToken,
        term: Option<&GroupReconcileTerm>,
    ) -> GroupReconcileReport {
        let mut report = GroupReconcileReport::default();
        for id in ids {
            if cancel.is_cancelled() {
                report.aborted = true;
                break;
            }
            report.attempted.push(id);
            let result = if let Some(term) = term {
                match self.with_reconcile_term(term, || ()) {
                    Ok(()) => {
                        self.alloc_nodes_in_term(
                            id,
                            None,
                            DEFAULT_KEYSPACE_GROUP_REPLICA_COUNT,
                            cancel,
                            Some(term),
                        )
                        .await
                    }
                    Err(error) => Err(GroupNodesError::Catalog(GroupCatalogError::Store(error))),
                }
            } else {
                self.alloc_nodes(id, None, DEFAULT_KEYSPACE_GROUP_REPLICA_COUNT, cancel)
                    .await
            };
            if let Err(error) = result {
                let abort = abort_group_reconcile(&error);
                report.failures.push((id, error));
                if abort {
                    report.aborted = true;
                    break;
                }
            }
        }
        report
    }
}
