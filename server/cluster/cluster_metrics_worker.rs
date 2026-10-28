// Copyright 2026 AsterSQL.
//! Host-owned periodic collection with registry-scoped cleanup.
use prometheus::{Gauge, GaugeVec, Opts, Registry};
use std::collections::BTreeMap;
use std::{future::Future, time::Duration};
use tokio::{runtime::Handle, task::JoinHandle};
use tokio_util::sync::CancellationToken;

/// Members are supplied by the host's canonical membership adapter.
#[derive(Clone)]
pub struct HealthMember {
    pub name: String,
    pub client_urls: Vec<String>,
}
/// One serving generation owns exactly one registration and joins all I/O.
pub struct ClusterMetricsWorker {
    cancel: CancellationToken,
    join: Option<JoinHandle<()>>,
}
struct Registration(Registry, GaugeVec);
impl Drop for Registration {
    fn drop(&mut self) {
        let _ = self.0.unregister(Box::new(self.1.clone()));
    }
}
impl Drop for ClusterMetricsWorker {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
impl ClusterMetricsWorker {
    pub fn start<F, Fut>(
        executor: &Handle,
        parent: &CancellationToken,
        registry: &Registry,
        mut members: F,
    ) -> prometheus::Result<Self>
    where
        F: FnMut() -> Fut + Send + 'static,
        Fut: Future<Output = Result<Vec<HealthMember>, String>> + Send + 'static,
    {
        let gauge = GaugeVec::new(
            Opts::new("pd_cluster_health_status", "Status of the cluster."),
            &["name"],
        )?;
        registry.register(Box::new(gauge.clone()))?;
        let registration = Registration(registry.clone(), gauge.clone());
        let cancel = parent.child_token();
        let token = cancel.clone();
        let join = executor.spawn(async move {
            let _registration = registration;
            let mut handles: BTreeMap<String, Gauge> = BTreeMap::new();
            let mut ticks = tokio::time::interval_at(tokio::time::Instant::now()+Duration::from_secs(10), Duration::from_secs(10));
            ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! { biased;
                    _ = token.cancelled() => break,
                    _ = ticks.tick() => {
                        let collection = async {
                            if let Ok(members) = members().await {
                                for member in members {
                                    let mut healthy = false;
                                    for url in member.client_urls {
                                        if probe(&url).await { healthy = true; break; }
                                    }
                                    handles.entry(member.name.clone()).or_insert_with(|| gauge.with_label_values(&[&member.name])).set(if healthy {1.} else {0.});
                                }
                            }
                        };
                        tokio::select! { biased; _ = token.cancelled() => break, _ = collection => {} }
                    }
                }
            }

        });
        Ok(Self {
            cancel,
            join: Some(join),
        })
    }
    pub async fn stop(mut self) -> Result<(), tokio::task::JoinError> {
        self.cancel.cancel();
        self.join.take().expect("worker join handle").await
    }
}
async fn probe(url: &str) -> bool {
    use http_body_util::Empty;
    use hyper::{Request, body::Bytes};
    use hyper_util::rt::TokioIo;
    let fetch = async {
        let uri: hyper::Uri = format!("{}/pd/api/v1/ping", url.trim_end_matches('/'))
            .parse()
            .ok()?;
        if uri.scheme_str() != Some("http") {
            return None;
        }
        let host = uri.host()?;
        let stream = tokio::net::TcpStream::connect((host, uri.port_u16().unwrap_or(80)))
            .await
            .ok()?;
        let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
            .await
            .ok()?;
        let request = Request::get(uri.path_and_query()?.as_str())
            .header("Host", uri.authority()?.as_str())
            .header("PD-Allow-follower-handle", "true")
            .body(Empty::<Bytes>::new())
            .ok()?;
        let response = sender.send_request(request);
        tokio::pin!(response);
        let response = tokio::select! {
            result = &mut response => result.ok()?,
            result = connection => { result.ok()?; response.await.ok()? },
        };
        Some(response.status() == hyper::StatusCode::OK)
    };
    tokio::time::timeout(Duration::from_secs(3), fetch)
        .await
        .ok()
        .flatten()
        .unwrap_or(false)
}
