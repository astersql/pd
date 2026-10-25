// Copyright 2026 AsterSQL.
//! Serial routing against the host's current canonical store registry.
use crate::{
    HeartbeatCommand,
    stream_lifecycle::{HeartbeatMessage, HeartbeatMetrics, KeepAliveUnsupported},
};
use astersql_pd_core::StoresInfo;
use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Go's bounded command queue capacity.
pub const HEARTBEAT_CHANNEL_CAPACITY: usize = 1024;

/// Transport failure. The underlying error remains available to the adapter.
pub type StreamError = Box<dyn std::error::Error + Send + Sync>;
/// A send may block; the serial router must await it before applying a rebind.
pub type StreamSend<'a> = Pin<Box<dyn Future<Output = Result<(), StreamError>> + Send + 'a>>;

/// External transport boundary; domain commands carry no wire types.
pub trait HeartbeatStream: Send + Sync {
    fn send(&self, command: HeartbeatCommand) -> StreamSend<'_>;
    fn keep_alive(&self, _cluster_id: u64) -> StreamSend<'_> {
        Box::pin(async { Err(Box::new(KeepAliveUnsupported) as StreamError) })
    }
}
impl HeartbeatStream for mpsc::Sender<HeartbeatCommand> {
    fn send(&self, command: HeartbeatCommand) -> StreamSend<'_> {
        Box::pin(async move {
            self.send(command)
                .await
                .map_err(|err| Box::new(err) as StreamError)
        })
    }
}

/// Channel transport carrying both commands and header-only keepalive.
pub struct MessageStream(pub mpsc::Sender<HeartbeatMessage>);
impl HeartbeatStream for MessageStream {
    fn send(&self, command: HeartbeatCommand) -> StreamSend<'_> {
        Box::pin(async move {
            self.0
                .send(HeartbeatMessage::Command(command))
                .await
                .map_err(|e| Box::new(e) as StreamError)
        })
    }
    fn keep_alive(&self, cluster_id: u64) -> StreamSend<'_> {
        Box::pin(async move {
            self.0
                .send(HeartbeatMessage::KeepAlive { cluster_id })
                .await
                .map_err(|e| Box::new(e) as StreamError)
        })
    }
}

struct StreamUpdate {
    store_id: u64,
    stream: Arc<dyn HeartbeatStream>,
}

/// Cloneable bounded inputs. A successful enqueue does not acknowledge delivery.
#[derive(Clone)]
pub struct RoutingHandle {
    commands: mpsc::Sender<HeartbeatCommand>,
    updates: mpsc::Sender<StreamUpdate>,
    cancel: CancellationToken,
}
impl RoutingHandle {
    /// Enqueue a replacement; updates have capacity one, as in Go BindStream.
    /// Only after the router applies it does the stream become current.
    pub async fn bind(
        &self,
        store_id: u64,
        stream: Arc<dyn HeartbeatStream>,
    ) -> Result<(), RoutingClosed> {
        tokio::select! {
            biased;
            _ = self.cancel.cancelled() => Err(RoutingClosed),
            result = self.updates.send(StreamUpdate { store_id, stream }) => result.map_err(|_| RoutingClosed),
        }
    }
    /// Queue an owned command, waiting for capacity.
    pub async fn send(
        &self,
        command: HeartbeatCommand,
    ) -> Result<(), mpsc::error::SendError<HeartbeatCommand>> {
        tokio::select! {
            biased;
            _ = self.cancel.cancelled() => Err(mpsc::error::SendError(command)),
            permit = self.commands.reserve() => match permit {
                Ok(permit) => { permit.send(command); Ok(()) },
                Err(_) => Err(mpsc::error::SendError(command)),
            },
        }
    }
    /// Nonblocking enqueue preserving Tokio's full/closed distinction and payload.
    pub fn try_send(
        &self,
        command: HeartbeatCommand,
    ) -> Result<(), mpsc::error::TrySendError<HeartbeatCommand>> {
        if self.cancel.is_cancelled() {
            return Err(mpsc::error::TrySendError::Closed(command));
        }
        self.commands.try_send(command)
    }
}

/// The routing consumer was dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoutingClosed;
impl std::fmt::Display for RoutingClosed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("heartbeat routing closed")
    }
}
impl std::error::Error for RoutingClosed {}

/// One applied event. Delivery failure removes the binding without a retry.
#[derive(Debug)]
pub enum RouteEvent {
    Bound(u64),
    MissingStore(u64),
    RemovedStore(u64),
    Unbound(u64),
    Sent(u64),
    SendFailed { store_id: u64, error: StreamError },
}

/// Host-driven single consumer. No runtime, worker or duplicated store state.
/// Stream lifecycle/keepalive can drive `step` on the host runtime.
pub struct StreamRouter {
    stores: Arc<StoresInfo>,
    streams: HashMap<u64, Arc<dyn HeartbeatStream>>,
    commands: mpsc::Receiver<HeartbeatCommand>,
    updates: mpsc::Receiver<StreamUpdate>,
    cancel: CancellationToken,
    metrics: Option<Arc<HeartbeatMetrics>>,
}
impl StreamRouter {
    pub fn new(stores: Arc<StoresInfo>) -> (RoutingHandle, Self) {
        let cancel = CancellationToken::new();
        let (commands_tx, commands) = mpsc::channel(HEARTBEAT_CHANNEL_CAPACITY);
        let (updates_tx, updates) = mpsc::channel(1);
        (
            RoutingHandle {
                commands: commands_tx,
                updates: updates_tx,
                cancel: cancel.clone(),
            },
            Self {
                stores,
                streams: HashMap::new(),
                commands,
                updates,
                cancel,
                metrics: None,
            },
        )
    }
    /// Current binding identities, in stable order for diagnostics.
    pub fn bound_stores(&self) -> Vec<u64> {
        let mut ids: Vec<_> = self.streams.keys().copied().collect();
        ids.sort_unstable();
        ids
    }
    /// Apply one event, or finish after both input queues close and drain.
    ///
    /// Cross-queue selection is intentionally unspecified, matching Go select.
    /// Once a send starts, callers must await it; canceling this future during
    /// transport I/O can abandon failure cleanup. Lifecycle owns that decision.
    pub async fn step(&mut self) -> Option<RouteEvent> {
        tokio::select! {
            Some(update) = self.updates.recv() => {
                self.streams.insert(update.store_id, update.stream);
                Some(RouteEvent::Bound(update.store_id))
            }
            Some(command) = self.commands.recv() => Some(self.route(command).await),
            else => None,
        }
    }
    pub(crate) fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }
    pub(crate) fn set_metrics(&mut self, metrics: Arc<HeartbeatMetrics>) {
        self.metrics = Some(metrics);
    }
    pub(crate) async fn lifecycle_step(
        &mut self,
        cluster_id: u64,
        ticker: &mut tokio::time::Interval,
    ) -> bool {
        // Cancellation is selected only before dispatch. Once Send starts we await
        // it, matching Go Close's waitgroup barrier rather than abandoning I/O.
        if self.cancel.is_cancelled() {
            return false;
        }
        tokio::select! {
            _ = self.cancel.cancelled() => false,
            Some(update) = self.updates.recv() => { self.streams.insert(update.store_id,update.stream); true },
            Some(command) = self.commands.recv() => { self.route(command).await; true },
            _ = ticker.tick() => { self.keep_alive(cluster_id).await; true },
        }
    }
    fn record_metrics(&self, id: u64, address: &str, kind: &'static str, status: &'static str) {
        // I/O may span store burial. Do not recreate a deleted store's series.
        if self.stores.get_store(id).is_some_and(|s| !s.is_removed()) {
            if let Some(metrics) = &self.metrics {
                metrics.record_metrics(address, id, kind, status);
            }
        }
    }
    pub(crate) async fn keep_alive(&mut self, cluster_id: u64) {
        let ids = self.bound_stores();
        for id in ids {
            let Some(store) = self.stores.get_store(id).filter(|s| !s.is_removed()) else {
                self.streams.remove(&id);
                continue;
            };
            let result = self.streams[&id].keep_alive(cluster_id).await;
            self.record_metrics(
                id,
                store.address(),
                "keepalive",
                if result.is_ok() { "ok" } else { "err" },
            );
            if result.is_err() {
                self.streams.remove(&id);
            }
        }
    }
    async fn route(&mut self, command: HeartbeatCommand) -> RouteEvent {
        let id = command.target_peer.store_id;
        let Some(store) = self.stores.get_store(id) else {
            self.streams.remove(&id);
            return RouteEvent::MissingStore(id);
        };
        if store.is_removed() {
            self.streams.remove(&id);
            return RouteEvent::RemovedStore(id);
        }
        let Some(stream) = self.streams.get(&id) else {
            self.record_metrics(id, store.address(), "push", "skip");
            return RouteEvent::Unbound(id);
        };
        match stream.send(command).await {
            Ok(()) => {
                self.record_metrics(id, store.address(), "push", "ok");
                RouteEvent::Sent(id)
            }
            Err(error) => {
                self.record_metrics(id, store.address(), "push", "err");
                self.streams.remove(&id);
                RouteEvent::SendFailed {
                    store_id: id,
                    error,
                }
            }
        }
    }
}
