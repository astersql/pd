// Copyright 2026 AsterSQL.
use crate::{ConcurrencyLimiter, TaskToken};
use std::{future::Future, pin::Pin, sync::Arc};
use tokio::{
    sync::mpsc,
    task::{JoinHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;

type Task = Box<dyn FnOnce(CancellationToken) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;

/// Submission requires a live runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("runner stopped")]
pub struct RunnerStopped;

struct Permit(Arc<ConcurrencyLimiter>, TaskToken);
impl Drop for Permit {
    fn drop(&mut self) {
        self.0.release_token(&self.1);
    }
}

/// Runs tasks on the caller's Tokio runtime with optional concurrency limits.
/// Stop cancels pending work and aborts/joins active futures. Unlike Go's
/// detached goroutines, owned futures and permits cannot outlive shutdown.
/// Queue age, deduplication, and metrics are separate policies.
pub struct ConcurrentRunner {
    limiter: Option<Arc<ConcurrencyLimiter>>,
    sender: Option<mpsc::UnboundedSender<Task>>,
    cancel: CancellationToken,
    worker: Option<JoinHandle<()>>,
}
impl ConcurrentRunner {
    /// Creates a stopped runner; `None` admits tasks without a token.
    pub fn new(limiter: Option<Arc<ConcurrencyLimiter>>) -> Self {
        Self {
            limiter,
            sender: None,
            cancel: CancellationToken::new(),
            worker: None,
        }
    }

    /// Starts once on the current runtime. Repeated starts while active are no-ops.
    pub fn start(&mut self, parent: &CancellationToken) {
        if self.worker.is_some() {
            return;
        }
        self.cancel = parent.child_token();
        let cancel = self.cancel.clone();
        let limiter = self.limiter.clone();
        let (sender, mut receiver) = mpsc::unbounded_channel::<Task>();
        self.sender = Some(sender);
        self.worker = Some(tokio::spawn(async move {
            let mut tasks = JoinSet::new();
            loop {
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => break,
                    _ = tasks.join_next(), if !tasks.is_empty() => {},
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
                    }
                }
            }
            receiver.close();
            drop(receiver);
            tasks.abort_all();
            while tasks.join_next().await.is_some() {}
        }));
    }

    /// Enqueues asynchronous work; submission never blocks on token acquisition.
    pub fn run_task<F, Fut>(&self, task: F) -> Result<(), RunnerStopped>
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        if self.cancel.is_cancelled() {
            return Err(RunnerStopped);
        }
        self.sender
            .as_ref()
            .ok_or(RunnerStopped)?
            .send(Box::new(move |cancel| Box::pin(task(cancel))))
            .map_err(|_| RunnerStopped)
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
