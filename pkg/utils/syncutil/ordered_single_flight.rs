// Copyright 2026 AsterSQL.

use std::{
    future::Future,
    sync::{Arc, Mutex},
};

use tokio::sync::{Semaphore, watch};

/// Cooperative cancellation for one shared execution.
/// The signal fires when its last waiter is dropped, or execution completes.
#[derive(Clone, Debug)]
pub struct ExecutionCancellation {
    cancelled: watch::Sender<bool>,
}

impl ExecutionCancellation {
    /// Reports whether cancellation has been signalled.
    pub fn is_cancelled(&self) -> bool {
        *self.cancelled.borrow()
    }

    /// Waits for cancellation, including a signal sent before this call.
    pub async fn cancelled(&self) {
        let mut receiver = self.cancelled.subscribe();
        receiver
            .wait_for(|cancelled| *cancelled)
            .await
            .expect("the cancellation handle keeps its sender alive");
    }

    fn cancel(&self) {
        self.cancelled.send_replace(true);
    }
}

struct Task<T, E> {
    result: watch::Sender<Option<Result<T, E>>>,
    waiters: Mutex<(usize, bool)>,
    cancellation: ExecutionCancellation,
}

impl<T, E> Task<T, E> {
    fn new() -> Self {
        let (result, _) = watch::channel(None);
        Self {
            result,
            waiters: Mutex::new((0, false)),
            cancellation: ExecutionCancellation {
                cancelled: watch::channel(false).0,
            },
        }
    }

    fn seal(&self) {
        let mut waiters = self
            .waiters
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        waiters.1 = true;
        if waiters.0 == 0 {
            self.cancellation.cancel();
        }
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
        self.cancellation.cancel();
    }
}

// A dropped call cancels only its own participation, never another waiter.
struct Waiter<T, E>(Arc<Task<T, E>>);

impl<T, E> Drop for Waiter<T, E> {
    fn drop(&mut self) {
        let mut waiters = self
            .0
            .waiters
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        waiters.0 -= 1;
        if waiters.0 == 0 && waiters.1 {
            self.0.cancellation.cancel();
        }
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
        self.call_cancellable(move |_| operation()).await
    }

    /// Runs an operation with cooperative cancellation.
    ///
    /// Dropping a call future removes that waiter. Once execution starts,
    /// dropping its last waiter signals cancellation. The operation must
    /// observe the signal itself; the ordering permit is held until it exits.
    /// An abandoned pending batch can still be reused by a later caller.
    pub async fn call_cancellable<F, Fut>(&self, operation: F) -> Result<T, E>
    where
        F: FnOnce(ExecutionCancellation) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
    {
        let task = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let task = Arc::clone(state.pending.get_or_insert_with(|| Arc::new(Task::new())));
            task.waiters
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0 += 1;
            task
        };
        let _waiter = Waiter(Arc::clone(&task));

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
            task.seal();
        }

        let executing_task = Arc::clone(&task);
        tokio::spawn(async move {
            let result = operation(executing_task.cancellation.clone()).await;
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
