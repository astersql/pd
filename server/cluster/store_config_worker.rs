// Copyright 2026 AsterSQL.
//! Host-owned TiKV configuration synchronization.
use astersql_pd_core::{NodeState, StoreInfo};
use astersql_pd_schedule_config::StoreConfig;
use http_body_util::{BodyExt, Empty};
use hyper::{Request, body::Bytes};
use hyper_util::rt::TokioIo;
use std::{future::Future, sync::Arc, time::Duration};
use tokio::{runtime::Handle, task::JoinHandle};
use tokio_util::sync::CancellationToken;

/// Immediate synchronization followed by Go's one-minute retry cadence.
pub struct StoreConfigWorker {
    wake: std::sync::Arc<tokio::sync::Notify>,
    cancel: CancellationToken,
    join: JoinHandle<()>,
}
impl StoreConfigWorker {
    /// Use the host executor and pass cancellation through to in-flight HTTP work.
    /// The callback must finish admitted storage transactions before returning.
    pub fn start<F, Fut>(executor: &Handle, parent: &CancellationToken, mut sync: F) -> Self
    where
        F: FnMut(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let cancel = parent.child_token();
        let token = cancel.clone();
        let wake = std::sync::Arc::new(tokio::sync::Notify::new());
        let worker_wake = wake.clone();
        let join = executor.spawn(async move {
            let mut ticks = tokio::time::interval(Duration::from_secs(60));
            ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! { biased;
                    _ = token.cancelled() => break,
                    _ = worker_wake.notified() => {
                        if !token.is_cancelled() {sync(token.clone()).await;}
                    },
                    _ = ticks.tick() => {
                        if !token.is_cancelled() { sync(token.clone()).await; }
                    }
                }
            }
        });
        Self { cancel, join, wake }
    }
    /// A bootstrap transition wakes the existing worker after stores become visible.
    pub fn wake_handle(&self) -> std::sync::Arc<tokio::sync::Notify> {
        self.wake.clone()
    }
    /// Cancel HTTP/retries and join any admitted configuration transaction.
    pub async fn stop(self) -> Result<(), tokio::task::JoinError> {
        self.cancel.cancel();
        self.join.await
    }
}

/// Fetch from the first eligible TiKV; failed candidates are removed for this round.
/// The host refreshes candidates from its shared cluster if no store succeeds.
/// A successful real HTTP observation, before host persistence/publication.
pub struct StoreConfigObservation {
    pub config: StoreConfig,
    pub address: String,
}
type FailureObserver<'a> = Option<&'a (dyn Fn(&str) + Send + Sync)>;
struct ConfigAttempt<'a> {
    address: String,
    failure: FailureObserver<'a>,
    complete: bool,
}
impl Drop for ConfigAttempt<'_> {
    fn drop(&mut self) {
        if !self.complete {
            if let Some(failure) = self.failure {
                failure(&self.address);
            }
        }
    }
}
pub async fn sync_store_config(
    stores: &mut Vec<Arc<StoreInfo>>,
    cancel: &CancellationToken,
) -> Result<Option<StoreConfig>, String> {
    sync_store_config_observed(stores, cancel, None)
        .await
        .map(|report| report.map(|report| report.config))
}
pub async fn sync_store_config_observed(
    stores: &mut Vec<Arc<StoreInfo>>,
    cancel: &CancellationToken,
    failure: FailureObserver<'_>,
) -> Result<Option<StoreConfigObservation>, String> {
    let mut index = 0;
    let mut last_error = None;
    while index < stores.len() {
        if cancel.is_cancelled() {
            return Err("store config sync cancelled".into());
        }
        let store = &stores[index];
        if !store.is_tikv()
            || !store
                .meta()
                .is_some_and(|m| matches!(m.node_state, NodeState::Preparing | NodeState::Serving))
        {
            index += 1;
            continue;
        }
        let fetch = async {
            let address = resolve_status_address(store.status_address(), store.address()).await;
            let mut attempt = ConfigAttempt {
                address,
                failure,
                complete: false,
            };
            let config = fetch_store_config(&attempt.address, cancel).await?;
            attempt.complete = true;
            Ok::<_, String>(StoreConfigObservation {
                config,
                address: attempt.address.clone(),
            })
        };
        let result = tokio::select! {biased;_=cancel.cancelled()=>return Err("store config sync cancelled".into()),result=fetch=>result};
        match result {
            Ok(report) => return Ok(Some(report)),
            Err(error) => {
                last_error = Some(error);
                stores.remove(index);
            }
        }
    }
    last_error.map_or(Ok(None), Err)
}

async fn resolve_status_address(address: &str, back: &str) -> String {
    let resolve = async {
        let a = tokio::net::lookup_host(address).await.ok()?.next()?;
        let b = tokio::net::lookup_host(back).await.ok()?.next()?;
        let local = |ip: std::net::IpAddr| ip.is_loopback() || ip.is_unspecified();
        (local(a.ip()) && !local(b.ip()))
            .then(|| std::net::SocketAddr::new(b.ip(), a.port()).to_string())
    };
    tokio::time::timeout(Duration::from_secs(3), resolve)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| address.into())
}

/// Read and decode the TiKV HTTP status endpoint with Go's three-second deadline.
/// The connection is driven in this future; dropping it closes the socket.
pub async fn fetch_store_config(
    address: &str,
    cancel: &CancellationToken,
) -> Result<StoreConfig, String> {
    let fetch = async {
        let stream = tokio::net::TcpStream::connect(address)
            .await
            .map_err(|e| e.to_string())?;
        let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
            .await
            .map_err(|e| e.to_string())?;
        let request = Request::get("/config")
            .header("Host", address)
            .body(Empty::<Bytes>::new())
            .map_err(|e| e.to_string())?;
        let response = async {
            // Go decodes the response body even for non-2xx statuses.
            let response = sender
                .send_request(request)
                .await
                .map_err(|e| e.to_string())?;
            let body = response
                .into_body()
                .collect()
                .await
                .map_err(|e| e.to_string())?
                .to_bytes();
            let value: serde_json::Value =
                serde_json::from_slice(&body).map_err(|e| e.to_string())?;
            for key in [None, Some("coprocessor"), Some("storage"), Some("readpool")] {
                let part = key.map_or(Some(&value), |key| value.get(key));
                if part.is_some_and(|v| !v.is_null() && !v.is_object()) {
                    return Err("expected store configuration object".into());
                }
            }
            if value
                .get("readpool")
                .and_then(|v| v.get("unified"))
                .is_some_and(|v| !v.is_null() && !v.is_object())
            {
                return Err("expected unified read pool object".into());
            }
            let mut config: StoreConfig = if value.is_null() {
                StoreConfig::default()
            } else {
                serde_json::from_value(value).map_err(|e| e.to_string())?
            };
            config.adjust();
            Ok(config)
        };
        tokio::pin!(response);
        tokio::select! { biased;
            result = &mut response => result,
            result = connection => {
                result.map_err(|e| e.to_string())?;
                // A Connection: close response can be buffered when its driver
                // finishes. Consume it before treating EOF as a failed request.
                response.await
            },
        }
    };
    tokio::select! { biased;
        _ = cancel.cancelled() => Err("store config sync cancelled".into()),
        result = tokio::time::timeout(Duration::from_secs(3), fetch) => result.map_err(|_| "store config request timed out".to_string())?,
    }
}
