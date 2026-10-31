// Copyright 2026 AsterSQL.
//! Per-listener collectors and exposition of the embedding host's Registry.
//! No default registry, process collector, private runtime or detached worker.
use crate::service_audit::{Record, Sink};
use axum::{
    Router,
    extract::State,
    response::{IntoResponse, Response},
    routing::get,
};
use prometheus::{
    Encoder, GaugeVec, HistogramOpts, HistogramVec, IntCounterVec, Opts, Registry, TextEncoder,
    core::Collector,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

/// Owns registrations until every listener/request clone has drained. An error
/// during construction rolls back only collectors registered by this attempt.
pub struct ServiceMetrics {
    pub(crate) stores: crate::service_store_metrics::StoreMetrics,
    registry: Arc<Registry>,
    registered: Vec<Box<dyn Collector>>,
    requests: IntCounterVec,
    handling: HistogramVec,
    pub(crate) tso_handling: prometheus::Histogram,
    pub(crate) tso_batch: prometheus::Histogram,
    pub(crate) query_handling: prometheus::Histogram,
    region_requests: IntCounterVec,
    region_children: Mutex<HashMap<(&'static str, &'static str), prometheus::IntCounter>>,
    forward_failures: IntCounterVec,
    forward_children: Mutex<HashMap<(&'static str, &'static str), prometheus::IntCounter>>,
    concurrency: prometheus::GaugeVec,
    limits: Mutex<Option<crate::http_service_limit::ServiceLimit>>,
    time_back: prometheus::IntCounter,
    metadata: prometheus::GaugeVec,
    consensus: prometheus::GaugeVec,
    client_tls: Mutex<Option<Arc<rustls::ClientConfig>>>,

    children: Mutex<HashMap<&'static str, (prometheus::IntCounter, prometheus::Histogram)>>,
}
impl ServiceMetrics {
    /// Register Go-compatible listener metrics in the supplied host registry.
    pub fn register(registry: Arc<Registry>) -> Result<Arc<Self>, prometheus::Error> {
        let info = GaugeVec::new(
            Opts::new(
                "pd_server_info",
                "Indicate the pd server info, and the value is the start timestamp (s).",
            ),
            &["version", "hash"],
        )?;
        let handling = HistogramVec::new(
            HistogramOpts::new(
                "pd_service_audit_handling_seconds",
                "PD server service handling performance metrics",
            ),
            &["service", "method"],
        )?;
        let requests = IntCounterVec::new(
            Opts::new(
                "pd_service_audit_requests_total",
                "Total number of service requests for audit",
            ),
            &["service", "method", "caller_component"],
        )?;
        let tso_handling = prometheus::Histogram::with_opts(
            HistogramOpts::new(
                "pd_server_handle_tso_duration_seconds",
                "Bucketed histogram of processing time (s) of handled tso requests.",
            )
            .buckets(prometheus::exponential_buckets(0.0005, 2.0, 13)?),
        )?;
        let tso_batch = prometheus::Histogram::with_opts(
            HistogramOpts::new(
                "pd_server_handle_tso_batch_size",
                "Bucketed histogram of the batch size of handled tso requests.",
            )
            .buckets(prometheus::exponential_buckets(1.0, 2.0, 13)?),
        )?;
        let query_handling = prometheus::Histogram::with_opts(
            HistogramOpts::new(
                "pd_server_query_region_duration_seconds",
                "Bucketed histogram of processing time (s) of region query requests.",
            )
            .buckets(prometheus::exponential_buckets(0.0005, 2.0, 13)?),
        )?;
        let region_requests = IntCounterVec::new(
            Opts::new("pd_server_region_request_cnt", "Counter of region request."),
            &["request", "caller_id", "caller_component", "event"],
        )?;
        let forward_failures = IntCounterVec::new(
            Opts::new("pd_server_forward_fail_total", "Counter of forward fail."),
            &["request", "type"],
        )?;
        let concurrency = prometheus::GaugeVec::new(
            Opts::new(
                "pd_server_api_concurrency",
                "Concurrency number of the api.",
            ),
            &["kind", "api"],
        )?;
        let time_back = prometheus::IntCounter::new(
            "pd_monitor_time_jump_back_total",
            "Counter of system time jumps backward.",
        )?;
        let metadata = prometheus::GaugeVec::new(
            Opts::new("pd_cluster_metadata", "Record critical metadata."),
            &["type"],
        )?;
        let mut scope = Self {
            tso_handling,
            tso_batch,
            query_handling,
            region_requests,
            forward_failures,
            concurrency,
            limits: Mutex::new(None),
            time_back,
            metadata,
            consensus: prometheus::GaugeVec::new(
                Opts::new("pd_server_etcd_state", "Etcd raft states."),
                &["type"],
            )?,
            client_tls: Mutex::new(None),
            region_children: Mutex::new(HashMap::new()),
            forward_children: Mutex::new(HashMap::new()),
            stores: crate::service_store_metrics::StoreMetrics::new()?,
            registry,
            registered: Vec::new(),
            requests,
            handling,
            children: Mutex::new(HashMap::new()),
        };
        scope.register_collector(info.clone())?;
        scope.register_collector(scope.handling.clone())?;
        scope.register_collector(scope.requests.clone())?;
        scope.register_collector(scope.tso_handling.clone())?;
        scope.register_collector(scope.tso_batch.clone())?;
        scope.register_collector(scope.query_handling.clone())?;
        scope.register_collector(scope.region_requests.clone())?;
        scope.register_collector(scope.forward_failures.clone())?;
        scope.register_collector(scope.concurrency.clone())?;
        scope.register_collector(scope.time_back.clone())?;
        scope.register_collector(scope.metadata.clone())?;
        scope.register_collector(scope.consensus.clone())?;
        // Go registers these at construction even in classic mode. No MCS
        // requests are admitted here: their untouched zero state is intentional,
        // and ordinary PD forwarding must never update these MCS measurements.
        for (name, help, buckets) in [
            (
                "pd_server_handle_tso_proxy_duration_seconds",
                "Bucketed histogram of processing time (s) of handled tso proxy requests.",
                prometheus::exponential_buckets(0.0005, 2.0, 13)?,
            ),
            (
                "pd_server_handle_tso_proxy_batch_size",
                "Bucketed histogram of the batch size of handled tso proxy requests.",
                prometheus::exponential_buckets(1.0, 2.0, 13)?,
            ),
            (
                "pd_server_forward_tso_duration_seconds",
                "Bucketed histogram of processing time (s) of handled forward tso requests.",
                prometheus::exponential_buckets(0.0005, 2.0, 13)?,
            ),
        ] {
            scope.register_collector(prometheus::Histogram::with_opts(
                HistogramOpts::new(name, help).buckets(buckets),
            )?)?;
        }
        for (name, help) in [
            (
                "pd_server_tso_proxy_forward_timeout_total",
                "Counter of timeouts when tso proxy forwarding tso requests to tso service.",
            ),
            (
                "pd_server_tso_forward_stream_total",
                "Counter of TSO streams forwarded to the independent TSO service.",
            ),
        ] {
            scope.register_collector(prometheus::IntCounter::new(name, help)?)?;
        }

        for (collector, owned) in scope.stores.collectors() {
            scope.registry.register(collector)?;
            scope.registered.push(owned);
        }

        for (method, kind) in [
            ("region_heartbeat", "send"),
            ("region_heartbeat", "client"),
            ("region_heartbeat", "stream"),
            ("region_heartbeat", "recv"),
            ("scatter_region", "send"),
            ("split_regions", "send"),
            ("store_heartbeat", "send"),
            ("get_operator", "send"),
        ] {
            scope.forward_children.lock().unwrap().insert(
                (method, kind),
                scope.forward_failures.with_label_values(&[method, kind]),
            );
        }

        info.with_label_values(&[
            option_env!("PD_RELEASE_VERSION").unwrap_or(env!("CARGO_PKG_VERSION")),
            option_env!("PD_GIT_HASH").unwrap_or("None"),
        ])
        .set(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64(),
        );
        Ok(Arc::new(scope))
    }
    /// Pin configured client roots at host startup, as Go's HTTP client does.
    pub fn prepare_transport(
        &self,
        config: &astersql_pd_server_config::Config,
    ) -> Result<(), String> {
        if config
            .pd_server
            .metric_storage
            .to_ascii_lowercase()
            .starts_with("https://")
            || config
                .metric
                .push_address
                .to_ascii_lowercase()
                .starts_with("https://")
        {
            *self.client_tls.lock().unwrap() =
                Some(Arc::new(crate::metric_transport::tls(config)?));
        }
        Ok(())
    }
    pub fn client_tls(&self) -> Option<Arc<rustls::ClientConfig>> {
        self.client_tls.lock().unwrap().clone()
    }
    pub(crate) fn region_result(&self, method: &'static str, error: bool) {
        use rand::Rng;
        if !error && rand::thread_rng().gen_range(0..100) != 0 {
            return;
        }
        let event = if error { "failed" } else { "success" };
        if let Ok(mut cache) = self.region_children.lock() {
            cache
                .entry((method, event))
                .or_insert_with(|| {
                    self.region_requests
                        .with_label_values(&[method, "unknown", "unknown", event])
                })
                .inc();
        }
    }
    pub(crate) fn set_limits(&self, limits: crate::http_service_limit::ServiceLimit) {
        *self.limits.lock().unwrap() = Some(limits);
    }
    /// Remove every address series for stores no longer in canonical membership.
    pub fn retain_stores(&self, ids: impl IntoIterator<Item = u64>) {
        self.stores.retain(ids);
    }
    /// Mark this listener's fixed cluster identity using Go's metadata sentinel.
    pub fn set_cluster(&self, id: u64) {
        self.metadata
            .with_label_values(&[&format!("cluster{id}")])
            .set(0.0);
    }
    /// Poll and join on the caller's runtime; no detached metrics workers.
    pub async fn run_worker(self: Arc<Self>, cancel: tokio_util::sync::CancellationToken) {
        self.run_worker_with_clock(cancel, Arc::new(SystemWallClock))
            .await;
    }
    pub(crate) async fn run_worker_with_clock(
        self: Arc<Self>,
        cancel: tokio_util::sync::CancellationToken,
        clock: Arc<dyn WallClock>,
    ) {
        let mut tick = tokio::time::interval_at(
            tokio::time::Instant::now() + std::time::Duration::from_millis(100),
            std::time::Duration::from_millis(100),
        );
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut collection = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            let before = clock.now();
            tokio::select! {biased;_=cancel.cancelled()=>return,_=tick.tick()=>{}}
            if clock.now() < before {
                self.time_back.inc();
            }
            if tokio::time::Instant::now() >= collection {
                let limits = self.limits.lock().unwrap().clone();
                if let Some(limits) = limits {
                    let collect = async {
                        limits.collect_metrics(&self.concurrency).await;
                        if let Some(state) = limits.consensus_metrics().await {
                            for (kind, value) in [
                                ("term", state.term),
                                ("appliedIndex", state.applied_index),
                                ("committedIndex", state.committed_index),
                            ] {
                                self.consensus.with_label_values(&[kind]).set(value as f64);
                            }
                        }

                        if let Some(ids) = limits.store_ids().await {
                            self.retain_stores(ids);
                        }
                    };
                    tokio::select! {biased;_=cancel.cancelled()=>return,_=collect=>{}}
                }
                collection = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
            }
        }
    }
    fn register_collector<C: Collector + Clone + 'static>(
        &mut self,
        collector: C,
    ) -> Result<(), prometheus::Error> {
        self.registry.register(Box::new(collector.clone()))?;
        self.registered.push(Box::new(collector));
        Ok(())
    }
    pub(crate) fn router(self: Arc<Self>) -> Router {
        Router::new()
            .route("/metrics", get(scrape))
            .with_state(self)
    }
}
impl Sink for ServiceMetrics {
    fn record(&self, record: Record) -> Result<(), String> {
        if record.policy.prometheus
            && record.status.is_some()
            && crate::service_audit::policy(record.service).is_some_and(|p| p.prometheus)
        {
            // Go's caller value is unbounded external input. The wire label
            // schema is preserved with one anonymous series, matching the
            // default caller. Never accept arbitrary caller IDs as labels.
            let mut children = self.children.lock().map_err(|_| "metrics cache poisoned")?;
            let (requests, handling) = children.entry(record.service).or_insert_with(|| {
                (
                    self.requests
                        .with_label_values(&[record.service, "HTTP", "anonymous"]),
                    self.handling.with_label_values(&[record.service, "HTTP"]),
                )
            });
            requests.inc();
            handling.observe(record.elapsed.as_secs_f64());
        }
        Ok(())
    }
}
impl Drop for ServiceMetrics {
    fn drop(&mut self) {
        for collector in self.registered.drain(..).rev() {
            // Only this scope can own these registrations; the host may have
            // explicitly unregistered them earlier, in which case cleanup is done.
            let _ = self.registry.unregister(collector);
        }
    }
}
async fn scrape(State(metrics): State<Arc<ServiceMetrics>>) -> Response {
    let encoder = TextEncoder::new();
    let mut output = Vec::new();
    match encoder.encode(&metrics.registry.gather(), &mut output) {
        Ok(()) => ([("content-type", encoder.format_type())], output).into_response(),
        Err(_) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "metrics encoding failed\n",
        )
            .into_response(),
    }
}

/// Host-polled Pushgateway worker. Cancellation drops DNS, TLS, HTTP and any
/// in-flight body; the embedding host joins this future before releasing storage.
pub async fn push_worker(
    registry: Arc<Registry>,
    config: Arc<astersql_pd_server_config::Config>,
    instance: String,
    cancel: tokio_util::sync::CancellationToken,
) {
    use crate::metric_transport::{Target, send};
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use std::time::Duration;
    let metric = &config.metric;
    if metric.push_address.is_empty()
        || metric.push_interval.as_nanos() <= 0
        || metric.push_job.is_empty()
    {
        cancel.cancelled().await;
        return;
    }
    fn component(value: &str) -> (String, bool) {
        use base64::Engine;
        if value.is_empty() {
            return ("=".into(), true);
        }
        if value.contains('/') {
            return (
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value),
                true,
            );
        }
        (
            value
                .bytes()
                .map(|b| match b {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                        (b as char).to_string()
                    }
                    b' ' => "+".into(),
                    _ => format!("%{b:02X}"),
                })
                .collect(),
            false,
        )
    }
    let address = if metric.push_address.contains("://") {
        metric.push_address.clone()
    } else {
        format!("http://{}", metric.push_address)
    };
    let (job, job64) = component(&metric.push_job);
    let (instance, instance64) = component(&instance);
    let path = format!(
        "{}/metrics/job{}/{job}/instance{}/{instance}",
        address.trim_end_matches('/'),
        if job64 { "@base64" } else { "" },
        if instance64 { "@base64" } else { "" }
    );
    let transport_config = astersql_pd_server_config::Config::default();
    loop {
        if cancel.is_cancelled() {
            return;
        }
        let attempt = async {
            let target = Target::parse_push(&path)?;
            let bytes = crate::metric_proto::encode(&registry.gather())
                .map_err(|_| "metrics encoding failed")?;
            let mut request = Request::builder()
                .method("PUT")
                .uri(target.uri.path_and_query().map_or("/", |p| p.as_str()))
                .header(
                    "host",
                    target.uri.authority().ok_or("missing authority")?.as_str(),
                )
                .header("content-type", crate::metric_proto::CONTENT_TYPE)
                .body(Body::from(bytes))
                .map_err(|_| "invalid push request")?;
            if let Some(auth) = &target.authorization {
                request.headers_mut().insert("authorization", auth.clone());
            }
            let response = send(
                target,
                request,
                &transport_config,
                false,
                None,
                cancel.clone(),
                tokio::time::Instant::now() + Duration::from_secs(30),
            )
            .await?;
            if !response.status().is_success() {
                return Err("push failed".to_string());
            }
            response
                .into_body()
                .collect()
                .await
                .map_err(|_| "push response failed")?;
            Ok::<_, String>(())
        };
        // A gateway outage must not stop the PD listener. Every attempt is
        // bounded, and the next retry follows the configured completion interval.
        tokio::select! {biased;_=cancel.cancelled()=>return,_=attempt=>{}}
        tokio::select! {biased;_=cancel.cancelled()=>return,_=tokio::time::sleep(Duration::from_nanos(metric.push_interval.as_nanos() as u64))=>{}}
    }
}

pub(crate) trait WallClock: Send + Sync {
    fn now(&self) -> std::time::SystemTime;
}
struct SystemWallClock;
impl WallClock for SystemWallClock {
    fn now(&self) -> std::time::SystemTime {
        std::time::SystemTime::now()
    }
}

/// Raft measurements are exported only for an explicitly bound real member.
#[derive(Clone, Copy)]
pub struct ConsensusMetrics {
    pub term: u64,
    pub applied_index: u64,
    pub committed_index: u64,
}
