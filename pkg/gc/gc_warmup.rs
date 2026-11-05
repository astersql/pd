// Copyright 2026 AsterSQL.
use crate::{GCReadError, GCStateManager, gc_read::Generation};
use astersql_pd_storage_domain::MAX_GC_SAFE_POINT_BATCH_SIZE;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, atomic::Ordering},
};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
/// A terminal result remains valid even if a later writer invalidates the cache.
#[derive(Clone, Debug)]
pub struct GCLoadWaiter {
    receiver: watch::Receiver<Option<Result<(), GCReadError>>>,
}
impl GCLoadWaiter {
    pub fn try_result(&self) -> Option<Result<(), GCReadError>> {
        self.receiver.borrow().clone()
    }
    pub async fn wait(&mut self, cancel: &CancellationToken) -> Result<(), GCReadError> {
        loop {
            if let Some(result) = self.receiver.borrow_and_update().clone() {
                return result;
            }
            tokio::select! {biased;_=cancel.cancelled()=>return Err(GCReadError::Cancelled),result=self.receiver.changed()=>if result.is_err(){return Err(GCReadError::NotLeader);}}
        }
    }
}
#[derive(Default, Debug)]
pub struct GCLoadBatchResult {
    pub completed: Vec<u32>,
    pub failed: BTreeMap<u32, GCReadError>,
    pub joined: BTreeMap<u32, GCLoadWaiter>,
}
pub(crate) struct Claims {
    pub(crate) generation: Arc<Generation>,
    pub(crate) shared: Arc<crate::gc_read::Shared>,
    pub(crate) owned: Vec<(u32, watch::Sender<Option<Result<(), GCReadError>>>)>,
}
impl Drop for Claims {
    fn drop(&mut self) {
        let mut flights = self.generation.flights.lock().unwrap();
        for (id, sender) in self.owned.drain(..) {
            flights.remove(&id);
            let error = if self.generation.cancel.is_cancelled()
                || self.shared.active.load(Ordering::Acquire) != self.generation.id
            {
                GCReadError::NotLeader
            } else {
                GCReadError::Cancelled
            };
            let _ = sender.send(Some(Err(error)));
        }
    }
}
impl GCStateManager {
    /// Assemble at most 60 owned misses under the existing generation flight lock.
    /// Cached scopes and joins consume no storage slots. The supplied iterator must never block.
    pub async fn warmup_batch(
        &self,
        ids: impl Iterator<Item = u32> + Send,
        cancel: &CancellationToken,
    ) -> Result<GCLoadBatchResult, GCReadError> {
        let generation = self.generation().ok_or(GCReadError::NotLeader)?;
        self.warmup_batch_in_generation(generation, ids, cancel)
            .await
    }
    pub(crate) async fn warmup_batch_in_generation(
        &self,
        generation: Arc<Generation>,
        mut ids: impl Iterator<Item = u32> + Send,
        cancel: &CancellationToken,
    ) -> Result<GCLoadBatchResult, GCReadError> {
        let _guard = tokio::select! {biased;_=cancel.cancelled()=>return Err(GCReadError::Cancelled),_=generation.cancel.cancelled()=>return Err(GCReadError::NotLeader),guard=self.shared.gate.read()=>guard};
        let mut result = GCLoadBatchResult::default();
        let mut claims = Claims {
            generation: generation.clone(),
            shared: self.shared.clone(),
            owned: vec![],
        };
        {
            let mut flights = generation.flights.lock().unwrap();
            let mut seen = BTreeSet::new();
            while claims.owned.len() < MAX_GC_SAFE_POINT_BATCH_SIZE {
                if cancel.is_cancelled() {
                    break;
                }
                if generation.cancel.is_cancelled()
                    || self.shared.active.load(Ordering::Acquire) != generation.id
                {
                    break;
                }
                let Some(id) = ids.next() else {
                    break;
                };
                if !seen.insert(id) {
                    continue;
                }
                if self.cached(&generation, id)?.is_some() {
                    result.completed.push(id);
                } else if let Some(receiver) = flights.get(&id) {
                    result.joined.insert(
                        id,
                        GCLoadWaiter {
                            receiver: receiver.clone(),
                        },
                    );
                } else {
                    let (sender, receiver) = watch::channel(None);
                    flights.insert(id, receiver);
                    claims.owned.push((id, sender));
                }
            }
        }
        if cancel.is_cancelled() {
            return Err(GCReadError::Cancelled);
        }
        if generation.cancel.is_cancelled()
            || self.shared.active.load(Ordering::Acquire) != generation.id
        {
            return Err(GCReadError::NotLeader);
        }
        if claims.owned.is_empty() {
            return Ok(result);
        }
        let loaded =
            load_claimed_gc_batch(self.storage.clone(), self.paths.clone(), claims, cancel).await;
        result.completed.extend(loaded.completed);
        result.failed.extend(loaded.failed);
        Ok(result)
    }
}

use crate::{EnabledKeyspace, EnabledKeyspaceIndex, EnabledSnapshot};
use astersql_pd_keypath::NULL_KEYSPACE_ID;
use std::{
    collections::VecDeque,
    sync::{Mutex, atomic::AtomicBool},
    time::{Duration, Instant},
};
#[derive(Default)]
struct Scope {
    completed: bool,
    queued: bool,
    running: bool,
    joined: Option<GCLoadWaiter>,
    error: Option<GCReadError>,
}
struct WarmupState {
    scopes: BTreeMap<u32, Scope>,
    candidates: VecDeque<u32>,
    null_pending: bool,
    reconciled: bool,
    remaining: usize,
    next_joined: Option<Instant>,
}
struct WarmupInput {
    accepting: bool,
    pages: VecDeque<Vec<EnabledKeyspace>>,
    snapshot: Option<EnabledSnapshot>,
}
/// A bounded initial discovery campaign; the host owns run, cancellation and join.
pub struct GCWarmup {
    manager: Arc<GCStateManager>,
    index: Option<Arc<EnabledKeyspaceIndex>>,
    concurrency: usize,
    input: Mutex<WarmupInput>,
    state: Mutex<WarmupState>,
    changed: watch::Sender<u64>,
    started: AtomicBool,
    finished: AtomicBool,
    stopped: AtomicBool,
}
impl GCWarmup {
    pub fn new(
        manager: Arc<GCStateManager>,
        index: Option<Arc<EnabledKeyspaceIndex>>,
        concurrency: usize,
    ) -> Result<Self, GCReadError> {
        if concurrency == 0 {
            return Err(GCReadError::InvalidArgument {
                argument: "gc-state-cache-warmup-concurrency",
                value: "0".into(),
            });
        }
        Ok(Self {
            manager,
            index,
            concurrency,
            input: Mutex::new(WarmupInput {
                accepting: true,
                pages: VecDeque::new(),
                snapshot: None,
            }),
            state: Mutex::new(WarmupState {
                scopes: BTreeMap::from([(NULL_KEYSPACE_ID, Scope::default())]),
                candidates: VecDeque::new(),
                null_pending: true,
                reconciled: false,
                remaining: 1,
                next_joined: None,
            }),
            changed: watch::channel(0).0,
            started: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
        })
    }
    fn signal(&self) {
        self.changed.send_modify(|v| *v = v.wrapping_add(1));
    }
    pub fn on_page(&self, entries: Vec<EnabledKeyspace>) {
        let mut input = self.input.lock().unwrap();
        if !input.accepting {
            return;
        }
        if input.pages.len() < 16 {
            input.pages.push_back(entries);
        }
        drop(input);
        self.signal();
    }
    pub fn on_initial_snapshot(&self, snapshot: EnabledSnapshot) {
        let mut input = self.input.lock().unwrap();
        if !input.accepting {
            return;
        }
        if input.snapshot.is_none() {
            input.snapshot = Some(snapshot);
        }
        drop(input);
        self.signal();
    }
    pub async fn run(self: &Arc<Self>, cancel: &CancellationToken) -> Result<(), GCReadError> {
        if self.started.swap(true, Ordering::AcqRel) {
            return Err(GCReadError::InvalidArgument {
                argument: "warmup campaign",
                value: "already started".into(),
            });
        }
        let _cleanup = WarmupCleanup(self);
        let generation = self.manager.generation().ok_or(GCReadError::NotLeader)?;
        let child = cancel.child_token();
        let mut workers = tokio::task::JoinSet::new();
        for _ in 0..self.concurrency {
            let warmup = self.clone();
            let token = child.clone();
            let generation = generation.clone();
            workers.spawn(async move { warmup.worker(generation, &token).await });
        }
        let mut failure = None;
        while let Some(result) = workers.join_next().await {
            let result = result.unwrap_or(Err(GCReadError::Cancelled));
            if let Err(error) = result {
                if failure.is_none() {
                    failure = Some(error);
                    child.cancel();
                }
            }
        }
        if cancel.is_cancelled() {
            Ok(())
        } else {
            failure.map_or(Ok(()), Err)
        }
    }
    pub async fn wait_finished(&self, cancel: &CancellationToken) -> Result<(), GCReadError> {
        let mut changed = self.changed.subscribe();
        loop {
            if self.finished.load(Ordering::Acquire) {
                return Ok(());
            }
            if self.stopped.load(Ordering::Acquire) {
                return Err(GCReadError::Cancelled);
            }
            tokio::select! {biased;_=cancel.cancelled()=>return Err(GCReadError::Cancelled),_=changed.changed()=>{}}
        }
    }
}
struct WarmupCleanup<'a>(&'a GCWarmup);
impl Drop for WarmupCleanup<'_> {
    fn drop(&mut self) {
        let mut input = self.0.input.lock().unwrap();
        input.accepting = false;
        input.pages.clear();
        input.snapshot = None;
        drop(input);
        let mut state = self.0.state.lock().unwrap();
        state.scopes.clear();
        state.candidates.clear();
        drop(state);
        self.0.stopped.store(true, Ordering::Release);
        self.0.signal();
    }
}
enum WarmupAction {
    Null,
    Candidates,
    Wait(Option<Instant>),
    Done,
}
impl GCWarmup {
    fn add(state: &mut WarmupState, id: u32) {
        if !state.scopes.contains_key(&id) {
            state.scopes.insert(
                id,
                Scope {
                    queued: true,
                    ..Default::default()
                },
            );
            state.candidates.push_back(id);
            state.remaining += 1;
        }
    }
    fn ingest(&self, state: &mut WarmupState) {
        if state.reconciled {
            return;
        }
        let mut input = self.input.lock().unwrap();
        while let Some(page) = input.pages.pop_front() {
            for entry in page {
                if entry.gc_management_type == "keyspace_level" {
                    Self::add(state, entry.id);
                }
            }
        }
    }
    fn reconcile(&self, state: &mut WarmupState) {
        if state.reconciled {
            return;
        }
        let snapshot = self.input.lock().unwrap().snapshot.take();
        if let Some(snapshot) = snapshot {
            let mut targets = BTreeSet::from([NULL_KEYSPACE_ID]);
            for entry in snapshot.entries.values() {
                if entry.gc_management_type == "keyspace_level" {
                    Self::add(state, entry.id);
                    targets.insert(entry.id);
                }
            }
            state.scopes.retain(|id, _| targets.contains(id));
            state.candidates.retain(|id| targets.contains(id));
            state.remaining = state.scopes.values().filter(|s| !s.completed).count();
            state.reconciled = true;
        }
    }
    fn record(state: &mut WarmupState, id: u32, result: Result<(), GCReadError>) {
        if let Some(scope) = state.scopes.get_mut(&id) {
            scope.running = false;
            scope.joined = None;
            match result {
                Ok(()) => {
                    if !scope.completed {
                        scope.completed = true;
                        state.remaining -= 1;
                    }
                    scope.error = None;
                }
                Err(error) => scope.error = Some(error),
            }
        }
    }
    fn refresh_joined(state: &mut WarmupState, now: Instant) {
        if state.next_joined.is_some_and(|deadline| deadline > now) {
            return;
        }
        let ready: Vec<_> = state
            .scopes
            .iter()
            .filter_map(|(id, scope)| {
                scope
                    .joined
                    .as_ref()
                    .and_then(|flight| flight.try_result())
                    .map(|r| (*id, r))
            })
            .collect();
        for (id, result) in ready {
            Self::record(state, id, result);
        }
        state.next_joined = state
            .scopes
            .values()
            .any(|s| s.joined.is_some())
            .then_some(now + Duration::from_millis(100));
    }
    fn next_candidate(&self) -> Option<u32> {
        let mut state = self.state.lock().unwrap();
        loop {
            if state.candidates.is_empty() {
                self.ingest(&mut state);
            }
            let id = state.candidates.pop_front()?;
            if let Some(scope) = state.scopes.get_mut(&id)
                && scope.queued
            {
                scope.queued = false;
                scope.running = true;
                return Some(id);
            }
        }
    }
    async fn worker(
        &self,
        generation: Arc<Generation>,
        cancel: &CancellationToken,
    ) -> Result<(), GCReadError> {
        let mut changed = self.changed.subscribe();
        loop {
            if cancel.is_cancelled() {
                return Ok(());
            }
            if generation.cancel.is_cancelled() {
                return Err(GCReadError::NotLeader);
            }
            let action = {
                let mut state = self.state.lock().unwrap();
                if !state.null_pending {
                    self.ingest(&mut state);
                    self.reconcile(&mut state);
                    Self::refresh_joined(&mut state, Instant::now());
                }
                if self.finished.load(Ordering::Acquire) {
                    WarmupAction::Done
                } else if state.reconciled && state.remaining == 0 {
                    self.finished.store(true, Ordering::Release);
                    self.signal();
                    WarmupAction::Done
                } else if state.null_pending {
                    state.null_pending = false;
                    state.scopes.get_mut(&NULL_KEYSPACE_ID).unwrap().running = true;
                    WarmupAction::Null
                } else if !state.candidates.is_empty() {
                    WarmupAction::Candidates
                } else {
                    WarmupAction::Wait(state.next_joined)
                }
            };
            let result = match action {
                WarmupAction::Done => return Ok(()),
                WarmupAction::Null => {
                    self.manager
                        .warmup_batch_in_generation(
                            generation.clone(),
                            [NULL_KEYSPACE_ID].into_iter(),
                            cancel,
                        )
                        .await?
                }
                WarmupAction::Candidates => {
                    self.manager
                        .warmup_batch_in_generation(
                            generation.clone(),
                            std::iter::from_fn(|| self.next_candidate()),
                            cancel,
                        )
                        .await?
                }
                WarmupAction::Wait(deadline) => {
                    tokio::select! {biased;_=cancel.cancelled()=>return Ok(()),_=generation.cancel.cancelled()=>return Err(GCReadError::NotLeader),_=changed.changed()=>{},_=async{if let Some(deadline)=deadline{tokio::time::sleep_until(deadline.into()).await}else{std::future::pending::<()>().await}}=>{}}
                    continue;
                }
            };
            {
                let mut state = self.state.lock().unwrap();
                for id in result.completed {
                    Self::record(&mut state, id, Ok(()));
                }
                for (id, error) in result.failed {
                    Self::record(&mut state, id, Err(error));
                }
                for (id, flight) in result.joined {
                    if let Some(scope) = state.scopes.get_mut(&id) {
                        scope.running = false;
                        scope.joined = Some(flight);
                        if state.next_joined.is_none() {
                            state.next_joined = Some(Instant::now() + Duration::from_millis(100));
                        }
                    }
                }
            }
            self.signal();
        }
    }
}

pub(crate) async fn load_claimed_gc_batch(
    storage: astersql_pd_storage_domain::StorageEndpoint,
    paths: astersql_pd_keypath::ClusterPaths,
    mut claims: Claims,
    cancel: &CancellationToken,
) -> GCLoadBatchResult {
    let generation = claims.generation.clone();
    let shared = claims.shared.clone();
    let mut result = GCLoadBatchResult::default();
    let ids: Vec<_> = claims.owned.iter().map(|(id, _)| *id).collect();
    let values = tokio::select! {biased;_=cancel.cancelled()=>Err(GCReadError::Cancelled),_=generation.cancel.cancelled()=>Err(GCReadError::NotLeader),result=storage.load_gc_safe_point_pairs(&paths,&ids,cancel)=>result.map_err(|e|GCReadError::Batch(Arc::new(e)))};
    let mut flights = generation.flights.lock().unwrap();
    let values = if generation.cancel.is_cancelled()
        || shared.active.load(Ordering::Acquire) != generation.id
    {
        Err(GCReadError::NotLeader)
    } else if cancel.is_cancelled() {
        Err(GCReadError::Cancelled)
    } else {
        values
    };
    let mut values = values.map(Vec::into_iter);
    for (id, sender) in claims.owned.drain(..) {
        let outcome = match &mut values {
            Err(e) => Err(e.clone()),
            Ok(values) => {
                let row = values
                    .next()
                    .expect("storage returns one row per requested scope");
                if let Some(error) = row.err {
                    Err(GCReadError::SafePoint(Arc::new(error)))
                } else {
                    shared
                        .cache
                        .write()
                        .unwrap()
                        .insert(id, (row.txn_safe_point, row.gc_safe_point));
                    Ok(())
                }
            }
        };
        flights.remove(&id);
        let _ = sender.send(Some(outcome.clone()));
        match outcome {
            Ok(()) => result.completed.push(id),
            Err(error) => {
                result.failed.insert(id, error);
            }
        }
    }
    result
}
