// Copyright 2026 AsterSQL.
//! Store labels come only from canonical membership, never heartbeat payloads.
use prometheus::{CounterVec, HistogramOpts, HistogramVec, Opts, core::Collector};
use std::{
    collections::{HashMap, HashSet},
    sync::Mutex,
};
struct Children {
    address: String,
    store: String,
    counters: HashMap<(bool, &'static str), prometheus::Counter>,
    histograms: HashMap<&'static str, prometheus::Histogram>,
}
pub(crate) struct StoreMetrics {
    region: CounterVec,
    buckets: CounterVec,
    region_latency: HistogramVec,
    region_handling: HistogramVec,
    store_handling: HistogramVec,
    bucket_handling: HistogramVec,
    bucket_interval: HistogramVec,
    children: Mutex<HashMap<u64, Children>>,
}
impl StoreMetrics {
    pub fn new() -> Result<Self, prometheus::Error> {
        let hist = |name, help, buckets| {
            HistogramVec::new(
                HistogramOpts::new(name, help).buckets(buckets),
                &["address", "store"],
            )
        };
        Ok(Self {
            region: CounterVec::new(
                Opts::new(
                    "pd_scheduler_region_heartbeat",
                    "Counter of region heartbeat.",
                ),
                &["address", "store", "type", "status"],
            )?,
            buckets: CounterVec::new(
                Opts::new("pd_server_bucket_report", "Counter of bucket report."),
                &["address", "store", "type", "status"],
            )?,
            region_latency: hist(
                "pd_scheduler_region_heartbeat_latency_seconds",
                "Bucketed histogram of latency (s) of receiving heartbeat.",
                prometheus::exponential_buckets(0.0005, 2.0, 13)?,
            )?,
            region_handling: hist(
                "pd_scheduler_handle_region_heartbeat_duration_seconds",
                "Bucketed histogram of processing time (s) of handled region heartbeat requests.",
                prometheus::exponential_buckets(0.0001, 2.0, 29)?,
            )?,
            store_handling: hist(
                "pd_scheduler_handle_store_heartbeat_duration_seconds",
                "Bucketed histogram of processing time (s) of handled store heartbeat requests.",
                prometheus::exponential_buckets(0.0001, 2.0, 29)?,
            )?,
            bucket_handling: hist(
                "pd_server_handle_bucket_report_duration_seconds",
                "Bucketed histogram of processing time (s) of handled bucket report requests.",
                prometheus::exponential_buckets(0.0001, 2.0, 29)?,
            )?,
            bucket_interval: hist(
                "pd_server_bucket_report_interval_seconds",
                "Bucketed histogram of processing time (s) of handled bucket report requests.",
                prometheus::linear_buckets(0.0, 30.0, 20)?,
            )?,
            children: Mutex::new(HashMap::new()),
        })
    }
    pub fn collectors(&self) -> Vec<(Box<dyn Collector>, Box<dyn Collector>)> {
        macro_rules! pair {
            ($v:expr) => {
                (
                    Box::new($v.clone()) as Box<dyn Collector>,
                    Box::new($v.clone()) as Box<dyn Collector>,
                )
            };
        }
        vec![
            pair!(self.region),
            pair!(self.buckets),
            pair!(self.region_latency),
            pair!(self.region_handling),
            pair!(self.store_handling),
            pair!(self.bucket_handling),
            pair!(self.bucket_interval),
        ]
    }
    fn erase(&self, c: &Children) {
        for ((bucket, status), _) in &c.counters {
            let _ = if *bucket { &self.buckets } else { &self.region }
                .remove_label_values(&[&c.address, &c.store, "report", status]);
        }
        for kind in c.histograms.keys() {
            let _ = self
                .histogram(kind)
                .remove_label_values(&[&c.address, &c.store]);
        }
    }
    fn histogram(&self, kind: &str) -> &HistogramVec {
        match kind {
            "region_latency" => &self.region_latency,
            "region" => &self.region_handling,
            "store" => &self.store_handling,
            "bucket" => &self.bucket_handling,
            "interval" => &self.bucket_interval,
            _ => unreachable!("static metric kind"),
        }
    }
    fn with_children(&self, id: u64, address: &str, f: impl FnOnce(&mut Children)) {
        let mut children = self.children.lock().unwrap();
        if children.get(&id).is_some_and(|c| c.address != address) {
            let old = children.remove(&id).unwrap();
            self.erase(&old);
        }
        let c = children.entry(id).or_insert_with(|| Children {
            address: address.into(),
            store: if id == 0 {
                String::new()
            } else {
                id.to_string()
            },
            counters: HashMap::new(),
            histograms: HashMap::new(),
        });
        f(c);
    }
    pub fn counter(&self, id: u64, address: &str, bucket: bool, status: &'static str) {
        self.with_children(id, address, |c| {
            c.counters
                .entry((bucket, status))
                .or_insert_with(|| {
                    if bucket { &self.buckets } else { &self.region }
                        .with_label_values(&[&c.address, &c.store, "report", status])
                })
                .inc()
        });
    }
    pub fn observe(&self, id: u64, address: &str, kind: &'static str, value: f64) {
        self.with_children(id, address, |c| {
            c.histograms
                .entry(kind)
                .or_insert_with(|| {
                    self.histogram(kind)
                        .with_label_values(&[&c.address, &c.store])
                })
                .observe(value)
        });
    }
    pub fn retain(&self, ids: impl IntoIterator<Item = u64>) {
        let ids: HashSet<_> = ids.into_iter().collect();
        let mut children = self.children.lock().unwrap();
        children.retain(|id, c| {
            if *id == 0 || ids.contains(id) {
                true
            } else {
                self.erase(c);
                false
            }
        });
    }
}
pub(crate) async fn labels(
    services: &dyn crate::PdServices,
    id: u64,
) -> Option<(
    std::sync::Arc<crate::service_metrics::ServiceMetrics>,
    String,
)> {
    let metrics = services.service_metrics()?;
    let store = services.cluster_read_service()?.store(id).await.ok()??;
    if store.is_removed() {
        return None;
    }
    Some((metrics, store.address().into()))
}
