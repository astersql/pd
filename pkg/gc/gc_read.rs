// Copyright 2026 AsterSQL.
use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
use astersql_pd_keyspace::{KeyspaceError, KeyspaceManager};
use astersql_pd_keyspace_constant::VALID_KEYSPACE_ID_MASK;
use astersql_pd_storage_domain::{
    EndpointError, GCBarrier, GCSafePointBatchError, GCStateTxnError, GlobalGCBarrier,
    SafePointReadError, StorageEndpoint,
};
use std::{
    collections::BTreeMap,
    convert::Infallible,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::{
    runtime::Handle,
    sync::{RwLock as AsyncRwLock, watch},
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GCState {
    pub keyspace_id: u32,
    pub is_keyspace_level: bool,
    pub gc_safe_point: u64,
    pub txn_safe_point: u64,
    pub gc_barriers: Vec<GCBarrier>,
}
impl GCState {
    pub(crate) fn pair(id: u32, txn: u64, gc: u64) -> Self {
        Self {
            keyspace_id: id,
            is_keyspace_level: id != NULL_KEYSPACE_ID,
            txn_safe_point: txn,
            gc_safe_point: gc,
            gc_barriers: vec![],
        }
    }
}
#[derive(Clone, Debug, thiserror::Error)]
pub enum GCReadError {
    #[error(
        "[PD:gc:ErrDecreasingGCSafePoint]trying to update GC safe point to a smaller value, current value: {current}, given: {given}"
    )]
    DecreasingGCSafePoint { current: u64, given: u64 },
    #[error(
        "[PD:gc:ErrGCSafePointExceedsTxnSafePoint]trying to update GC safe point to a too large value that exceeds the txn safe point, current value: {current}, given: {given}, current txn safe point: {txn}"
    )]
    GCExceedsTxn { current: u64, given: u64, txn: u64 },
    #[error("TTL of gc_worker's service safe point must be infinity")]
    LegacyWorkerTTL,

    #[error(
        "[PD:gc:ErrGlobalGCBarrierTSBehindTxnSafePoint]trying to set a global GC barrier on ts {barrier} which is already behind the txn safe point {txn} of keyspace {keyspace}"
    )]
    GlobalBarrierBehind {
        barrier: u64,
        txn: u64,
        keyspace: String,
    },
    #[error("keyspace metadata scan did not advance")]
    NonAdvancingKeyspaceScan,

    #[error("[PD:common:ErrInvalidArgument]invalid argument for {argument}: {value}")]
    InvalidArgument {
        argument: &'static str,
        value: String,
    },
    #[error(
        "[PD:gc:ErrReservedGCBarrierID]trying to set a GC barrier with a barrier ID that is reserved: {0}"
    )]
    ReservedBarrierId(String),
    #[error(
        "[PD:gc:ErrGCBarrierTSBehindTxnSafePoint]trying to set a GC barrier on ts {barrier} which is already behind the txn safe point {txn}"
    )]
    BarrierBehind { barrier: u64, txn: u64 },

    #[error(
        "[PD:gc:ErrDecreasingTxnSafePoint]trying to update txn safe point to a smaller value, current value: {current}, given: {given}"
    )]
    DecreasingTxnSafePoint { current: u64, given: u64 },
    #[error("stored GC barrier is null")]
    NullBarrier,
    #[error(transparent)]
    Keyspace(Arc<KeyspaceError>),
    #[error(transparent)]
    SafePoint(Arc<SafePointReadError>),
    #[error(transparent)]
    Batch(Arc<GCSafePointBatchError>),
    #[error(transparent)]
    Endpoint(Arc<EndpointError>),
    #[error(transparent)]
    Transaction(Arc<GCStateTxnError<Infallible>>),
    #[error("context canceled")]
    Cancelled,
    #[error("rpc error: code = Unavailable desc = not leader")]
    NotLeader,
    #[error(
        "[PD:gc:ErrGCOnInvalidKeyspace]trying to manage GC in keyspace {name} (id: {id}) where keyspace level GC is not enabled"
    )]
    InvalidKeyspace { name: String, id: u32 },
}
type Pair = (u64, u64);
type Flight = watch::Receiver<Option<Result<(), GCReadError>>>;
pub(crate) struct Generation {
    pub(crate) id: u64,
    pub(crate) cancel: CancellationToken,
    pub(crate) work: TaskTracker,
    pub(crate) flights: Mutex<BTreeMap<u32, Flight>>,
    pub(crate) discovery: Mutex<Option<Arc<crate::EnabledKeyspaceIndex>>>,
}
impl Generation {
    fn retire(&self) {
        let _claims = self.flights.lock().unwrap();
        self.cancel.cancel();
        self.work.close();
    }
}
pub(crate) struct Shared {
    pub(crate) active: AtomicU64,
    pub(crate) cache: RwLock<BTreeMap<u32, Pair>>,
    pub(crate) gate: AsyncRwLock<()>,
    pub(crate) barrier_metrics: Arc<crate::gc_metrics::BarrierMetrics>,
    pub(crate) metrics_binding: std::sync::OnceLock<crate::GCMetrics>,
}
/// Host-owned role lifetime. Dropping fences and cancels its pending I/O; shutdown also joins it.
pub struct GCLeadership {
    generation: Arc<Generation>,
    pub(crate) shared: Arc<Shared>,
}
impl GCLeadership {
    pub fn retire(&self) {
        if self
            .shared
            .active
            .compare_exchange(self.generation.id, 0, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            self.shared.retire_gc_metrics();
        }
        self.generation.retire();
    }
    pub async fn shutdown(self) {
        self.retire();
        self.generation.work.wait().await;
        let _guard = self.shared.gate.write().await;
        if self.shared.active.load(Ordering::Acquire) == 0 {
            self.shared.cache.write().unwrap().clear();
        }
    }
}
impl Drop for GCLeadership {
    fn drop(&mut self) {
        self.retire();
    }
}
/// Shared reads use only the supplied host runtime and tracked, cancellable generation tasks.
pub struct GCStateManager {
    pub(crate) all_states:
        [astersql_pd_syncutil::OrderedSingleFlight<BTreeMap<u32, GCState>, GCReadError>; 2],
    pub(crate) storage: StorageEndpoint,
    pub(crate) paths: ClusterPaths,
    pub(crate) keyspaces: Arc<KeyspaceManager>,
    pub(crate) host: Handle,
    pub(crate) shared: Arc<Shared>,
    generation: Mutex<Option<Arc<Generation>>>,
    pub(crate) lifecycle: tokio::sync::Mutex<()>,
    next: AtomicU64,
    pub(crate) diagnostic_time:
        Arc<dyn Fn(astersql_pd_storage_domain::BarrierTime) -> String + Send + Sync>,
}
impl Drop for GCStateManager {
    fn drop(&mut self) {
        self.shared.active.store(0, Ordering::Release);
        self.shared.retire_gc_metrics();
        if let Some(g) = self.generation.get_mut().unwrap().take() {
            g.retire();
        }
    }
}
impl GCStateManager {
    pub fn new(
        storage: StorageEndpoint,
        paths: ClusterPaths,
        keyspaces: Arc<KeyspaceManager>,
        host: Handle,
    ) -> Self {
        let barrier_metrics = Arc::new(crate::gc_metrics::BarrierMetrics::default());
        let observer = Arc::downgrade(&barrier_metrics);
        keyspaces.set_gc_barrier_invalidator(Some(Arc::new(move |id| {
            if let Some(metrics) = observer.upgrade() {
                metrics.invalidate_keyspace_metrics(id);
            }
        })));
        Self {
            all_states: std::array::from_fn(|_| Default::default()),
            storage,
            paths,
            keyspaces,
            host,
            shared: Arc::new(Shared {
                active: AtomicU64::new(0),
                cache: RwLock::new(BTreeMap::new()),
                gate: AsyncRwLock::new(()),
                barrier_metrics,
                metrics_binding: std::sync::OnceLock::new(),
            }),
            generation: Mutex::new(None),
            lifecycle: tokio::sync::Mutex::new(()),
            next: AtomicU64::new(1),
            diagnostic_time: Arc::new(crate::txn_safe_point::utc_time),
        }
    }
    pub async fn on_node_becomes_leader(&self) -> GCLeadership {
        self.on_node_becomes_leader_with_cancel(&CancellationToken::new())
            .await
    }
    /// Bind the generation to the actual host term without spawning a private cancellation worker.
    pub async fn on_node_becomes_leader_with_cancel(
        &self,
        host: &CancellationToken,
    ) -> GCLeadership {
        let _reset = self.lifecycle.lock().await;
        self.shared.active.store(0, Ordering::Release);
        self.shared.retire_gc_metrics();
        let old = self.generation.lock().unwrap().take();
        if let Some(old) = old {
            old.retire();
            old.work.wait().await;
        }
        let _guard = self.shared.gate.write().await;
        self.shared.cache.write().unwrap().clear();
        let generation = Arc::new(Generation {
            id: self.next.fetch_add(1, Ordering::Relaxed),
            cancel: host.child_token(),
            work: TaskTracker::new(),
            flights: Mutex::new(BTreeMap::new()),
            discovery: Mutex::new(None),
        });
        *self.generation.lock().unwrap() = Some(generation.clone());
        self.shared.active.store(generation.id, Ordering::Release);
        self.shared.activate_gc_metrics();
        GCLeadership {
            generation,
            shared: self.shared.clone(),
        }
    }
    pub(crate) async fn redirect(&self, id: u32, user: bool) -> Result<u32, GCReadError> {
        Ok(self.redirect_named(id, user).await?.0)
    }
    pub(crate) async fn redirect_named(
        &self,
        id: u32,
        user: bool,
    ) -> Result<(u32, String), GCReadError> {
        if id & !VALID_KEYSPACE_ID_MASK != 0 {
            return Ok((NULL_KEYSPACE_ID, String::new()));
        }
        let meta = self
            .keyspaces
            .load_by_id(id)
            .await
            .map_err(|e| GCReadError::Keyspace(Arc::new(e)))?;
        if meta
            .config
            .get("gc_management_type")
            .is_none_or(|v| v != "keyspace_level")
        {
            if user {
                return Ok((NULL_KEYSPACE_ID, String::new()));
            }
            return Err(GCReadError::InvalidKeyspace {
                name: meta.name,
                id,
            });
        }
        Ok((id, meta.name))
    }
    pub(crate) fn generation(&self) -> Option<Arc<Generation>> {
        self.generation
            .lock()
            .unwrap()
            .as_ref()
            .filter(|g| self.shared.active.load(Ordering::Acquire) == g.id)
            .cloned()
    }
    pub(crate) fn cached(&self, g: &Generation, id: u32) -> Result<Option<Pair>, GCReadError> {
        if self.shared.active.load(Ordering::Acquire) != g.id || g.cancel.is_cancelled() {
            return Err(GCReadError::NotLeader);
        }
        let value = self.shared.cache.read().unwrap().get(&id).copied();
        if self.shared.active.load(Ordering::Acquire) != g.id || g.cancel.is_cancelled() {
            return Err(GCReadError::NotLeader);
        }
        Ok(value)
    }
    pub async fn get_gc_state(&self, id: u32, exclude: bool) -> Result<GCState, GCReadError> {
        self.get_gc_state_with_cancel(id, exclude, &CancellationToken::new())
            .await
    }
    pub async fn get_gc_state_with_cancel(
        &self,
        id: u32,
        exclude: bool,
        cancel: &CancellationToken,
    ) -> Result<GCState, GCReadError> {
        let id = self.redirect(id, true).await?;
        self.read_gc_state_scope(id, exclude, cancel).await
    }
    pub(crate) async fn read_gc_state_scope(
        &self,
        id: u32,
        exclude: bool,
        cancel: &CancellationToken,
    ) -> Result<GCState, GCReadError> {
        if exclude && let Some(g) = self.generation() {
            if let Some((txn, gc)) = self.cached(&g, id)? {
                self.shared.cache_access_metrics(0);
                return Ok(GCState::pair(id, txn, gc));
            }
            return self.shared_load(&g, id, cancel).await;
        }
        let _guard = tokio::select! {biased;_=cancel.cancelled()=>return Err(GCReadError::Cancelled),guard=self.shared.gate.read()=>guard};
        self.shared.cache_access_metrics(2);
        let (state, _) = self.read_transaction(id, exclude, false, cancel).await?;
        if exclude {
            self.shared
                .cache
                .write()
                .unwrap()
                .insert(id, (state.txn_safe_point, state.gc_safe_point));
        }
        Ok(state)
    }
    async fn read_transaction(
        &self,
        id: u32,
        exclude: bool,
        global: bool,
        cancel: &CancellationToken,
    ) -> Result<(GCState, Vec<GlobalGCBarrier>), GCReadError> {
        tokio::select! {biased;_=cancel.cancelled()=>Err(GCReadError::Cancelled),result=async {self.prepare_read(id,exclude,global).await?.commit().await}=>result}
    }
    pub(crate) async fn prepare_read(
        &self,
        id: u32,
        exclude: bool,
        global: bool,
    ) -> Result<PreparedGCRead<'_>, GCReadError> {
        let txn = self
            .storage
            .begin_gc_state_transaction(&self.paths)
            .await
            .map_err(|e| GCReadError::Transaction(Arc::new(e)))?;
        let t = self
            .storage
            .load_txn_safe_point(&self.paths, id)
            .await
            .map_err(|e| GCReadError::SafePoint(Arc::new(e)))?;
        let g = self
            .storage
            .load_gc_safe_point(&self.paths, id)
            .await
            .map_err(|e| GCReadError::SafePoint(Arc::new(e)))?;
        let mut state = GCState::pair(id, t, g);
        if !exclude {
            state.gc_barriers = self
                .storage
                .load_all_gc_barriers(&self.paths, id)
                .await
                .map_err(|e| GCReadError::Endpoint(Arc::new(e)))?
                .into_iter()
                .flatten()
                .filter(|b| b.barrier_id != "gc_worker")
                .collect();
        }
        let globals = if global {
            self.storage
                .load_all_global_gc_barriers(&self.paths)
                .await
                .map_err(|e| GCReadError::Endpoint(Arc::new(e)))?
                .into_iter()
                .flatten()
                .collect()
        } else {
            vec![]
        };
        Ok(PreparedGCRead {
            txn,
            state,
            globals,
        })
    }

    /// Load the global list for the all-keyspace observation API, without reading a scope.
    pub async fn load_all_global_gc_barriers(&self) -> Result<Vec<GlobalGCBarrier>, GCReadError> {
        self.storage
            .load_all_global_gc_barriers(&self.paths)
            .await
            .map(|v| v.into_iter().flatten().collect())
            .map_err(|e| GCReadError::Endpoint(Arc::new(e)))
    }
    pub async fn get_gc_state_with_global_barriers(
        &self,
        id: u32,
        exclude: bool,
    ) -> Result<(GCState, Vec<GlobalGCBarrier>), GCReadError> {
        let id = self.redirect(id, true).await?;
        let _guard = self.shared.gate.read().await;
        let result = self
            .read_transaction(id, exclude, true, &CancellationToken::new())
            .await?;
        if exclude {
            self.shared
                .cache
                .write()
                .unwrap()
                .insert(id, (result.0.txn_safe_point, result.0.gc_safe_point));
        }
        Ok(result)
    }
    pub async fn compatible_load_gc_safe_point(&self, id: u32) -> Result<u64, GCReadError> {
        let id = self.redirect(id, false).await?;
        if let Some(g) = self.generation()
            && let Some((_, gc)) = self.cached(&g, id)?
        {
            return Ok(gc);
        }
        self.storage
            .load_gc_safe_point(&self.paths, id)
            .await
            .map_err(|e| GCReadError::SafePoint(Arc::new(e)))
    }
    async fn shared_load(
        &self,
        g: &Arc<Generation>,
        id: u32,
        cancel: &CancellationToken,
    ) -> Result<GCState, GCReadError> {
        for _ in 0..2 {
            let mut flight = {
                let mut flights = g.flights.lock().unwrap();
                if let Some((t, s)) = self.cached(g, id)? {
                    self.shared.cache_access_metrics(1);
                    return Ok(GCState::pair(id, t, s));
                }
                self.shared.cache_access_metrics(2);
                if let Some(f) = flights.get(&id) {
                    f.clone()
                } else {
                    let (sender, receiver) = watch::channel(None);
                    flights.insert(id, receiver.clone());
                    let gen_copy = g.clone();
                    let shared = self.shared.clone();
                    let storage = self.storage.clone();
                    let paths = self.paths.clone();
                    let claims = crate::gc_warmup::Claims {
                        generation: gen_copy.clone(),
                        shared: shared.clone(),
                        owned: vec![(id, sender)],
                    };
                    g.work.spawn_on(async move {
                        let token=gen_copy.cancel.clone();
                        let _guard=tokio::select!{biased;_=token.cancelled()=>return,guard=shared.gate.read()=>guard};
                        let _=crate::gc_warmup::load_claimed_gc_batch(storage,paths,claims,&token).await;
                    },&self.host);
                    receiver
                }
            };
            loop {
                if let Some(result) = flight.borrow_and_update().clone() {
                    result?;
                    break;
                }
                tokio::select! {biased;_=cancel.cancelled()=>return Err(GCReadError::Cancelled),_=g.cancel.cancelled()=>return Err(GCReadError::NotLeader),result=flight.changed()=>{if result.is_err(){return Err(GCReadError::NotLeader);}}}
            }
            if cancel.is_cancelled() {
                return Err(GCReadError::Cancelled);
            }
            if let Some((t, s)) = self.cached(g, id)? {
                return Ok(GCState::pair(id, t, s));
            }
        }
        let _guard = tokio::select! {biased;_=cancel.cancelled()=>return Err(GCReadError::Cancelled),_=g.cancel.cancelled()=>return Err(GCReadError::NotLeader),guard=self.shared.gate.read()=>guard};
        if self.shared.active.load(Ordering::Acquire) != g.id || g.cancel.is_cancelled() {
            return Err(GCReadError::NotLeader);
        }
        let ids = [id];
        let rows = tokio::select! {biased;
            _=cancel.cancelled()=>return Err(GCReadError::Cancelled),
            _=g.cancel.cancelled()=>return Err(GCReadError::NotLeader),
            result=self.storage.load_gc_safe_point_pairs(&self.paths,&ids,cancel)=>result.map_err(|e|GCReadError::Batch(Arc::new(e)))?,
        };
        if self.shared.active.load(Ordering::Acquire) != g.id || g.cancel.is_cancelled() {
            return Err(GCReadError::NotLeader);
        }
        let row = rows.into_iter().next().unwrap();
        if let Some(e) = row.err {
            return Err(GCReadError::SafePoint(Arc::new(e)));
        }
        Ok(GCState::pair(id, row.txn_safe_point, row.gc_safe_point))
    }
}

pub(crate) struct PreparedGCRead<'a> {
    txn: astersql_pd_storage_domain::PreparedGCStateTxn<'a>,
    state: GCState,
    globals: Vec<GlobalGCBarrier>,
}
impl PreparedGCRead<'_> {
    pub(crate) async fn commit(self) -> Result<(GCState, Vec<GlobalGCBarrier>), GCReadError> {
        self.txn
            .commit()
            .await
            .map_err(|e| GCReadError::Transaction(Arc::new(e)))?;
        Ok((self.state, self.globals))
    }
}

impl GCStateManager {
    /// Supply the host's diagnostic timezone rendering; storage and expiry comparisons remain absolute.
    pub fn with_diagnostic_time_formatter(
        mut self,
        format: Arc<dyn Fn(astersql_pd_storage_domain::BarrierTime) -> String + Send + Sync>,
    ) -> Self {
        self.diagnostic_time = format;
        self
    }
    pub(crate) async fn mutation_lock(&self) -> tokio::sync::RwLockWriteGuard<'_, ()> {
        self.shared.gate.write().await
    }
    pub(crate) fn publish_points(&self, id: u32, txn: u64, gc: u64) {
        if self
            .generation
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|g| g.cancel.is_cancelled())
        {
            return;
        }
        self.shared.cache.write().unwrap().insert(id, (txn, gc));
    }
    pub(crate) fn invalidate_points(&self, id: u32) {
        self.shared.cache.write().unwrap().remove(&id);
    }
}
impl From<SafePointReadError> for GCReadError {
    fn from(error: SafePointReadError) -> Self {
        Self::SafePoint(Arc::new(error))
    }
}
impl From<EndpointError> for GCReadError {
    fn from(error: EndpointError) -> Self {
        Self::Endpoint(Arc::new(error))
    }
}
impl From<GCStateTxnError<Infallible>> for GCReadError {
    fn from(error: GCStateTxnError<Infallible>) -> Self {
        Self::Transaction(Arc::new(error))
    }
}
