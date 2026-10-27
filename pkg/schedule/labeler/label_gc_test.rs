// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::StorageEndpoint;
use astersql_pd_storage_domain::{MemoryStore, MetaStore};
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;

async fn contract(store: Arc<dyn MetaStore>) {
    let paths = ClusterPaths::new(200);
    let mut owner = RegionLabeler::load(StorageEndpoint::new(store.clone()), paths, 0)
        .await
        .unwrap();
    for (id, permanent) in [("dead", false), ("partial", true)] {
        let mut labels = vec![json!({"key":"old","value":"v","ttl":"1s"})];
        if permanent {
            labels.push(json!({"key":"keep","value":"v"}));
        }
        let rule=LabelRule::from_json(&serde_json::to_vec(&json!({"id":id,"labels":labels,"rule_type":"key-range","data":[{"start_key":"10","end_key":"20"}]})).unwrap(),0).unwrap();
        owner.set_validated_label_rule(rule).await.unwrap();
    }
    assert!(owner.clear_expired_labels(1_000_000_000).await.is_empty());
    assert_eq!(owner.index().rule("dead").unwrap().labels.len(), 1);
    assert_eq!(owner.get_rule_and_key_range_counts(1_000_000_001), (1, 1));
    assert!(owner.clear_expired_labels(1_000_000_001).await.is_empty());
    assert!(owner.index().rule("dead").is_none());
    assert_eq!(owner.index().rule("partial").unwrap().labels.len(), 1);
    let restored = RegionLabeler::load(StorageEndpoint::new(store), paths, 1_000_000_001)
        .await
        .unwrap();
    assert_eq!(restored.index().rules().count(), 1);
    assert_eq!(
        restored.index().rule("partial").unwrap().labels[0].key,
        "keep"
    );
}
#[tokio::test]
async fn label_gc_memory_expiry_restart() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn label_gc_etcd_expiry_restart() {
    let f = label_restore_test::fixture::Fixture::start();
    contract(Arc::new(f.store().await)).await;
}
#[tokio::test(start_paused = true)]
async fn label_gc_background_cancellation_and_cadence() {
    let owner = Arc::new(Mutex::new(
        RegionLabeler::load(
            StorageEndpoint::new(Arc::new(MemoryStore::default())),
            ClusterPaths::new(201),
            0,
        )
        .await
        .unwrap(),
    ));
    let rule=LabelRule::from_json(br#"{"id":"dead","labels":[{"key":"k","value":"v","ttl":"1s"}],"rule_type":"key-range","data":[{"start_key":"10","end_key":"20"}]}"#,0).unwrap();
    owner
        .lock()
        .await
        .set_validated_label_rule(rule)
        .await
        .unwrap();
    let start = tokio::time::Instant::now();
    let gc = LabelerGc::start(owner.clone(), Duration::from_secs(2), move || {
        start.elapsed().as_nanos() as i128
    });
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(1)).await;
    assert_eq!(owner.lock().await.index().rules().count(), 1);
    tokio::time::advance(Duration::from_secs(1)).await;
    tokio::task::yield_now().await;
    assert_eq!(owner.lock().await.index().rules().count(), 0);
    assert!(gc.shutdown().await.unwrap().is_empty());
    let guard = owner.lock().await;
    let gc = LabelerGc::start(owner.clone(), Duration::from_secs(1), || 0);
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(1)).await;
    tokio::task::yield_now().await;
    assert!(gc.shutdown().await.unwrap().is_empty());
    drop(guard);
}
#[test]
fn label_gc_metrics_host_registry() {
    let registry = prometheus::Registry::new();
    let metrics = LabelerMetrics::register(&registry).unwrap();
    metrics
        .events
        .with_label_values(&["checkers", "deny"])
        .inc();
    metrics.creation_duration.observe(0.01);
    assert!(LabelerMetrics::register(&registry).is_err());
    let families = registry.gather();
    assert_eq!(families.len(), 2);
    let counter = families
        .iter()
        .find(|f| f.name() == "pd_schedule_labeler_event_counter")
        .unwrap();
    assert_eq!(counter.help(), "Counter of the scheduler label.");
    assert_eq!(
        counter.get_field_type(),
        prometheus::proto::MetricType::COUNTER
    );
    assert_eq!(
        counter.get_metric()[0]
            .get_label()
            .iter()
            .map(|l| l.name())
            .collect::<Vec<_>>(),
        vec!["event", "type"]
    );
    let hist = families
        .iter()
        .find(|f| f.name() == "pd_schedule_new_region_labeler_duration_seconds")
        .unwrap();
    assert_eq!(hist.get_metric()[0].get_histogram().get_bucket().len(), 13);
}

use astersql_pd_storage_domain::*;
use async_trait::async_trait;
use std::sync::atomic::{AtomicIsize, Ordering};
struct Gate {
    inner: Arc<dyn MetaStore>,
    reads: AtomicIsize,
    writes: AtomicIsize,
}
impl Gate {
    fn new(inner: Arc<dyn MetaStore>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            reads: AtomicIsize::new(-1),
            writes: AtomicIsize::new(-1),
        })
    }
}
#[async_trait]
impl MetaStore for Gate {
    async fn read(&self, r: Read) -> Result<Snapshot> {
        if self.reads.fetch_sub(1, Ordering::SeqCst) == 1 {
            return Err(StoreError::Unavailable);
        }
        self.inner.read(r).await
    }
    async fn transact(&self, c: Vec<Compare>, w: Vec<Mutation>) -> Result<Commit> {
        if self.writes.fetch_sub(1, Ordering::SeqCst) == 1 {
            return Err(StoreError::Unavailable);
        }
        self.inner.transact(c, w).await
    }
    async fn transact_branches(
        &self,
        c: Vec<Compare>,
        t: Vec<Mutation>,
        e: Vec<Mutation>,
    ) -> Result<Commit> {
        self.inner.transact_branches(c, t, e).await
    }
    async fn transact_raw(
        &self,
        c: Vec<Compare>,
        t: Vec<RawTxnOp>,
        e: Vec<RawTxnOp>,
    ) -> Result<RawTxnResponse> {
        self.inner.transact_raw(c, t, e).await
    }
    async fn watch(&self, p: Vec<u8>, r: Revision) -> Result<Box<dyn MetaWatch>> {
        self.inner.watch(p, r).await
    }
    async fn grant(&self, t: i64) -> Result<Lease> {
        self.inner.grant(t).await
    }
    async fn keep_alive(&self, l: LeaseId) -> Result<Lease> {
        self.inner.keep_alive(l).await
    }
    async fn revoke(&self, l: LeaseId) -> Result<()> {
        self.inner.revoke(l).await
    }
    async fn ttl(&self, l: LeaseId) -> Result<i64> {
        self.inner.ttl(l).await
    }
    async fn compact(&self, r: Revision) -> Result<()> {
        self.inner.compact(r).await
    }
}
async fn gc_failure_contract(store: Arc<dyn MetaStore>) {
    let gate = Gate::new(store.clone());
    let paths = ClusterPaths::new(202);
    let mut owner = RegionLabeler::load(StorageEndpoint::new(gate.clone()), paths, 0)
        .await
        .unwrap();
    for id in ["a", "b"] {
        let rule=LabelRule::from_json(&serde_json::to_vec(&json!({"id":id,"labels":[{"key":"old","value":"v","ttl":"1s"}],"rule_type":"key-range","data":[{"start_key":"10","end_key":"20"}]})).unwrap(),0).unwrap();
        owner.set_validated_label_rule(rule).await.unwrap();
    }
    gate.writes.store(1, Ordering::SeqCst);
    assert_eq!(owner.clear_expired_labels(1_000_000_001).await.len(), 1);
    assert!(owner.index().rule("a").unwrap().labels.is_empty());
    assert!(
        owner.index().rule("b").is_none(),
        "continue after failed delete"
    );
    assert_eq!(owner.get_rule_and_key_range_counts(1_000_000_001), (0, 0));
    let restored = RegionLabeler::load(StorageEndpoint::new(store), paths, 1_000_000_001)
        .await
        .unwrap();
    assert_eq!(
        restored.index().rules().count(),
        0,
        "failed GC persistence cannot resurrect expired data"
    );
}
#[tokio::test]
async fn label_gc_memory_io_failure() {
    gc_failure_contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn label_gc_etcd_io_failure() {
    let f = label_restore_test::fixture::Fixture::start();
    gc_failure_contract(Arc::new(f.store().await)).await;
}
#[tokio::test]
async fn label_gc_go_runtime_fixture() {
    let oracle: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule/labeler/task20/go.json"
    ))
    .unwrap();
    let mut input = oracle["before"][1].clone();
    input["labels"][0]["start_at"] = json!("Thu Jan  1 00:00:00 UTC 1970");
    let rule = LabelRule::from_json(&serde_json::to_vec(&input).unwrap(), 0).unwrap();
    let mut owner = RegionLabeler::load(
        StorageEndpoint::new(Arc::new(MemoryStore::default())),
        ClusterPaths::new(203),
        0,
    )
    .await
    .unwrap();
    owner.set_validated_label_rule(rule).await.unwrap();
    assert!(owner.clear_expired_labels(2_000_000_000).await.is_empty());
    assert_eq!(
        serde_json::to_value(owner.index().rule("ttl20").unwrap()).unwrap(),
        oracle["after"][1]
    );
}
#[tokio::test]
async fn label_gc_etcd_same_wal_restart() {
    let mut fixture = label_restore_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let owner = RegionLabeler::load(
        StorageEndpoint::new(Arc::new(fixture.store().await)),
        ClusterPaths::new(200),
        2_000_000_000,
    )
    .await
    .unwrap();
    assert_eq!(owner.index().rules().count(), 1);
    assert_eq!(owner.index().rule("partial").unwrap().labels[0].key, "keep");
}
