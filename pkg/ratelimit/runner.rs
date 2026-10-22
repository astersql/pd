// Copyright 2026 AsterSQL.
use crate::{ConcurrencyLimiter, TaskToken};
use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::time::Instant;
use tokio::{
    sync::mpsc,
    task::{JoinHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;

type Task = Box<dyn FnOnce(CancellationToken) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;

/// Submission failed because the runner stopped or its pending queue is full/old.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RunnerError {
    #[error("runner stopped")]
    Stopped,
    #[error("max waiting tasks exceeded")]
    MaxWaitingTasksExceeded,
}
impl RunnerError {
    /// Stable Go domain error for queue admission failures.
    pub fn error_template(self) -> Option<astersql_pd_errs::ErrorTemplate> {
        match self {
            Self::Stopped => None,
            Self::MaxWaitingTasksExceeded => {
                Some(astersql_pd_errs::catalog::ERR_MAX_WAITING_TASKS_EXCEEDED)
            }
        }
    }
}

const INITIAL_CAPACITY: usize = 10_000;
#[derive(Clone, Hash, PartialEq, Eq)]
enum TaskId {
    Named(u64, String),
    Anonymous(u64),
}
struct Entry {
    submitted_at: Instant,
    task: Task,
}
struct Pending {
    order: VecDeque<TaskId>,
    entries: HashMap<TaskId, Entry>,
}
impl Pending {
    fn new() -> Self {
        Self {
            order: VecDeque::with_capacity(INITIAL_CAPACITY),
            entries: HashMap::new(),
        }
    }
    fn dispatch(&mut self, sender: &mpsc::Sender<Task>) {
        if self.order.is_empty() {
            return;
        }
        let Ok(permit) = sender.try_reserve() else {
            return;
        };
        let id = self.order.pop_front().unwrap();
        let entry = self.entries.remove(&id).unwrap();
        permit.send(entry.task);
        if self.order.is_empty() {
            // Reuse the normal ring, release oversized backlog allocations.
            if self.order.capacity() > INITIAL_CAPACITY {
                self.order = VecDeque::with_capacity(INITIAL_CAPACITY);
            }
            self.entries = HashMap::new();
        }
    }
    fn clear(&mut self) {
        self.order = VecDeque::new();
        self.entries = HashMap::new();
    }
}

struct Permit(Arc<ConcurrencyLimiter>, TaskToken);
impl Drop for Permit {
    fn drop(&mut self) {
        self.0.release_token(&self.1);
    }
}

/// Runs tasks on the caller's Tokio runtime with optional concurrency limits.
/// Stop cancels pending work and aborts/joins active futures. Unlike Go's
/// detached goroutines, owned futures and permits cannot outlive shutdown.
/// Pending tasks retain FIFO age when their (ID, name) closure is replaced.
pub struct ConcurrentRunner {
    limiter: Option<Arc<ConcurrencyLimiter>>,
    sender: Option<mpsc::Sender<Task>>,
    pending: Arc<Mutex<Pending>>,
    next_id: AtomicU64,
    max_pending_duration: Duration,
    max_pending_tasks: usize,
    cancel: CancellationToken,
    worker: Option<JoinHandle<()>>,
}
impl ConcurrentRunner {
    /// Creates a stopped runner; `None` admits tasks without a token.
    pub fn new(limiter: Option<Arc<ConcurrencyLimiter>>) -> Self {
        Self {
            limiter,
            sender: None,
            pending: Arc::new(Mutex::new(Pending::new())),
            next_id: AtomicU64::new(0),
            max_pending_duration: Duration::MAX,
            max_pending_tasks: 20_000_000,
            cancel: CancellationToken::new(),
            worker: None,
        }
    }

    /// Configures pending admission limits before starting. Retained submissions
    /// bypass age rejection, but cannot bypass the pending task count limit.
    pub fn with_queue_limits(mut self, max_age: Duration, max_tasks: usize) -> Self {
        self.max_pending_duration = max_age;
        self.max_pending_tasks = max_tasks;
        self
    }

    /// Returns queued tasks, excluding the dispatch channel and active work.
    pub fn pending_task_count(&self) -> usize {
        self.pending.lock().unwrap().order.len()
    }

    /// Starts once on the current runtime. Repeated starts while active are no-ops.
    pub fn start(&mut self, parent: &CancellationToken) {
        if self.worker.is_some() {
            return;
        }
        self.cancel = parent.child_token();
        let cancel = self.cancel.clone();
        let limiter = self.limiter.clone();
        let (sender, mut receiver) = mpsc::channel::<Task>(1);
        self.sender = Some(sender.clone());
        let pending = self.pending.clone();
        self.worker = Some(tokio::spawn(async move {
            let mut tasks = JoinSet::new();
            loop {
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => break,
                    _ = tasks.join_next(), if !tasks.is_empty() => {
                        pending.lock().unwrap().dispatch(&sender);
                    },
                    task = receiver.recv() => {
                        let Some(task) = task else { break; };
                        let permit = if let Some(limiter) = &limiter {
                            match limiter.acquire_token(&cancel).await {
                                Ok(token) => Some(Permit(limiter.clone(), token)),
                                Err(_) => break,
                            }
                        } else { None };
                        if cancel.is_cancelled() { break; }
                        let child = cancel.clone();
                        tasks.spawn(async move {
                            let _permit = permit;
                            task(child).await;
                        });
                        if limiter.is_none() {
                            pending.lock().unwrap().dispatch(&sender);
                        }
                    }
                }
            }
            receiver.close();
            pending.lock().unwrap().clear();
            drop(receiver);
            tasks.abort_all();
            while tasks.join_next().await.is_some() {}
        }));
    }

    /// Enqueues asynchronous work; submission never blocks on token acquisition.
    pub fn run_task<F, Fut>(&self, task: F) -> Result<(), RunnerError>
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.submit(
            TaskId::Anonymous(self.next_id.fetch_add(1, Ordering::Relaxed)),
            false,
            Box::new(move |cancel| Box::pin(task(cancel))),
        )
    }

    /// Enqueues keyed work. A pending duplicate replaces only the closure,
    /// retaining its original FIFO position and submission time, even at capacity.
    pub fn run_task_with_id<F, Fut>(
        &self,
        id: u64,
        name: &str,
        retained: bool,
        task: F,
    ) -> Result<(), RunnerError>
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.submit(
            TaskId::Named(id, name.to_owned()),
            retained,
            Box::new(move |cancel| Box::pin(task(cancel))),
        )
    }

    fn submit(&self, id: TaskId, retained: bool, task: Task) -> Result<(), RunnerError> {
        let sender = self.sender.as_ref().ok_or(RunnerError::Stopped)?;
        let mut pending = self.pending.lock().unwrap();
        if self.cancel.is_cancelled() || sender.is_closed() {
            return Err(RunnerError::Stopped);
        }
        let result = if let Some(entry) = pending.entries.get_mut(&id) {
            entry.task = task;
            Ok(())
        } else if !pending.order.is_empty()
            && ((!retained
                && pending.entries[&pending.order[0]].submitted_at.elapsed()
                    > self.max_pending_duration)
                || pending.order.len() >= self.max_pending_tasks)
        {
            Err(RunnerError::MaxWaitingTasksExceeded)
        } else {
            pending.order.push_back(id.clone());
            pending.entries.insert(
                id,
                Entry {
                    submitted_at: Instant::now(),
                    task,
                },
            );
            Ok(())
        };
        pending.dispatch(sender);
        result
    }

    /// Cancels work and waits until every owned task and token is released.
    pub async fn stop(&mut self) {
        self.cancel.cancel();
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.await;
        }
    }
}
impl Drop for ConcurrentRunner {
    fn drop(&mut self) {
        // The dispatcher owns the JoinSet and drains it on cancellation.
        self.cancel.cancel();
    }
}

/// Executes closures immediately with an independent background context.
#[derive(Debug, Default, Clone, Copy)]
pub struct SyncRunner;
impl SyncRunner {
    /// Synchronous runners require no background worker.
    pub fn start(&self, _parent: &CancellationToken) {}
    /// Calls the closure before returning.
    pub fn run_task(&self, task: impl FnOnce(CancellationToken)) {
        task(CancellationToken::new());
    }
    /// Synchronous runners have no work to stop.
    pub fn stop(&self) {}
}

#[cfg(test)]
#[path = "runner_test.rs"]
mod tests;
