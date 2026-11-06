// Copyright 2026 AsterSQL.
use super::cancellable_io::CancellableIo;
use axum::{Router, serve::Listener};
use std::{io, net::SocketAddr};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
/// Runs after complete routes and an owned listener are handed to the host executor.
pub type StartCallback = Box<dyn FnOnce() -> Result<(), String> + Send>;
struct OwnedListener<L> {
    inner: L,
    cancel: CancellationToken,
}
impl<L: Listener<Addr = SocketAddr> + Send + 'static> Listener for OwnedListener<L> {
    type Io = CancellableIo<L::Io>;
    type Addr = SocketAddr;
    async fn accept(&mut self) -> (Self::Io, SocketAddr) {
        let (io, addr) = self.inner.accept().await;
        (CancellableIo::new(io, self.cancel.clone()), addr)
    }
    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }
}
/// Own one listener and its cancellation scope; never creates an executor or signals.
pub struct BaseServer {
    cancel: CancellationToken,
    task: Option<JoinHandle<io::Result<()>>>,
    pub address: SocketAddr,
}
impl BaseServer {
    pub async fn start<L, F>(
        runtime: tokio::runtime::Handle,
        parent: &CancellationToken,
        listener: L,
        build: F,
        callbacks: Vec<StartCallback>,
    ) -> Result<Self, String>
    where
        L: Listener<Addr = SocketAddr> + Send + 'static,
        F: FnOnce(CancellationToken) -> Result<Router, String>,
    {
        let address = listener.local_addr().map_err(|e| e.to_string())?;
        let cancel = parent.child_token();
        let router = build(cancel.clone())?;
        let owned = OwnedListener {
            inner: listener,
            cancel: cancel.clone(),
        };
        let stop = cancel.clone();
        let failure = cancel.clone();
        let (ready, wait) = tokio::sync::oneshot::channel();
        let task = runtime.spawn(async move {
            let serving = axum::serve(owned, router).with_graceful_shutdown(stop.cancelled_owned());
            let _ = ready.send(());
            let result = serving.await;
            if result.is_err() {
                failure.cancel();
            }
            result
        });
        let mut server = Self {
            cancel,
            task: Some(task),
            address,
        };
        if wait.await.is_err() {
            let _ = server.shutdown().await;
            return Err("microservice listener failed before ready".into());
        }
        for callback in callbacks {
            if let Err(error) = callback() {
                let cleanup = server.shutdown().await;
                return Err(match cleanup {
                    Ok(()) => error,
                    Err(cleanup) => format!("{error}; shutdown failed: {cleanup}"),
                });
            }
        }
        Ok(server)
    }
    pub fn is_closed(&self) -> bool {
        self.cancel.is_cancelled()
    }
    pub fn cancellation(&self) -> CancellationToken {
        self.cancel.clone()
    }
    pub async fn shutdown(&mut self) -> Result<(), String> {
        self.cancel.cancel();
        let Some(mut task) = self.task.take() else {
            return Ok(());
        };
        match tokio::time::timeout(
            astersql_pd_mcs_constant::DEFAULT_GRPC_GRACEFUL_STOP_TIMEOUT,
            &mut task,
        )
        .await
        {
            Ok(Ok(result)) => result.map_err(|e| e.to_string()),
            Ok(Err(error)) => Err(error.to_string()),
            Err(_) => {
                task.abort();
                let _ = task.await;
                Err("microservice listener shutdown timed out".into())
            }
        }
    }
}
impl Drop for BaseServer {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}
