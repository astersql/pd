// Copyright 2026 AsterSQL.

use std::{
    future::Future,
    sync::{Arc, Mutex},
};

use tokio::sync::{Semaphore, watch};

struct Task<T, E> {
    result: watch::Sender<Option<Result<T, E>>>,
}

impl<T, E> Task<T, E> {
    fn new() -> Self {
        let (result, _) = watch::channel(None);
        Self { result }
    }

    async fn wait(&self) -> Result<T, E>
    where
        T: Clone,
        E: Clone,
    {
        let mut result = self.result.subscribe();
        loop {
            if let Some(result) = result.borrow().clone() {
                return result;
            }
            result
                .changed()
                .await
                .expect("an ordered single-flight task keeps its sender alive");
        }
    }

    fn finish(&self, result: Result<T, E>) {
        self.result.send_replace(Some(result));
    }
}

struct State<T, E> {
    pending: Option<Arc<Task<T, E>>>,
}

/// Coalesces calls awaiting the same not-yet-started execution and runs
/// executions in invocation order.
///
/// Unlike a keyed single-flight group, an `OrderedSingleFlight` controls one
/// operation stream. Calls that arrive before an execution starts share that
/// execution. Calls arriving after it starts form the next batch, so they
/// never observe a result from an execution that predates their invocation.
/// The caller supplies the Tokio runtime used to run each execution.
pub struct OrderedSingleFlight<T, E> {
    state: Mutex<State<T, E>>,
    token: Arc<Semaphore>,
}

impl<T, E> OrderedSingleFlight<T, E>
where
    T: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
{
    /// Creates an idle ordered single-flight stream.
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State { pending: None }),
            token: Arc::new(Semaphore::new(1)),
        }
    }

    /// Runs `operation`, or shares the result of the pending execution.
    pub async fn call<F, Fut>(&self, operation: F) -> Result<T, E>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
    {
        let task = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            Arc::clone(state.pending.get_or_insert_with(|| Arc::new(Task::new())))
        };

        let permit = tokio::select! {
            biased;
            result = task.wait() => return result,
            permit = Arc::clone(&self.token).acquire_owned() => {
                permit.expect("the ordered single-flight semaphore is never closed")
            }
        };

        if let Some(result) = task.result.borrow().clone() {
            return result;
        }

        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if state
                .pending
                .as_ref()
                .is_some_and(|pending| Arc::ptr_eq(pending, &task))
            {
                state.pending = None;
            }
        }

        let executing_task = Arc::clone(&task);
        tokio::spawn(async move {
            let result = operation().await;
            executing_task.finish(result);
            drop(permit);
        });

        task.wait().await
    }
}

impl<T, E> Default for OrderedSingleFlight<T, E>
where
    T: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}
