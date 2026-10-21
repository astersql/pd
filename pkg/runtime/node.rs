// Copyright 2026 AsterSQL.
use tokio::{runtime::Handle, sync::oneshot, task::JoinHandle};
use tokio_util::sync::CancellationToken;

/// Assembles a node on a host-owned runtime and cancellation scope.
pub struct PdNodeBuilder {
    runtime: Handle,
    cancellation: CancellationToken,
}

impl PdNodeBuilder {
    /// Uses the supplied runtime without creating a private executor.
    pub fn new(runtime: Handle, cancellation: CancellationToken) -> Self {
        Self {
            runtime,
            cancellation: cancellation.child_token(),
        }
    }

    /// Starts the empty lifecycle worker; no listeners or business APIs exist yet.
    pub fn start(self) -> PdNode {
        let (ready_tx, ready_rx) = oneshot::channel();
        let cancellation = self.cancellation.clone();
        let worker = self.runtime.spawn(async move {
            let _ = ready_tx.send(());
            cancellation.cancelled().await;
        });
        PdNode {
            cancellation: self.cancellation,
            ready: Some(ready_rx),
            worker: Some(worker),
        }
    }
}

/// Owns the lifecycle worker. Explicit shutdown waits for its completion.
pub struct PdNode {
    cancellation: CancellationToken,
    ready: Option<oneshot::Receiver<()>>,
    worker: Option<JoinHandle<()>>,
}

impl PdNode {
    /// Waits for the empty worker to start, not for PD business readiness.
    pub async fn ready(&mut self) -> Result<(), oneshot::error::RecvError> {
        if let Some(ready) = self.ready.as_mut() {
            ready.await?;
            self.ready = None;
        }
        Ok(())
    }

    /// Cancels and joins the worker; repeated calls are safe.
    pub async fn shutdown(&mut self) -> Result<(), tokio::task::JoinError> {
        self.cancellation.cancel();
        if let Some(worker) = self.worker.as_mut() {
            worker.await?;
            self.worker = None;
        }
        Ok(())
    }
}

impl Drop for PdNode {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}
