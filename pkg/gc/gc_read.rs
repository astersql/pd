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
    fn pair(id: u32, txn: u64, gc: u64) -> Self {
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
struct Generation {
    id: u64,
    cancel: CancellationToken,
    work: TaskTracker,
    flights: Mutex<BTreeMap<u32, Flight>>,
}
impl Generation {
    fn retire(&self) {
        let _claims = self.flights.lock().unwrap();
        self.cancel.cancel();
        self.work.close();
    }
}
struct Shared {
    active: AtomicU64,
    cache: RwLock<BTreeMap<u32, Pair>>,
    gate: AsyncRwLock<()>,
}
/// Host-owned role lifetime. Dropping fences and cancels its pending I/O; shutdown also joins it.
pub struct GCLeadership {
    generation: Arc<Generation>,
    shared: Arc<Shared>,
}
impl GCLeadership {
    pub fn retire(&self) {
        let _ = self.shared.active.compare_exchange(
            self.generation.id,
            0,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
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
    pub(crate) storage: StorageEndpoint,
    pub(crate) paths: ClusterPaths,
    pub(crate) keyspaces: Arc<KeyspaceManager>,
    host: Handle,
    shared: Arc<Shared>,
    generation: Mutex<Option<Arc<Generation>>>,
    lifecycle: tokio::sync::Mutex<()>,
    next: AtomicU64,
}
impl Drop for GCStateManager {
    fn drop(&mut self) {
        self.shared.active.store(0, Ordering::Release);
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
        Self {
            storage,
            paths,
            keyspaces,
            host,
            shared: Arc::new(Shared {
                active: AtomicU64::new(0),
                cache: RwLock::new(BTreeMap::new()),
                gate: AsyncRwLock::new(()),
            }),
            generation: Mutex::new(None),
            lifecycle: tokio::sync::Mutex::new(()),
            next: AtomicU64::new(1),
        }
    }
    pub async fn on_node_becomes_leader(&self) -> GCLeadership {
        let _reset = self.lifecycle.lock().await;
        self.shared.active.store(0, Ordering::Release);
        let old = self.generation.lock().unwrap().take();
        if let Some(old) = old {
            old.retire();
            old.work.wait().await;
        }
        let _guard = self.shared.gate.write().await;
        self.shared.cache.write().unwrap().clear();
        let generation = Arc::new(Generation {
            id: self.next.fetch_add(1, Ordering::Relaxed),
            cancel: CancellationToken::new(),
            work: TaskTracker::new(),
            flights: Mutex::new(BTreeMap::new()),
        });
        *self.generation.lock().unwrap() = Some(generation.clone());
        self.shared.active.store(generation.id, Ordering::Release);
        GCLeadership {
            generation,
            shared: self.shared.clone(),
        }
    }
    pub(crate) async fn redirect(&self, id: u32, user: bool) -> Result<u32, GCReadError> {
        if id & !VALID_KEYSPACE_ID_MASK != 0 {
            return Ok(NULL_KEYSPACE_ID);
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
                return Ok(NULL_KEYSPACE_ID);
            }
            return Err(GCReadError::InvalidKeyspace {
                name: meta.name,
                id,
            });
        }
        Ok(id)
    }
    fn generation(&self) -> Option<Arc<Generation>> {
        self.generation
            .lock()
            .unwrap()
            .as_ref()
            .filter(|g| self.shared.active.load(Ordering::Acquire) == g.id)
            .cloned()
    }
    fn cached(&self, g: &Generation, id: u32) -> Result<Option<Pair>, GCReadError> {
        if self.shared.active.load(Ordering::Acquire) != g.id {
            return Err(GCReadError::NotLeader);
        }
        let value = self.shared.cache.read().unwrap().get(&id).copied();
        if self.shared.active.load(Ordering::Acquire) != g.id {
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
        if exclude && let Some(g) = self.generation() {
            if let Some((txn, gc)) = self.cached(&g, id)? {
                return Ok(GCState::pair(id, txn, gc));
            }
            return self.shared_load(&g, id, cancel).await;
        }
        let _guard = tokio::select! {biased;_=cancel.cancelled()=>return Err(GCReadError::Cancelled),guard=self.shared.gate.read()=>guard};
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
                    return Ok(GCState::pair(id, t, s));
                }
                if let Some(f) = flights.get(&id) {
                    f.clone()
                } else {
                    let (sender, receiver) = watch::channel(None);
                    flights.insert(id, receiver.clone());
                    let gen_copy = g.clone();
                    let shared = self.shared.clone();
                    let storage = self.storage.clone();
                    let paths = self.paths.clone();
                    g.work.spawn_on(async move {
                        let result=async {
                            let _guard=tokio::select!{biased;_=gen_copy.cancel.cancelled()=>return Err(GCReadError::NotLeader),guard=shared.gate.read()=>guard};
                            if shared.active.load(Ordering::Acquire)!=gen_copy.id {return Err(GCReadError::NotLeader);}
                            let mut rows=storage.load_gc_safe_point_pairs(&paths,&[id],&gen_copy.cancel).await.map_err(|e|GCReadError::Batch(Arc::new(e)))?;
                            if shared.active.load(Ordering::Acquire)!=gen_copy.id || gen_copy.cancel.is_cancelled(){return Err(GCReadError::NotLeader);}
                            let row=rows.remove(0);if let Some(e)=row.err{return Err(GCReadError::SafePoint(Arc::new(e)));}
                            shared.cache.write().unwrap().insert(id,(row.txn_safe_point,row.gc_safe_point));Ok(())
                        }.await;
                        // Never hold the scheduling lock across I/O or manager locks.
                        let result = if shared.active.load(Ordering::Acquire) != gen_copy.id || gen_copy.cancel.is_cancelled() { Err(GCReadError::NotLeader) } else { result };
                        gen_copy.flights.lock().unwrap().remove(&id);
                        let _=sender.send(Some(result));
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
        if self.shared.active.load(Ordering::Acquire) != g.id {
            return Err(GCReadError::NotLeader);
        }
        let ids = [id];
        let rows = tokio::select! {biased;
            _=cancel.cancelled()=>return Err(GCReadError::Cancelled),
            _=g.cancel.cancelled()=>return Err(GCReadError::NotLeader),
            result=self.storage.load_gc_safe_point_pairs(&self.paths,&ids,cancel)=>result.map_err(|e|GCReadError::Batch(Arc::new(e)))?,
        };
        if self.shared.active.load(Ordering::Acquire) != g.id {
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
