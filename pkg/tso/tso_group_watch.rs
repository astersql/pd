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
        let worker = config.runtime.spawn(allocator_worker(
            config.clone(),
            id,
            transitioning,
            cancel.clone(),
            oracle.clone(),
        ));
        Self {
            id,
            oracle,
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
        let term_cancel = cancel.child_token();
        let elected = Leadership::campaign_with_key(
            config.store.clone(),
            term_cancel.clone(),
            config.cluster_id,
            key.clone(),
            (config.participant)(id),
            config.lease_seconds,
            vec![],
        )
        .await;
        if let Ok(term) = elected {
            let term = Arc::new(term);
            if let Ok(oracle) = Oracle::new_for_group(
                config.store.clone(),
                term.clone(),
                config.clock.clone(),
                term_cancel.clone(),
                config.save_interval_millis,
                id,
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
        if let Some(batch) = self.pending.take() {
            if let Err(error) = self.apply(&batch).await {
                self.pending = Some(batch);
                return Err(error);
            }
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
            .map(|g| (g.id, g.is_splitting() || g.is_merging()))
            .collect::<BTreeMap<_, _>>();
        let removed = self
            .allocators
            .iter()
            .filter(|(id, a)| assigned.get(id).is_none_or(|t| *t != a.transitioning))
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
