// Copyright 2026 AsterSQL.
//! Host-runtime heartbeat lifetime and host-registered compatible metrics.
use crate::{HeartbeatCommand, routing::StreamRouter};
use prometheus::{IntCounter, IntCounterVec, Opts, Registry};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    runtime::Handle,
    task::{JoinError, JoinHandle},
};
use tokio_util::sync::CancellationToken;

pub const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(60);

/// Header-only keepalive is distinct from a region command with a zero peer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HeartbeatMessage {
    Command(HeartbeatCommand),
    KeepAlive { cluster_id: u64 },
}

/// A command-only transport has no representation for header-only keepalive.
#[derive(Debug)]
pub struct KeepAliveUnsupported;
impl std::fmt::Display for KeepAliveUnsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("heartbeat transport does not support keepalive")
    }
}
impl std::error::Error for KeepAliveUnsupported {}

/// Owned, explicitly registered metric family; no global registry or runtime.
/// Labels and help retain Go's public metric contract.
pub struct HeartbeatMetrics {
    counters: IntCounterVec,
    series: Mutex<HashMap<(u64, String, &'static str, &'static str), IntCounter>>,
}
impl HeartbeatMetrics {
    pub fn new(registry: &Registry) -> Result<Self, prometheus::Error> {
        let counters = IntCounterVec::new(
            Opts::new(
                "pd_hbstream_region_message",
                "Counter of message hbstream sent.",
            ),
            &["address", "store", "type", "status"],
        )?;
        registry.register(Box::new(counters.clone()))?;
        Ok(Self {
            counters,
            series: Mutex::new(HashMap::new()),
        })
    }
    pub(crate) fn record_metrics(
        &self,
        address: &str,
        id: u64,
        kind: &'static str,
        status: &'static str,
    ) {
        let mut series = self.series.lock().unwrap();
        series
            .entry((id, address.to_owned(), kind, status))
            .or_insert_with(|| {
                self.counters
                    .with_label_values(&[address, &id.to_string(), kind, status])
            })
            .inc();
    }
    /// Delete all address/type/status series for the store, including old addresses.
    pub fn delete_store_metrics(&self, id: u64) {
        let mut series = self.series.lock().unwrap();
        series.retain(|(store, address, kind, status), _| {
            if *store != id {
                return true;
            }
            self.counters
                .remove_label_values(&[address.as_str(), &id.to_string(), *kind, *status])
                .expect("registered heartbeat series");
            false
        });
    }
    pub fn reset_metrics(&self) {
        let mut series = self.series.lock().unwrap();
        self.counters.reset();
        series.clear();
    }
}

/// One serial background consumer on the supplied runtime. Stop is idempotent;
/// join waits for in-flight transport I/O before dropping all pending input.
/// Transport adapters must bound I/O, as Go Send is responsible for returning.
/// Dropping requests cancellation; hosts use `stop` + `join` for a shutdown barrier.
pub struct HeartbeatStreams {
    cancel: CancellationToken,
    task: Option<JoinHandle<()>>,
}
impl HeartbeatStreams {
    pub fn start(
        mut router: StreamRouter,
        cluster_id: u64,
        metrics: Arc<HeartbeatMetrics>,
        runtime: &Handle,
    ) -> Self {
        let cancel = router.cancel_token();
        let task = runtime.spawn(async move {
            router.set_metrics(metrics);
            let mut ticker = tokio::time::interval_at(
                tokio::time::Instant::now() + KEEP_ALIVE_INTERVAL,
                KEEP_ALIVE_INTERVAL,
            );
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            while router.lifecycle_step(cluster_id, &mut ticker).await {}
        });
        Self {
            cancel,
            task: Some(task),
        }
    }
    pub fn stop(&self) {
        self.cancel.cancel();
    }
    /// Cancellation-safe join: a dropped join future retains the task handle.
    pub async fn join(&mut self) -> Result<(), JoinError> {
        if let Some(task) = self.task.as_mut() {
            let result = task.await;
            self.task = None;
            result
        } else {
            Ok(())
        }
    }
}
impl Drop for HeartbeatStreams {
    fn drop(&mut self) {
        self.stop();
    }
}
