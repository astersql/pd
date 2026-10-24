// Copyright 2026 AsterSQL.
use crate::*;
use prometheus::Registry;
use std::{
    future::pending,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{runtime::Handle, sync::Notify, time::timeout};

#[derive(Default)]
struct Probe {
    entered: Notify,
    active: AtomicUsize,
    maximum: AtomicUsize,
    calls: AtomicUsize,
    dropped: AtomicUsize,
}
struct InFlight(Arc<Probe>);
impl InFlight {
    fn enter(probe: &Arc<Probe>) -> Self {
        let active = probe.active.fetch_add(1, Ordering::SeqCst) + 1;
        probe.maximum.fetch_max(active, Ordering::SeqCst);
        probe.calls.fetch_add(1, Ordering::SeqCst);
        probe.entered.notify_one();
        Self(probe.clone())
    }
}
impl Drop for InFlight {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
    }
}
struct Blocked(Arc<Probe>);
impl Drop for Blocked {
    fn drop(&mut self) {
        self.0.dropped.fetch_add(1, Ordering::SeqCst);
    }
}
impl DownstreamSender for Blocked {
    async fn send(&mut self, _: SyncFrame) -> Result<(), String> {
        let _flight = InFlight::enter(&self.0);
        pending().await
    }
}
struct Attempt {
    probe: Arc<Probe>,
    retry: bool,
    complete: bool,
}
impl Drop for Attempt {
    fn drop(&mut self) {
        self.probe.dropped.fetch_add(1, Ordering::SeqCst);
    }
}
impl SyncAttempt for Attempt {
    async fn sync(&mut self) -> Result<SyncAttemptOutcome, String> {
        let _flight = InFlight::enter(&self.probe);
        if self.complete {
            return Ok(SyncAttemptOutcome::Complete);
        }
        if self.retry {
            return Err("connection interrupted".into());
        }
        pending().await
    }
}
fn setup() -> (Downstreams, SyncLifecycle, Registry) {
    let mut history = HistoryBuffer::with_config(8, 8, 1).unwrap();
    history.reset_with_index(HistoryIndex(10));
    let downstreams = Downstreams::new(history);
    let registry = Registry::new();
    let lifecycle = SyncLifecycle::new(
        Handle::current(),
        downstreams.clone(),
        SyncMetrics::register(&registry).unwrap(),
    );
    (downstreams, lifecycle, registry)
}
async fn entered(probe: &Probe) {
    timeout(Duration::from_secs(2), probe.entered.notified())
        .await
        .expect("must enter I/O");
}
async fn stop(lifecycle: &mut SyncLifecycle) -> Vec<SyncLifecycleError> {
    timeout(Duration::from_secs(2), lifecycle.stop())
        .await
        .expect("all workers must join")
}
fn released(probe: &Probe) {
    assert_eq!(probe.active.load(Ordering::SeqCst), 0);
    assert_eq!(probe.dropped.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn state_task_40_lifecycle_stop_joins_all_blocked_sends() {
    let (d, mut lifecycle, _) = setup();
    let probes: Vec<_> = (0..3).map(|_| Arc::new(Probe::default())).collect();
    for (i, probe) in probes.iter().enumerate() {
        lifecycle
            .start_downstream(
                i.to_string(),
                HistoryIndex(10),
                Blocked(probe.clone()),
                Duration::from_secs(60),
            )
            .unwrap();
    }
    d.broadcast(true);
    for probe in &probes {
        entered(probe).await;
    }
    assert!(stop(&mut lifecycle).await.is_empty());
    for probe in &probes {
        released(probe);
    }
    assert!(d.names().is_empty());
    assert!(stop(&mut lifecycle).await.is_empty());
    assert!(matches!(
        lifecycle.start_downstream(
            "late".into(),
            HistoryIndex(10),
            Blocked(Arc::default()),
            Duration::from_secs(1)
        ),
        Err(SyncLifecycleError::Closed)
    ));
}

#[tokio::test]
async fn state_task_40_lifecycle_timeout_closes_stream_without_parallel_send() {
    let (d, mut lifecycle, _) = setup();
    let probe = Arc::new(Probe::default());
    lifecycle
        .start_downstream(
            "blocked".into(),
            HistoryIndex(10),
            Blocked(probe.clone()),
            Duration::from_millis(10),
        )
        .unwrap();
    d.broadcast(true);
    entered(&probe).await;
    for _ in 0..20 {
        d.broadcast(true);
    }
    let exited = timeout(Duration::from_millis(250), async {
        while !d.names().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await;
    // Always reclaim before asserting, including the red run.
    let errors = stop(&mut lifecycle).await;
    assert!(
        exited.is_ok(),
        "send deadline must remove the blocked stream"
    );
    assert_eq!(errors.len(), 1, "timeout must be reported to the host");
    assert!(errors[0].to_string().contains("timeout"));
    assert_eq!(probe.maximum.load(Ordering::SeqCst), 1);
    assert_eq!(probe.calls.load(Ordering::SeqCst), 1);
    released(&probe);
}

#[tokio::test]
async fn state_task_40_lifecycle_cancel_before_request_poll() {
    let (_, mut lifecycle, _) = setup();
    let probe = Arc::new(Probe::default());
    lifecycle
        .start_client(
            Attempt {
                probe: probe.clone(),
                retry: false,
                complete: false,
            },
            Duration::from_secs(1),
        )
        .unwrap();
    // No yield: cancellation must win before the first request is polled.
    lifecycle.cancel();
    assert!(stop(&mut lifecycle).await.is_empty());
    assert_eq!(probe.calls.load(Ordering::SeqCst), 0);
    released(&probe);
}

#[tokio::test]
async fn state_task_40_lifecycle_cancel_while_waiting_first_request() {
    let (_, mut lifecycle, _) = setup();
    let probe = Arc::new(Probe::default());
    lifecycle
        .start_client(
            Attempt {
                probe: probe.clone(),
                retry: false,
                complete: false,
            },
            Duration::from_secs(1),
        )
        .unwrap();
    entered(&probe).await;
    assert!(stop(&mut lifecycle).await.is_empty());
    released(&probe);
}

#[tokio::test]
async fn state_task_40_lifecycle_cancel_during_retry_wait() {
    let (_, mut lifecycle, _) = setup();
    let probe = Arc::new(Probe::default());
    lifecycle
        .start_client(
            Attempt {
                probe: probe.clone(),
                retry: true,
                complete: false,
            },
            Duration::from_secs(3600),
        )
        .unwrap();
    entered(&probe).await;
    assert_eq!(probe.active.load(Ordering::SeqCst), 0);
    assert!(stop(&mut lifecycle).await.is_empty());
    assert_eq!(probe.calls.load(Ordering::SeqCst), 1);
    released(&probe);
}

#[tokio::test(start_paused = true)]
async fn state_task_40_lifecycle_retry_serially_then_cancel() {
    let (_, mut lifecycle, _) = setup();
    let probe = Arc::new(Probe::default());
    lifecycle
        .start_client(
            Attempt {
                probe: probe.clone(),
                retry: true,
                complete: false,
            },
            Duration::from_secs(1),
        )
        .unwrap();
    entered(&probe).await;
    tokio::time::advance(Duration::from_secs(1)).await;
    entered(&probe).await;
    assert!(stop(&mut lifecycle).await.is_empty());
    assert_eq!(probe.calls.load(Ordering::SeqCst), 2);
    assert_eq!(probe.maximum.load(Ordering::SeqCst), 1);
    released(&probe);
}

#[tokio::test]
async fn state_task_40_lifecycle_complete_releases_and_metrics_match_history() {
    let (d, mut lifecycle, registry) = setup();
    let probe = Arc::new(Probe::default());
    lifecycle
        .start_client(
            Attempt {
                probe: probe.clone(),
                retry: false,
                complete: true,
            },
            Duration::from_secs(1),
        )
        .unwrap();
    entered(&probe).await;
    released(&probe);
    d.append([Arc::new(astersql_pd_core::RegionInfo::new(
        astersql_pd_core::RegionMeta {
            id: 42,
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    ))])
    .unwrap();
    let families = registry.gather();
    assert_eq!(families.len(), 1);
    let family = &families[0];
    assert_eq!(family.name(), "pd_region_syncer_status");
    assert_eq!(family.help(), "Inner status of the region syncer.");
    let mut labels = std::collections::BTreeMap::new();
    for metric in family.get_metric() {
        assert_eq!(metric.get_label()[0].name(), "type");
        labels.insert(
            metric.get_label()[0].value(),
            metric.get_gauge().get_value() as u64,
        );
    }
    assert_eq!(
        labels,
        std::collections::BTreeMap::from([
            ("first_index", 0),
            ("last_index", 0),
            ("sync_index", 10)
        ])
    );
    assert!(SyncMetrics::register(&registry).is_err());
    assert!(stop(&mut lifecycle).await.is_empty());
}

#[tokio::test]
async fn state_task_40_lifecycle_worker_panic_still_joins_other_workers() {
    struct PanicAttempt(Arc<Probe>);
    impl SyncAttempt for PanicAttempt {
        async fn sync(&mut self) -> Result<SyncAttemptOutcome, String> {
            let _flight = InFlight::enter(&self.0);
            panic!("injected transport panic");
        }
    }
    let (d, mut lifecycle, _) = setup();
    let blocked = Arc::new(Probe::default());
    lifecycle
        .start_downstream(
            "blocked".into(),
            HistoryIndex(10),
            Blocked(blocked.clone()),
            Duration::from_secs(60),
        )
        .unwrap();
    d.broadcast(true);
    entered(&blocked).await;
    let panic_probe = Arc::new(Probe::default());
    lifecycle
        .start_client(PanicAttempt(panic_probe.clone()), Duration::from_secs(1))
        .unwrap();
    entered(&panic_probe).await;
    let errors = stop(&mut lifecycle).await;
    assert_eq!(errors.len(), 1);
    assert!(matches!(&errors[0], SyncLifecycleError::Worker(e) if e.is_panic()));
    released(&blocked);
    assert!(d.names().is_empty());
}

#[tokio::test(start_paused = true)]
async fn state_task_40_lifecycle_zero_deadline_uses_go_default() {
    let (d, mut lifecycle, _) = setup();
    let probe = Arc::new(Probe::default());
    lifecycle
        .start_downstream(
            "default".into(),
            HistoryIndex(10),
            Blocked(probe.clone()),
            Duration::ZERO,
        )
        .unwrap();
    d.broadcast(true);
    entered(&probe).await;
    tokio::time::advance(Duration::from_secs(9)).await;
    assert_eq!(probe.active.load(Ordering::SeqCst), 1);
    tokio::time::advance(Duration::from_secs(1)).await;
    tokio::task::yield_now().await;
    assert!(d.names().is_empty());
    let errors = stop(&mut lifecycle).await;
    assert_eq!(errors.len(), 1);
    released(&probe);
}

// A real watch attempt: no backend results are mocked. Cancellation must drop
// MetadataWatch and its underlying stream, including while first next is pending.
struct WatchAttempt {
    watch: MetadataWatch,
    probe: Arc<Probe>,
}
impl Drop for WatchAttempt {
    fn drop(&mut self) {
        self.probe.dropped.fetch_add(1, Ordering::SeqCst);
    }
}
impl SyncAttempt for WatchAttempt {
    async fn sync(&mut self) -> Result<SyncAttemptOutcome, String> {
        let _flight = InFlight::enter(&self.probe);
        self.watch
            .advance(&tokio_util::sync::CancellationToken::new())
            .await
            .map_err(|e| e.to_string())?;
        Ok(SyncAttemptOutcome::Retry)
    }
}
struct ApplyingAttempt {
    input: tokio::sync::mpsc::Receiver<SyncApplyResponse>,
    storage: Arc<astersql_pd_storage::Storage>,
    probe: Arc<Probe>,
    applied: Arc<Notify>,
}
impl Drop for ApplyingAttempt {
    fn drop(&mut self) {
        self.probe.dropped.fetch_add(1, Ordering::SeqCst);
    }
}
impl SyncAttempt for ApplyingAttempt {
    async fn sync(&mut self) -> Result<SyncAttemptOutcome, String> {
        let _flight = InFlight::enter(&self.probe);
        let mut client = ClientApply::default();
        let mut cluster = astersql_pd_core::BasicCluster::new();
        let mut history = HistoryBuffer::new(8).unwrap();
        let paths = astersql_pd_keypath::ClusterPaths::new(40);
        while let Some(frame) = self.input.recv().await {
            client
                .apply(frame, &mut cluster, &self.storage, &paths, &mut history)
                .await
                .map_err(|e| e.to_string())?;
            if client.is_running() {
                assert!(!client.full_syncing());
                assert_eq!(history.next_index(), HistoryIndex(11));
                assert_eq!(cluster.regions.get_region(42).unwrap().meta().id, 42);
                assert_eq!(
                    self.storage
                        .load_region(&paths, 42)
                        .await
                        .unwrap()
                        .unwrap()
                        .id,
                    42
                );
                self.applied.notify_one();
            }
        }
        Ok(SyncAttemptOutcome::Complete)
    }
}
async fn backend_lifecycle(store: Arc<dyn astersql_pd_storage_kv::MetaStore>) {
    use astersql_pd_storage_kv::{Base, Mutation};
    let (_, mut lifecycle, _) = setup();
    let registry = Registry::new();
    let metrics = SyncMetrics::register(&registry).unwrap();
    let mut history = HistoryBuffer::with_config(8, 8, 1).unwrap();
    history.attach_metrics(metrics);
    let mut history = PersistentHistory::load(history, store.as_ref())
        .await
        .unwrap();
    history
        .reset_with_index_and_persist(HistoryIndex(10))
        .await
        .unwrap();
    let record = Arc::new(astersql_pd_core::RegionInfo::new(
        astersql_pd_core::RegionMeta {
            id: 42,
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    ));
    assert_eq!(history.record(record).await.unwrap(), HistoryIndex(10));
    history.persist().await.unwrap();
    assert_eq!(
        Base::new(store.as_ref()).load(HISTORY_KEY).await.unwrap(),
        b"11"
    );
    let labels: std::collections::BTreeMap<_, _> = registry.gather()[0]
        .get_metric()
        .iter()
        .map(|m| {
            (
                m.get_label()[0].value().to_owned(),
                m.get_gauge().get_value() as u64,
            )
        })
        .collect();
    assert_eq!(
        labels,
        std::collections::BTreeMap::from([
            ("first_index".to_owned(), 10),
            ("last_index".to_owned(), 11),
            ("sync_index".to_owned(), 10)
        ])
    );
    history.reset_with_index(HistoryIndex(700));
    assert_eq!(
        registry.gather()[0]
            .get_metric()
            .iter()
            .find(|m| m.get_label()[0].value() == "sync_index")
            .unwrap()
            .get_gauge()
            .get_value(),
        10.0
    );
    let storage = Arc::new(astersql_pd_storage::Storage::new(store.clone()));
    let (input, receiver) = tokio::sync::mpsc::channel(2);
    let applying = Arc::new(Probe::default());
    let applied = Arc::new(Notify::new());
    lifecycle
        .start_client(
            ApplyingAttempt {
                input: receiver,
                storage: storage.clone(),
                probe: applying.clone(),
                applied: applied.clone(),
            },
            Duration::from_secs(1),
        )
        .unwrap();
    entered(&applying).await;
    input
        .send(SyncApplyResponse {
            start_index: HistoryIndex(10),
            regions: vec![astersql_pd_core::RegionMeta {
                id: 42,
                ..Default::default()
            }],
            ..Default::default()
        })
        .await
        .unwrap();
    timeout(Duration::from_secs(2), applied.notified())
        .await
        .expect("real follower application must complete");
    let cancel = tokio_util::sync::CancellationToken::new();
    store
        .transact(vec![], vec![Mutation::put(b"/lifecycle/a", b"ready")])
        .await
        .unwrap();
    let watch = MetadataWatch::load(store.clone(), b"/lifecycle/".to_vec(), 1, &cancel)
        .await
        .unwrap();
    assert_eq!(watch.entries().values().next().unwrap().value, b"ready");
    let probe = Arc::new(Probe::default());
    lifecycle
        .start_client(
            WatchAttempt {
                watch,
                probe: probe.clone(),
            },
            Duration::from_secs(3600),
        )
        .unwrap();
    entered(&probe).await;
    assert!(stop(&mut lifecycle).await.is_empty());
    released(&probe);
    released(&applying);
    assert_eq!(
        storage
            .load_region(&astersql_pd_keypath::ClusterPaths::new(40), 42)
            .await
            .unwrap()
            .unwrap()
            .id,
        42
    );
    assert!(input.send(SyncApplyResponse::default()).await.is_err());
    assert_eq!(history.history().next_index(), HistoryIndex(700));
    // Backend remains usable after the observation stream was released.
    store
        .transact(vec![], vec![Mutation::put(b"/lifecycle/a", b"after")])
        .await
        .unwrap();
    assert_eq!(
        Base::new(store.as_ref())
            .load(b"/lifecycle/a")
            .await
            .unwrap(),
        b"after"
    );
}
#[tokio::test]
async fn state_task_40_lifecycle_memory_watch_and_persistence() {
    backend_lifecycle(Arc::new(astersql_pd_storage_kv::MemoryStore::default())).await;
}
#[tokio::test]
async fn state_task_40_lifecycle_etcd_watch_and_persistence() {
    let fixture = crate::persist_fixture::Fixture::start();
    backend_lifecycle(Arc::new(fixture.store().await)).await;
}
