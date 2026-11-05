// Copyright 2026 AsterSQL.
use crate::{
    BoundType, Cache, GroupAssignmentError, GroupCatalogError, GroupMembershipError, Kernel,
    KeyspaceGroupManager, KeyspaceRegionSplit, KeyspaceSplitFailure, KeyspaceState,
    MetaServiceGroupManager,
};
use astersql_pd_id::{Allocator, AllocatorError};
use astersql_pd_storage_domain::{
    Base, CallbackTxnError, Keyspace, KeyspaceIdentity, KeyspaceMetaError, KeyspaceStorage,
    StorageEndpoint, StoreError, UserKind,
};
use std::{
    collections::HashMap,
    convert::Infallible,
    sync::{Arc, RwLock},
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

#[derive(Debug, thiserror::Error)]
pub enum KeyspaceError {
    #[error("[PD:keyspace:ErrKeyspaceExists]keyspace already exists")]
    Exists,
    #[error("[PD:keyspace:ErrKeyspaceNotFound]keyspace does not exist")]
    NotFound,
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Storage(#[from] KeyspaceMetaError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Allocation(#[from] AllocatorError),
    #[error(transparent)]
    Assignment(#[from] GroupAssignmentError),
    #[error(transparent)]
    Group(#[from] GroupMembershipError),
    #[error(transparent)]
    Catalog(#[from] GroupCatalogError),
    #[error("{error}")]
    Split {
        error: KeyspaceSplitFailure,
        cleanup_error: Option<Box<KeyspaceError>>,
    },
    #[error(
        "[PD:keyspace:ErrKeyspaceConfigPreconditionFailed]keyspace config precondition failed, {0}"
    )]
    ConfigPrecondition(String),
    #[error("transaction conflict")]
    Conflict,
}
pub(crate) fn commit_error(e: CallbackTxnError<Infallible>) -> KeyspaceError {
    match e {
        CallbackTxnError::Conflict => KeyspaceError::Conflict,
        CallbackTxnError::Store(e) => e.into(),
        CallbackTxnError::Callback(e) => match e {},
    }
}
#[derive(Clone, Copy)]
pub struct KeyspaceCreateConfig {
    pub wait_region_split: bool,
    pub bound: BoundType,
}
impl Default for KeyspaceCreateConfig {
    fn default() -> Self {
        Self {
            wait_region_split: true,
            bound: BoundType::Txn,
        }
    }
}
#[derive(Default)]
pub struct CreateKeyspaceRequest {
    pub name: String,
    pub config: HashMap<String, String>,
    pub create_time: i64,
}
#[derive(Default)]
pub struct CreateKeyspaceByIdRequest {
    pub id: Option<u32>,
    pub name: String,
    pub config: HashMap<String, String>,
    pub create_time: i64,
}
/// Host-owned collaborators. Absent group managers preserve Go's nil-manager semantics.
pub struct KeyspaceCreateResources {
    pub kernel: Kernel,
    pub allocator: Option<Arc<Allocator>>,
    pub splitter: Option<Arc<KeyspaceRegionSplit>>,
    pub groups: Option<Arc<KeyspaceGroupManager>>,
    pub meta_groups: Option<Arc<MetaServiceGroupManager>>,
    pub config: Arc<RwLock<KeyspaceCreateConfig>>,
}
impl Default for KeyspaceCreateResources {
    fn default() -> Self {
        Self {
            kernel: Kernel::current(),
            allocator: None,
            splitter: None,
            groups: None,
            meta_groups: None,
            config: Arc::new(RwLock::new(KeyspaceCreateConfig::default())),
        }
    }
}
/// Owns keyspace metadata operations over the host's codec and shared store.
pub struct KeyspaceManager {
    pub(crate) storage: KeyspaceStorage,
    pub cache: Arc<Cache>,
    pub(crate) mutation: Mutex<()>,
    pub(crate) resources: KeyspaceCreateResources,
    pub(crate) removal_observer: RwLock<Option<crate::GCBarrierInvalidator>>,
}
impl KeyspaceManager {
    pub fn new(storage: KeyspaceStorage) -> Self {
        Self::with_resources(storage, KeyspaceCreateResources::default())
    }
    pub fn with_resources(storage: KeyspaceStorage, resources: KeyspaceCreateResources) -> Self {
        Self {
            storage,
            cache: Arc::new(Cache::default()),
            mutation: Mutex::new(()),
            resources,
            removal_observer: RwLock::new(None),
        }
    }
    pub(crate) fn publish(&self, meta: &Keyspace) {
        self.cache.save(
            meta.id(),
            meta.name.clone(),
            match meta.state {
                0 => KeyspaceState::Enabled,
                1 => KeyspaceState::Disabled,
                2 => KeyspaceState::Archived,
                3 => KeyspaceState::Tombstone,
                n => KeyspaceState::Unknown(n),
            },
        );
    }
    async fn buffer_new(
        &self,
        txn: &mut astersql_pd_storage_domain::CallbackTxn<'_>,
        meta: &Keyspace,
    ) -> Result<(), KeyspaceError> {
        let s = &self.storage;
        let p = &s.paths;
        if s.load_keyspace_id(txn, p, &meta.name).await?.is_some() {
            return Err(KeyspaceError::Exists);
        }
        s.save_keyspace_id(txn, p, meta.id(), &meta.name)?;
        if s.load_keyspace_meta(txn, p, meta.id()).await?.is_some() {
            return Err(KeyspaceError::Exists);
        }
        s.save_keyspace_meta(txn, p, meta)?;
        Ok(())
    }
    pub async fn save_new(&self, meta: &Keyspace) -> Result<(), KeyspaceError> {
        let _mutation = self.mutation.lock().await;
        let mut txn = Base::new(self.storage.store.as_ref()).begin_txn();
        self.buffer_new(&mut txn, meta).await?;
        txn.commit::<Infallible>().await.map_err(commit_error)?;
        self.publish(meta);
        Ok(())
    }
    pub(crate) async fn decorate(&self, mut meta: Keyspace) -> Keyspace {
        if let Some(m) = &self.resources.meta_groups {
            meta.config = m
                .attach_endpoints(&meta.config.into_iter().collect())
                .await
                .into_iter()
                .collect();
        }
        meta
    }
    pub async fn load_by_name(&self, name: &str) -> Result<Keyspace, KeyspaceError> {
        let s = &self.storage;
        let mut txn = Base::new(s.store.as_ref()).begin_txn();
        let id = s
            .load_keyspace_id(&mut txn, &s.paths, name)
            .await?
            .ok_or(KeyspaceError::NotFound)?;
        let meta = s
            .load_keyspace_meta(&mut txn, &s.paths, id)
            .await?
            .ok_or(KeyspaceError::NotFound)?;
        txn.commit::<Infallible>().await.map_err(commit_error)?;
        Ok(self.decorate(meta).await)
    }
    pub async fn load_by_id(&self, id: u32) -> Result<Keyspace, KeyspaceError> {
        let s = &self.storage;
        let mut txn = Base::new(s.store.as_ref()).begin_txn();
        let meta = s
            .load_keyspace_meta(&mut txn, &s.paths, id)
            .await?
            .ok_or(KeyspaceError::NotFound)?;
        txn.commit::<Infallible>().await.map_err(commit_error)?;
        Ok(self.decorate(meta).await)
    }
    pub async fn load_range(
        &self,
        start: u32,
        limit: usize,
    ) -> Result<Vec<Keyspace>, KeyspaceError> {
        if start > 0xffffff {
            return Err(KeyspaceError::Validation(format!(
                "startID of the scan {start} exceeds spaceID Max 16777215"
            )));
        }
        let s = &self.storage;
        let mut txn = Base::new(s.store.as_ref()).begin_txn();
        let metas = s
            .load_range_keyspace(&mut txn, &s.paths, start, limit)
            .await?;
        txn.commit::<Infallible>().await.map_err(commit_error)?;
        let mut result = Vec::with_capacity(metas.len());
        for meta in metas {
            result.push(self.decorate(meta).await);
        }
        Ok(result)
    }
    async fn precheck(&self, name: &str, id: Option<u32>) -> Result<(), KeyspaceError> {
        let s = &self.storage;
        let mut txn = Base::new(s.store.as_ref()).begin_txn();
        if s.load_keyspace_id(&mut txn, &s.paths, name)
            .await?
            .is_some()
        {
            return Err(KeyspaceError::Exists);
        }
        if let Some(id) = id
            && s.load_keyspace_meta(&mut txn, &s.paths, id)
                .await?
                .is_some()
        {
            return Err(KeyspaceError::Exists);
        }
        txn.commit::<Infallible>().await.map_err(commit_error)
    }
    pub async fn create(
        &self,
        cancel: &CancellationToken,
        request: CreateKeyspaceRequest,
    ) -> Result<Keyspace, KeyspaceError> {
        self.resources
            .kernel
            .validate_name(&request.name)
            .map_err(|e| KeyspaceError::Validation(e.to_string()))?;
        self.precheck(&request.name, None).await?;
        let allocator =
            self.resources.allocator.as_ref().ok_or_else(|| {
                KeyspaceError::Validation("keyspace allocator is unavailable".into())
            })?;
        let id = allocator.alloc().await? as u32;
        self.resources
            .kernel
            .validate_id(id)
            .map_err(|e| KeyspaceError::Validation(e.to_string()))?;
        self.create_inner(cancel, id, request).await
    }
    pub async fn create_by_id(
        &self,
        cancel: &CancellationToken,
        request: CreateKeyspaceByIdRequest,
    ) -> Result<Keyspace, KeyspaceError> {
        let id = request
            .id
            .ok_or_else(|| KeyspaceError::Validation("keyspace id is empty".into()))?;
        if request.name.is_empty() {
            return Err(KeyspaceError::Validation("keyspace name is empty".into()));
        }
        self.resources
            .kernel
            .validate_name(&request.name)
            .map_err(|e| KeyspaceError::Validation(e.to_string()))?;
        self.precheck(&request.name, Some(id)).await?;
        self.create_inner(
            cancel,
            id,
            CreateKeyspaceRequest {
                name: request.name,
                config: request.config,
                create_time: request.create_time,
            },
        )
        .await
    }
    async fn create_inner(
        &self,
        cancel: &CancellationToken,
        id: u32,
        request: CreateKeyspaceRequest,
    ) -> Result<Keyspace, KeyspaceError> {
        let config_snapshot = *self.resources.config.read().unwrap();
        let mut config = request.config;
        for key in [
            "meta_service_group_addrs",
            "meta_service_group_id",
            "tso_keyspace_group_id",
        ] {
            config.remove(key);
        }
        let kind = UserKind::from_name(config.get("user_kind").map(String::as_str).unwrap_or(""));
        if let Some(groups) = &self.resources.groups {
            let group = groups.creation_group(kind).await?;
            config.insert("tso_keyspace_group_id".into(), group.to_string());
            config.insert("user_kind".into(), kind.to_string());
            config.insert(
                "region_bound_type".into(),
                config_snapshot.bound.to_string(),
            );
        }
        if self.resources.kernel == Kernel::NextGeneration
            && config
                .get("gc_management_type")
                .is_none_or(String::is_empty)
        {
            config.insert("gc_management_type".into(), "keyspace_level".into());
        }
        let mut meta = Keyspace {
            identity: Some(KeyspaceIdentity::Id(id)),
            name: request.name,
            state: 1,
            created_at: request.create_time,
            state_changed_at: request.create_time,
            config,
        };
        {
            let _mutation = self.mutation.lock().await;
            let mut txn = Base::new(self.storage.store.as_ref()).begin_txn();
            let mut reservation = None;
            if let Some(m) = &self.resources.meta_groups {
                match m.assign_in_txn(&mut txn, 1).await {
                    Ok(group) => {
                        meta.config
                            .insert("meta_service_group_id".into(), group.group.clone());
                        reservation = Some(group);
                    }
                    Err(GroupAssignmentError::NoAvailableGroups) => {}
                    Err(e) => return Err(e.into()),
                }
            }
            self.buffer_new(&mut txn, &meta).await?;
            txn.commit::<Infallible>().await.map_err(commit_error)?;
            drop(reservation);
            self.publish(&meta);
        }
        let wait_region_split = self.resources.config.read().unwrap().wait_region_split;
        let split = match &self.resources.splitter {
            Some(splitter) => {
                splitter
                    .split(
                        cancel,
                        id,
                        wait_region_split,
                        config_snapshot.bound,
                        i128::from(request.create_time) * 1_000_000_000,
                    )
                    .await
            }
            None => Err(KeyspaceSplitFailure {
                error: crate::KeyspaceSplitError::Unsupported,
                cleanup_error: None,
            }),
        };
        if let Err(error) = split {
            // Cleanup is best effort and must not replace the split failure.
            let cleanup_error = self.rollback_creation(&meta).await.err().map(Box::new);
            return Err(KeyspaceError::Split {
                error,
                cleanup_error,
            });
        }
        self.enable_created(&meta).await?;
        meta.state = 0;
        if let Some(groups) = &self.resources.groups {
            groups
                .add_created_keyspace(kind, &meta.config["tso_keyspace_group_id"], id)
                .await?;
        }
        Ok(self.decorate(meta).await)
    }
    async fn rollback_creation(&self, meta: &Keyspace) -> Result<(), KeyspaceError> {
        let _mutation = self.mutation.lock().await;
        let mut txn = Base::new(self.storage.store.as_ref()).begin_txn();
        self.storage
            .remove_keyspace(&mut txn, &self.storage.paths, meta.id(), &meta.name)?;
        if let Some(m) = &self.resources.meta_groups
            && let Some(id) = meta.config.get("meta_service_group_id")
        {
            m.rollback_in_txn(&mut txn, id).await?;
        }
        txn.commit::<Infallible>().await.map_err(commit_error)
    }
    async fn enable_created(&self, meta: &Keyspace) -> Result<(), KeyspaceError> {
        self.update_state_by_id(meta.id(), 0, meta.created_at)
            .await
            .map(|_| ())
    }
}
impl KeyspaceGroupManager {
    async fn creation_group(&self, kind: UserKind) -> Result<u32, KeyspaceError> {
        let index = self.groups.read().await;
        let groups = index
            .get(&kind.0)
            .ok_or_else(|| KeyspaceError::Validation(format!("user kind {kind} not found")))?;
        groups
            .values()
            .min_by_key(|g| (g.keyspaces.as_ref().map_or(0, Vec::len), g.id))
            .map(|g| g.id)
            .ok_or_else(|| {
                KeyspaceError::Validation(format!("no keyspace group for user kind {kind}"))
            })
    }
    async fn add_created_keyspace(
        &self,
        kind: UserKind,
        group_id: &str,
        id: u32,
    ) -> Result<(), KeyspaceError> {
        let _membership = self.membership_mutex.lock().await;
        let mut index = self.groups.write().await;
        let group_id: u32 = group_id
            .parse()
            .map_err(|_| KeyspaceError::Validation("invalid group id".into()))?;
        let endpoint = StorageEndpoint::new(self.store.clone());
        let mut txn = Base::new(self.store.as_ref()).begin_txn();
        let mut group = endpoint
            .load_keyspace_group(&mut txn, &self.paths, group_id)
            .await
            .map_err(GroupCatalogError::from)?
            .filter(|g| UserKind::from_name(&g.user_kind) == kind)
            .ok_or_else(|| {
                KeyspaceError::Validation(crate::GroupSplitError::NotExists(group_id).to_string())
            })?;
        if group.is_splitting() {
            return Err(GroupCatalogError::InSplit(group_id).into());
        }
        if group.is_merging() {
            return Err(KeyspaceError::Validation(
                crate::GroupMergeError::InMerging(group_id).to_string(),
            ));
        }
        if !group
            .keyspaces
            .as_ref()
            .is_some_and(|keys| keys.contains(&id))
        {
            group.keyspaces.get_or_insert_default().push(id);
            endpoint
                .save_keyspace_group(&mut txn, &self.paths, &group)
                .map_err(GroupCatalogError::from)?;
        }
        txn.commit::<Infallible>().await.map_err(commit_error)?;
        crate::group_catalog::put_index(&mut index, group);
        Ok(())
    }
}
