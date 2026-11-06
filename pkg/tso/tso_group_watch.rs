// Copyright 2026 AsterSQL.
//! Host-owned observation of TSO assignments and allocator resources.
use crate::{Clock, Oracle, TsoError};
use astersql_pd_etcdutil::ReconciledWatch;
use astersql_pd_keypath::{ClusterPaths, MsParam};
use astersql_pd_member::Leadership;
use astersql_pd_storage_domain::{Event, KeyspaceGroup, KeyspaceGroupMember, WatchBatch};
use astersql_pd_storage_kv::{MetaStore, StoreError};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
/// Resources supplied by the host; participant bytes are encoded at its protocol boundary.
#[derive(Clone)]
pub struct GroupWatchConfig {
    pub store: Arc<dyn MetaStore>,
    pub runtime: tokio::runtime::Handle,
    pub cancel: CancellationToken,
    pub cluster_id: u64,
    pub address: String,
    pub bootstrap_keyspace: u32,
    pub participant: Arc<dyn Fn(u32) -> Vec<u8> + Send + Sync>,
    pub clock: Arc<dyn Clock>,
    pub lease_seconds: i64,
    pub save_interval_millis: u64,
}
/// Actual allocator worker and its immutable group identity.
pub struct GroupAllocator {
    pub id: u32,
    oracle: Arc<tokio::sync::RwLock<Option<Arc<Oracle>>>>,
    cancel: CancellationToken,
    term: Arc<tokio::sync::RwLock<Option<Arc<Leadership>>>>,
    transitioning: bool,
    worker: Option<tokio::task::JoinHandle<()>>,
}
impl Drop for GroupAllocator {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
impl GroupAllocator {
    pub async fn allocate(&self, count: u32) -> Result<crate::Timestamp, TsoError> {
        if self.cancel.is_cancelled() {
            return Err(TsoError::Cancelled);
        }
        let oracle = self
            .oracle
            .read()
            .await
            .clone()
            .ok_or(TsoError::Member(astersql_pd_member::MemberError::NotLeader))?;
        oracle.allocate(count).await
    }
    /// Transfer this allocator's current term using the shared expected-primary algorithm.
    pub async fn transfer(
        &self,
        config: &GroupWatchConfig,
        entries: &[astersql_pd_mcs_discovery::ServiceRegistryEntry],
        target: &str,
        members: &[String],
    ) -> Result<(), String> {
        let term = self
            .term
            .read()
            .await
            .clone()
            .ok_or("current member is not serving as primary, please check leadership")?;
        astersql_pd_mcs_utils::transfer_primary(
            config.store.as_ref(),
            &term,
            &ClusterPaths::new(config.cluster_id),
            &MsParam {
                service_name: "tso".into(),
                group_id: self.id,
            },
            entries,
            &config.address,
            target,
            members,
            config.lease_seconds,
        )
        .await?;
        *self.oracle.write().await = None;
        Ok(())
    }
    pub async fn is_primary(&self) -> bool {
        self.term
            .read()
            .await
            .as_ref()
            .is_some_and(|t| !t.is_expired())
            && self.oracle.read().await.is_some()
    }
    pub async fn shutdown(mut self) -> Result<(), TsoError> {
        self.cancel.cancel();
        if let Some(worker) = self.worker.take() {
            worker.await.map_err(|_| TsoError::Cancelled)?;
        }
        Ok(())
    }
    fn start(config: &GroupWatchConfig, id: u32, transitioning: bool) -> Self {
        let cancel = config.cancel.child_token();
        let oracle = Arc::new(tokio::sync::RwLock::new(None));
        let term = Arc::new(tokio::sync::RwLock::new(None));
        let worker = config.runtime.spawn(allocator_worker(
            config.clone(),
            id,
            transitioning,
            cancel.clone(),
            oracle.clone(),
            term.clone(),
        ));
        Self {
            id,
            oracle,
            term,
            cancel,
            transitioning,
            worker: Some(worker),
        }
    }
}
async fn allocator_worker(
    config: GroupWatchConfig,
    id: u32,
    transitioning: bool,
    cancel: CancellationToken,
    state: Arc<tokio::sync::RwLock<Option<Arc<Oracle>>>>,
    serving_term: Arc<tokio::sync::RwLock<Option<Arc<Leadership>>>>,
) {
    // Split/merge handoff remains owned by its domain tasks. Never serve a fresh
    // independent window before those tasks have established a safe boundary.
    if transitioning {
        cancel.cancelled().await;
        return;
    }
    let paths = ClusterPaths::new(config.cluster_id);
    let key = paths
        .election_path(Some(&MsParam {
            service_name: "tso".into(),
            group_id: id,
        }))
        .into_bytes();
    while !cancel.is_cancelled() {
        let metadata_key = paths.keyspace_group_id_path(id).into_bytes();
        let snapshot = match config
            .store
            .read(astersql_pd_storage_domain::Read::prefix(
                &metadata_key,
                astersql_pd_storage_domain::Consistency::Linearizable,
            ))
            .await
        {
            Ok(s) => s,
            Err(_) => {
                tokio::select! {_=cancel.cancelled()=>break,_=tokio::time::sleep(Duration::from_millis(100))=>{}}
                continue;
            }
        };
        let row = snapshot.entries.iter().find(|e| e.key == metadata_key);
        let metadata_guard = if let Some(row) = row {
            let group: KeyspaceGroup = match serde_json::from_slice(&row.value) {
                Ok(g) => g,
                Err(_) => break,
            };
            if !group
                .members
                .iter()
                .flatten()
                .any(|n| n.is_address_equivalent(&config.address))
                && !(id == 0 && group.members.as_ref().is_none_or(Vec::is_empty))
            {
                break;
            }
            if group.is_split_target() || group.is_merging() {
                break;
            }
            vec![astersql_pd_storage_domain::Compare::Value(
                metadata_key,
                row.value.clone(),
            )]
        } else if id == 0 {
            vec![astersql_pd_storage_domain::Compare::CreateRevision(
                metadata_key,
                0,
            )]
        } else {
            break;
        };
        let param = MsParam {
            service_name: "tso".into(),
            group_id: id,
        };
        let expected = match astersql_pd_mcs_utils::expected_primary(
            config.store.as_ref(),
            &paths,
            &param,
        )
        .await
        {
            Ok(e) => e,
            Err(_) => {
                tokio::select! {_=cancel.cancelled()=>break,_=tokio::time::sleep(Duration::from_millis(100))=>{}}
                continue;
            }
        };
        if expected.as_ref().is_some_and(|e| {
            !astersql_pd_mcs_utils::identifies(
                &astersql_pd_mcs_discovery::ServiceRegistryEntry {
                    service_addr: config.address.clone(),
                    ..Default::default()
                },
                &String::from_utf8_lossy(&e.value),
            )
        }) {
            tokio::select! {_=cancel.cancelled()=>break,_=tokio::time::sleep(Duration::from_millis(50))=>{}}
            continue;
        }
        let mut campaign_guard = metadata_guard.clone();
        if let Some(e) = &expected {
            campaign_guard.push(astersql_pd_storage_domain::Compare::Value(
                e.key.clone(),
                e.value.clone(),
            ));
        }
        let term_cancel = cancel.child_token();
        let elected = Leadership::campaign_with_key(
            config.store.clone(),
            term_cancel.clone(),
            config.cluster_id,
            key.clone(),
            (config.participant)(id),
            config.lease_seconds,
            campaign_guard,
        )
        .await;
        if let Ok(term) = elected {
            let term = Arc::new(term);
            if let Some(e) = &expected {
                let _ = astersql_pd_mcs_utils::clear_expected_primary(
                    config.store.as_ref(),
                    e.key.clone(),
                    e,
                )
                .await;
            }
            *serving_term.write().await = Some(term.clone());
            if let Ok(oracle) = Oracle::new_for_group_guarded(
                config.store.clone(),
                term.clone(),
                config.clock.clone(),
                term_cancel.clone(),
                config.save_interval_millis,
                id,
                metadata_guard,
            )
            .await
            {
                let oracle = Arc::new(oracle);
                *state.write().await = Some(oracle.clone());
                let mut update = tokio::time::interval(Duration::from_millis(50));
                update.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                let mut renew = tokio::time::interval(Duration::from_millis(
                    (config.lease_seconds as u64) * 1000 / 3,
                ));
                renew.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    let result = tokio::select! {biased;_=cancel.cancelled()=>break,_=renew.tick()=>term.renew().await.map(|_|()).map_err(TsoError::from),_=update.tick()=>oracle.update().await};
                    if result.is_err() {
                        break;
                    }
                }
                *state.write().await = None;
            }
            *serving_term.write().await = None;
            term_cancel.cancel();
            // Cleanup is allowed after cancellation; revoke only this election's lease.
            let _ = term.resign().await;
        }
        tokio::select! {biased;_=cancel.cancelled()=>break,_=tokio::time::sleep(Duration::from_millis(100))=>{}}
    }
    *state.write().await = None;
}
/// Pull-driven manager. The host owns retry policy, shutdown and its one runtime.
pub struct GroupAllocatorWatch {
    config: GroupWatchConfig,
    allocators: BTreeMap<u32, GroupAllocator>,
    groups: BTreeMap<u32, KeyspaceGroup>,
    watch: ReconciledWatch,
    pending: Option<WatchBatch>,
    revision: i64,
}
impl GroupAllocatorWatch {
    pub async fn load(mut config: GroupWatchConfig) -> Result<Self, TsoError> {
        if config.lease_seconds < 1 || config.save_interval_millis < 2 || config.address.is_empty()
        {
            return Err(TsoError::InvalidArgument);
        }
        config.cancel = config.cancel.child_token();
        let paths = ClusterPaths::new(config.cluster_id);
        let (snapshot, watch) = ReconciledWatch::load(
            config.store.clone(),
            paths.keyspace_group_id_prefix().into_bytes(),
            true,
            100,
            &config.cancel,
        )
        .await?;
        let mut result = Self {
            config,
            allocators: BTreeMap::new(),
            groups: BTreeMap::new(),
            watch,
            pending: None,
            revision: 0,
        };
        result
            .apply(&WatchBatch {
                revision: snapshot.revision,
                events: snapshot.entries.into_iter().map(Event::Put).collect(),
            })
            .await?;
        Ok(result)
    }
    pub fn allocator(&self, id: u32) -> Option<&GroupAllocator> {
        self.allocators.get(&id)
    }
    pub fn assigned_ids(&self) -> Vec<u32> {
        self.allocators.keys().copied().collect()
    }
    pub fn loaded_ids(&self) -> Vec<u32> {
        self.groups.keys().copied().collect()
    }
    pub fn group(&self, id: u32) -> Option<&KeyspaceGroup> {
        self.groups.get(&id)
    }
    pub fn revision(&self) -> i64 {
        self.revision
    }
    /// Resume at the last published revision; transient and compaction recovery
    /// are delegated to the canonical reconciled snapshot/watch implementation.
    pub async fn next(&mut self) -> Result<i64, TsoError> {
        self.receive_pending().await?;
        self.apply_pending().await
    }
    /// Bound only network receive time, allowing resource shutdown to finish before publication.
    pub async fn poll_next(&mut self, wait: Duration) -> Result<Option<i64>, TsoError> {
        match tokio::time::timeout(wait, self.receive_pending()).await {
            Ok(result) => result?,
            Err(_) => return Ok(None),
        }
        self.apply_pending().await.map(Some)
    }
    async fn receive_pending(&mut self) -> Result<(), TsoError> {
        if self.pending.is_none() {
            let mut received = None;
            self.watch
                .apply_next(&self.config.cancel, |batch| {
                    received = Some(batch.clone());
                    Ok(())
                })
                .await?;
            self.pending = received;
        }
        Ok(())
    }
    async fn apply_pending(&mut self) -> Result<i64, TsoError> {
        // Retain the consumed batch until publication, even if the caller drops this future
        // while shutdown is waiting for a real lease-revocation RPC.
        if let Some(batch) = self.pending.clone() {
            self.apply(&batch).await?;
            self.pending = None;
        }
        Ok(self.revision)
    }
    /// Explicit resnapshot is useful after repairing invalid source data.
    pub async fn reload(&mut self) -> Result<i64, TsoError> {
        let paths = ClusterPaths::new(self.config.cluster_id);
        let (snapshot, watch) = ReconciledWatch::load(
            self.config.store.clone(),
            paths.keyspace_group_id_prefix().into_bytes(),
            true,
            100,
            &self.config.cancel,
        )
        .await?;
        let mut events = snapshot
            .entries
            .into_iter()
            .map(Event::Put)
            .collect::<Vec<_>>();
        let present = events
            .iter()
            .filter_map(|e| parse_id(e.key()).ok())
            .collect::<std::collections::BTreeSet<_>>();
        for id in self.groups.keys() {
            if !present.contains(id) {
                events.push(Event::Delete {
                    key: paths.keyspace_group_id_path(*id).into_bytes(),
                    revision: snapshot.revision,
                });
            }
        }
        self.apply(&WatchBatch {
            revision: snapshot.revision,
            events,
        })
        .await?;
        self.watch = watch;
        self.pending = None;
        Ok(self.revision)
    }
    async fn apply(&mut self, batch: &WatchBatch) -> Result<(), TsoError> {
        let mut groups = self.groups.clone();
        // Decode the whole batch before changing live resources or publishing a cursor.
        for event in &batch.events {
            let id = parse_id(event.key())?;
            if id >= 4096 {
                continue;
            }
            match event {
                Event::Put(row) => {
                    let group: KeyspaceGroup =
                        serde_json::from_slice(&row.value).map_err(|_| StoreError::Backend)?;
                    if group.id != id {
                        return Err(StoreError::Backend.into());
                    }
                    groups.insert(id, group);
                }
                Event::Delete { .. } => {
                    groups.remove(&id);
                }
            }
        }
        let default = groups.entry(0).or_insert_with(|| KeyspaceGroup {
            id: 0,
            keyspaces: Some(vec![self.config.bootstrap_keyspace]),
            ..Default::default()
        });
        if default.members.as_ref().is_none_or(Vec::is_empty) {
            default.members = Some(vec![KeyspaceGroupMember {
                address: self.config.address.clone(),
                priority: 0,
            }]);
        }
        // The serving view protects the bootstrap keyspace without rewriting
        // the persisted directory, matching Go's local membership repair.
        for group in groups.values_mut() {
            if group.id == 0 {
                let keys = group.keyspaces.get_or_insert_with(Vec::new);
                keys.sort_unstable();
                if !keys.contains(&self.config.bootstrap_keyspace) {
                    keys.insert(0, self.config.bootstrap_keyspace);
                }
            } else if let Some(keys) = &mut group.keyspaces {
                keys.sort_unstable();
                keys.retain(|id| *id != self.config.bootstrap_keyspace);
            }
        }
        let assigned = groups
            .values()
            .filter(|g| {
                g.members
                    .iter()
                    .flatten()
                    .any(|n| n.is_address_equivalent(&self.config.address))
            })
            .map(|g| (g.id, g.is_split_target() || g.is_merging()))
            .collect::<BTreeMap<_, _>>();
        let removed = self
            .allocators
            .iter()
            .filter(|(id, a)| {
                assigned.get(id).is_none_or(|t| *t != a.transitioning)
                    || a.worker.as_ref().is_some_and(|w| w.is_finished())
            })
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for id in removed {
            if let Some(a) = self.allocators.remove(&id) {
                a.shutdown().await?;
            }
        }
        for (id, transitioning) in assigned {
            self.allocators
                .entry(id)
                .or_insert_with(|| GroupAllocator::start(&self.config, id, transitioning));
        }
        self.groups = groups;
        self.revision = batch.revision;
        Ok(())
    }
    pub async fn shutdown(mut self) -> Result<(), TsoError> {
        self.stop_allocators().await
    }
    /// Stop every owned allocator while retaining the last readable assignment snapshot.
    /// Evict primary groups after validating all candidates and split states.
    pub async fn evict_primaries(
        &mut self,
        entries: &[astersql_pd_mcs_discovery::ServiceRegistryEntry],
        target: &str,
    ) -> Result<std::collections::BTreeMap<u32, String>, (u16, String)> {
        let mut ids = vec![];
        for (id, a) in &self.allocators {
            if a.is_primary().await {
                ids.push(*id);
            }
        }
        for id in &ids {
            if let Some(group) = self.groups.get(id) {
                if group.is_split_source() || group.is_split_target() {
                    return Err((
                        500,
                        format!(
                            "[PD:keyspace:ErrKeyspaceGroupInSplit]keyspace group {id} is in split"
                        ),
                    ));
                }
                let members = group
                    .members
                    .iter()
                    .flatten()
                    .map(|m| m.address.clone())
                    .collect::<Vec<_>>();
                if !astersql_pd_mcs_utils::is_valid_primary_candidate(entries, target, &members) {
                    return Err((
                        400,
                        format!("new_primary {target:?} is not a member of keyspace group {id}"),
                    ));
                }
            }
        }
        let mut results = std::collections::BTreeMap::new();
        for id in ids {
            let members = self
                .groups
                .get(&id)
                .map(|g| {
                    g.members
                        .iter()
                        .flatten()
                        .map(|m| m.address.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let result = self.allocators[&id]
                .transfer(&self.config, entries, target, &members)
                .await;
            results.insert(id, result.map_or_else(|e| e, |_| "success".into()));
        }
        Ok(results)
    }
    pub async fn stop_allocators(&mut self) -> Result<(), TsoError> {
        self.config.cancel.cancel();
        let mut failure = None;
        for (_, allocator) in std::mem::take(&mut self.allocators) {
            if let Err(e) = allocator.shutdown().await {
                failure = Some(e);
            }
        }
        match failure {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}
impl Drop for GroupAllocatorWatch {
    fn drop(&mut self) {
        self.config.cancel.cancel();
    }
}
fn parse_id(key: &[u8]) -> Result<u32, StoreError> {
    astersql_pd_keypath::keyspace_group_id_from_path(
        std::str::from_utf8(key).map_err(|_| StoreError::Backend)?,
    )
    .and_then(|s| s.parse().ok())
    .ok_or(StoreError::Backend)
}
