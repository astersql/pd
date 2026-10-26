// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_cache::{TTLString, TTLUint64, TtlDuration};
use astersql_pd_core::{
    BasicCluster, KeyRange, Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta,
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_hbstream::routing::{RouteEvent, StreamRouter};
use astersql_pd_schedule_scatter::ScatterBatchMetrics;
use astersql_pd_schedule_schedulers::{FactoryContext, JobTime};
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
fn setup(
    config: serde_json::Value,
    backend: Arc<dyn astersql_pd_storage_kv::MetaStore>,
) -> (
    Coordinator<ProviderSnapshot<()>>,
    Arc<TTLUint64<()>>,
    StreamRouter,
) {
    let mut cluster = BasicCluster::new();
    let peers: Vec<_> = (1..=3)
        .map(|id| Peer {
            id,
            store_id: id,
            ..Default::default()
        })
        .collect();
    let region = Arc::new(RegionInfo::new(
        RegionMeta {
            id: 1,
            peers: peers.clone(),
            key_range: KeyRange {
                start_key: b"a".to_vec(),
                end_key: b"b".to_vec(),
            },
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![],
    ));
    let (origin, overlaps, changed) = cluster.regions.set_region(region.clone());
    cluster
        .regions
        .update_sub_tree(region, origin.as_deref(), &overlaps, changed);
    let cluster = Arc::new(cluster);
    for id in 1..=3 {
        cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        })));
    }
    let routing_stores = Arc::new(astersql_pd_core::StoresInfo::new());
    for store in cluster.stores.get_stores() {
        routing_stores.put_store(store);
    }
    let (heartbeat, router) = StreamRouter::new(routing_stores);
    let pending = Arc::new(TTLUint64::new(
        tokio::runtime::Handle::current(),
        CancellationToken::new(),
        Duration::from_secs(60),
        TtlDuration::from_nanos(180_000_000_000),
    ));
    let c = Coordinator::new(CoordinatorInputs {
        cluster,
        config: Arc::new({
            let mut conf = ProviderSnapshot::<()>::default();
            conf.schedule.scheduler_max_waiting_operator = 10;
            conf.replication.enable_placement_rules = false;
            conf.replication.max_replicas = 3;
            conf
        }),
        heartbeat,
        pending: pending.clone(),
        factory: FactoryContext::new(
            Arc::new(StorageEndpoint::new(backend)),
            ClusterPaths::new(114),
            JobTime(0),
        ),
        scheduler_configuration: config,
        scatter_metrics: ScatterBatchMetrics::new(&prometheus::Registry::new()).unwrap(),
        start: Duration::ZERO,
        now_unix_nanos: 0,
        split_merge_interval_nanos: 0,
        total_regions: Box::new(|| Ok(0)),
        unix_seconds: Arc::new(|| 0),
    });
    (c, pending, router)
}
use astersql_pd_schedule_affinity::manager::Manager;
use astersql_pd_schedule_checker::*;
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_schedule_operator::{
    ControllerAdmission, OpKind, OpStep, Operator, TransferLeader,
};
use astersql_pd_schedule_placement::RuleManager;
use async_trait::async_trait;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
};
use tokio::sync::{Notify, Semaphore};
struct Host {
    cluster: Arc<BasicCluster>,
    checker: Arc<std::sync::Mutex<CheckerDispatcher>>,
    operators: Arc<ControllerAdmission>,
    config: Arc<ProviderSnapshot<()>>,
    rules: RefCell<RuleManager>,
    affinity: RefCell<Manager>,
    labels: RegionLabeler,
    storage: Arc<StorageEndpoint>,
    paths: ClusterPaths,
    split: RefCell<SplitScatterRetryController>,
    labels_stats: RefCell<astersql_pd_statistics::LabelStatistics>,
    scanned: Cell<usize>,
    seen_pending: Cell<bool>,
    cleared: Cell<bool>,
    block: Cell<bool>,
    entered: Notify,
    release: Semaphore,
    invalid: Cell<bool>,
}
impl Host {
    async fn new(c: &Coordinator<ProviderSnapshot<()>>) -> Self {
        let ctx = c.get_schedulers_controller().context();
        Self {
            cluster: c.get_cluster().clone(),
            checker: c.get_checker_controller().clone(),
            operators: c.operator_handle(),
            config: c.get_config().clone(),
            rules: RefCell::new(RuleManager::new(ctx.storage.clone(), ctx.paths.clone())),
            affinity: RefCell::new(Manager::default()),
            labels: RegionLabeler::load(
                StorageEndpoint::new(Arc::new(MemoryStore::default())),
                ctx.paths.clone(),
                0,
            )
            .await
            .unwrap(),
            storage: ctx.storage.clone(),
            paths: ctx.paths.clone(),
            split: RefCell::new(SplitScatterRetryController::default()),
            labels_stats: RefCell::new(astersql_pd_statistics::LabelStatistics::new()),
            scanned: Cell::new(0),
            seen_pending: Cell::new(false),
            cleared: Cell::new(false),
            block: Cell::new(false),
            entered: Notify::new(),
            release: Semaphore::new(0),
            invalid: Cell::new(false),
        }
    }
}
#[async_trait(?Send)]
impl PatrolHost for Host {
    type Error = &'static str;
    async fn config(&self) -> PatrolConfig {
        PatrolConfig {
            interval: if self.invalid.get() {
                Duration::ZERO
            } else {
                Duration::from_millis(10)
            },
            workers: 1,
        }
    }
    async fn region_count(&self) -> usize {
        self.cluster.regions.tree_len()
    }
    async fn scheduling_halted(&self) -> bool {
        self.config.schedule.halt_scheduling
    }
    async fn priority_ids(&self) -> Vec<u64> {
        self.checker
            .lock()
            .unwrap()
            .priority
            .get_priority_regions(&*self.config, 0)
    }
    async fn pending_ids(&self) -> Vec<u64> {
        let ids = self.checker.lock().unwrap().pending_processed_regions();
        self.seen_pending
            .set(self.seen_pending.get() || ids.contains(&1));
        ids
    }
    async fn region(&self, id: u64) -> Option<Arc<RegionInfo>> {
        self.cluster.regions.get_region(id)
    }
    async fn remove_priority(&self, id: u64) {
        self.checker
            .lock()
            .unwrap()
            .priority
            .remove_priority_region(id);
    }
    async fn check(&self, r: Arc<RegionInfo>, source: PatrolSource) -> Result<(), Self::Error> {
        if source == PatrolSource::Scan {
            self.scanned.set(self.scanned.get() + 1);
            if self.block.get() {
                self.entered.notify_one();
                self.release.acquire().await.unwrap().forget();
            }
        }
        let hot = BTreeSet::new();
        let ctx = CheckerDispatchContext {
            affinity: AffinityContext {
                rules: RuleMissingContext {
                    replica: ReplicaFaultContext {
                        config: &*self.config,
                        stores: &self.cluster.stores,
                        rules: &[],
                        now_unix_nanos: 0,
                        supports_conf_change_v2: false,
                    },
                    witness_enabled: false,
                },
                labels: &self.labels,
                storage: &self.storage,
                paths: &self.paths,
                region_exists: true,
            },
            cluster: &self.cluster,
            operators: &self.operators,
            hot_regions: &hot,
        };
        let _ = self
            .checker
            .lock()
            .unwrap()
            .check_patrol_region(
                &ctx,
                &mut self.rules.borrow_mut(),
                &mut self.affinity.borrow_mut(),
                &r,
                source,
                &mut || Ok(100),
            )
            .await;
        Ok(())
    }
    async fn dispatch_split_scatter(&self) {
        let stores = self.cluster.stores.get_stores();
        let mut rules = self.rules.borrow_mut();
        let replication = astersql_pd_schedule_filter::RegionReplicationContext {
            conf: &*self.config,
            stores: &stores,
            manager: RefCell::new(&mut *rules),
            now: 0,
        };
        let errors = self
            .split
            .borrow_mut()
            .dispatch(
                &self.cluster,
                &self.operators,
                &replication,
                Some(&self.labels),
                Duration::ZERO,
                1,
                |_, _, _, _| panic!("no pending split results"),
            )
            .await;
        assert!(errors.is_empty());
    }
    async fn clear_split_scatter(&self) {
        self.split.borrow_mut().clear();
        self.cleared.set(true);
    }
    async fn scan(&self, start: &[u8], limit: usize) -> Vec<Arc<RegionInfo>> {
        self.cluster
            .regions
            .scan_regions(start, &[], limit as isize)
    }
    async fn update_label_stats(&self, regions: &[Arc<RegionInfo>]) {
        for region in regions {
            self.labels_stats.borrow_mut().observe(region, &[], &[]);
        }
    }
}
fn ranges() -> (SuspectRanges, Arc<TTLString<[Vec<u8>; 2]>>) {
    let cache = Arc::new(TTLString::new(
        tokio::runtime::Handle::current(),
        CancellationToken::new(),
        Duration::from_secs(60),
        TtlDuration::from_nanos(180_000_000_000),
    ));
    (SuspectRanges::new(cache.clone()), cache)
}
fn inputs<'a>(
    host: &'a Host,
    ranges: &'a SuspectRanges,
    token: CancellationToken,
    kv2: bool,
) -> CoordinatorLifecycleInputs<'a, Host> {
    CoordinatorLifecycleInputs {
        host,
        suspect_ranges: ranges,
        cancellation: token,
        elapsed: &|| Duration::from_secs(301),
        unix_nanos: &|| 0,
        push_time: &|| tokio::time::Instant::now().into_std(),
        raft_kv2: if kv2 { &|| true } else { &|| false },
        cluster_id: 115,
        conf_change_v2: false,
        waiting_random: &|| 0.5,
        scheduler_workers: Box::new(|_| vec![]),
    }
}
#[tokio::test(start_paused = true)]
async fn task_115_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_115.json"
    ))
    .unwrap();
    for row in fixture["cases"].as_array().unwrap() {
        let backend = Arc::new(FaultStore::default());
        let cfg = if row["existing"] == true {
            serde_json::json!({"schedule":{"schedulers-v2":[{"type":"evict-slow-trend","args":[]}]}})
        } else {
            serde_json::json!({})
        };
        let (mut c, pending, mut router) = setup(cfg, backend.clone());
        let host = Host::new(&c).await;
        let (tx, mut rx) = tokio::sync::mpsc::channel(128);
        c.get_heartbeat_streams()
            .bind(1, Arc::new(tx))
            .await
            .unwrap();
        assert!(matches!(router.step().await, Some(RouteEvent::Bound(1))));
        let (ranges, cache) = ranges();
        ranges.add(b"a", b"b");
        let op = Arc::new(Operator::new(
            "push",
            "",
            1,
            None,
            OpKind::LEADER,
            1,
            vec![OpStep::TransferLeader(TransferLeader {
                from_store: 1,
                to_store: 2,
                to_stores: vec![],
            })],
        ));
        c.get_operator_controller()
            .add_operators(c.get_cluster(), &[op])
            .unwrap();
        let token = CancellationToken::new();
        let run = c
            .run(inputs(
                &host,
                &ranges,
                token.clone(),
                row["kv2"].as_bool().unwrap(),
            ))
            .await
            .expect("coordinator starts");
        let stop = run.cancellation();
        let (errors, ()) = tokio::join!(run.wait(), async {
            for tick in 0..250 {
                if tick == 2 && row["fail_once"] == true {
                    backend
                        .writes_until_failure
                        .store(1, std::sync::atomic::Ordering::SeqCst);
                }
                tokio::time::advance(Duration::from_millis(500)).await;
                tokio::task::yield_now().await;
            }
            assert!(matches!(
                tokio::time::timeout(Duration::from_secs(1), router.step())
                    .await
                    .unwrap(),
                Some(RouteEvent::Sent(1))
            ));
            let command = rx.recv().await.unwrap();
            assert_eq!(command.region_id, 1);
            assert_eq!(
                command
                    .operation
                    .transfer_leader
                    .unwrap()
                    .peer
                    .unwrap()
                    .store_id,
                2
            );
            stop.cancel();
            stop.cancel();
        });
        assert!(errors.is_empty(), "{errors:?}");
        assert!(host.scanned.get() > 0);
        assert!(host.cleared.get());
        assert!(host.seen_pending.get());
        assert_eq!(
            c.get_schedulers_controller()
                .names()
                .contains(&"evict-slow-trend-scheduler".into()),
            row["slow"].as_bool().unwrap()
        );
        assert_eq!(
            !c.get_schedulers_controller().cleanup_errors.is_empty(),
            row["fail_once"] == true
        );
        cache.shutdown().await;
        pending.shutdown().await;
    }
}
#[tokio::test(start_paused = true)]
async fn task_115_stop_joins_inflight() {
    let (mut c, pending, _router) = setup(serde_json::json!({}), Arc::new(MemoryStore::default()));
    let host = Host::new(&c).await;
    host.block.set(true);
    let (ranges, cache) = ranges();
    let run = c
        .run(inputs(&host, &ranges, CancellationToken::new(), false))
        .await
        .unwrap();
    let stop = run.cancellation();
    let finished = Cell::new(false);
    let (errors, ()) = tokio::join!(
        async {
            let e = run.wait().await;
            finished.set(true);
            e
        },
        async {
            host.entered.notified().await;
            stop.cancel();
            stop.cancel();
            tokio::time::advance(Duration::from_millis(100)).await;
            assert!(!finished.get());
            assert!(!host.cleared.get());
            host.release.add_permits(1);
        }
    );
    assert!(errors.is_empty());
    assert!(finished.get());
    assert!(host.cleared.get());
    cache.shutdown().await;
    pending.shutdown().await;
}

use astersql_pd_storage_kv::{
    Commit, Compare, Lease, MetaWatch, Mutation, RawTxnOp, RawTxnResponse, Read, Snapshot,
    StoreError,
};
#[derive(Default)]
struct FaultStore {
    store: MemoryStore,
    fail: std::sync::atomic::AtomicBool,
    writes_until_failure: std::sync::atomic::AtomicUsize,
    reads: std::sync::atomic::AtomicUsize,
    read_failures: std::sync::atomic::AtomicUsize,
}
#[async_trait::async_trait]
impl astersql_pd_storage_kv::MetaStore for FaultStore {
    async fn read(&self, r: Read) -> astersql_pd_storage_kv::Result<Snapshot> {
        use std::sync::atomic::Ordering::SeqCst;
        self.reads.fetch_add(1, SeqCst);
        if self
            .read_failures
            .try_update(SeqCst, SeqCst, |n| n.checked_sub(1))
            .is_ok()
        {
            return Err(StoreError::Unavailable);
        }
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(StoreError::Unavailable);
        }
        self.store.read(r).await
    }
    async fn transact(
        &self,
        c: Vec<Compare>,
        w: Vec<Mutation>,
    ) -> astersql_pd_storage_kv::Result<Commit> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(StoreError::Unavailable);
        }
        use std::sync::atomic::Ordering;
        if self
            .writes_until_failure
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            .is_ok_and(|n| n == 1)
        {
            return Err(StoreError::Unavailable);
        }
        self.store.transact(c, w).await
    }
    async fn transact_branches(
        &self,
        c: Vec<Compare>,
        t: Vec<Mutation>,
        e: Vec<Mutation>,
    ) -> astersql_pd_storage_kv::Result<Commit> {
        self.store.transact_branches(c, t, e).await
    }
    async fn transact_raw(
        &self,
        c: Vec<Compare>,
        t: Vec<RawTxnOp>,
        e: Vec<RawTxnOp>,
    ) -> astersql_pd_storage_kv::Result<RawTxnResponse> {
        self.store.transact_raw(c, t, e).await
    }
    async fn watch(
        &self,
        p: Vec<u8>,
        r: i64,
    ) -> astersql_pd_storage_kv::Result<Box<dyn MetaWatch>> {
        self.store.watch(p, r).await
    }
    async fn grant(&self, t: i64) -> astersql_pd_storage_kv::Result<Lease> {
        self.store.grant(t).await
    }
    async fn keep_alive(&self, l: i64) -> astersql_pd_storage_kv::Result<Lease> {
        self.store.keep_alive(l).await
    }
    async fn revoke(&self, l: i64) -> astersql_pd_storage_kv::Result<()> {
        self.store.revoke(l).await
    }
    async fn ttl(&self, l: i64) -> astersql_pd_storage_kv::Result<i64> {
        self.store.ttl(l).await
    }
    async fn compact(&self, r: i64) -> astersql_pd_storage_kv::Result<()> {
        self.store.compact(r).await
    }
}

#[tokio::test(start_paused = true)]
async fn task_115_prepare_and_partial_startup_faults() {
    for pre_cancelled in [false, true] {
        let backend = Arc::new(FaultStore::default());
        let (mut c, pending, _router) = setup(serde_json::json!({}), backend.clone());
        let host = Host::new(&c).await;
        let (ranges, cache) = ranges();
        let token = CancellationToken::new();
        if pre_cancelled {
            token.cancel();
        }
        let mut input = inputs(&host, &ranges, token.clone(), false);
        input.elapsed = &|| Duration::ZERO;
        let (result, ()) = tokio::join!(async { c.run(input).await.map(|_| ()) }, async {
            tokio::time::sleep(Duration::from_secs(4)).await;
            assert_eq!(backend.reads.load(std::sync::atomic::Ordering::SeqCst), 0);
            assert_eq!(host.scanned.get(), 0);
            token.cancel();
        });
        assert!(matches!(result, Err(CoordinatorLifecycleError::Cancelled)));
        assert!(!c.are_schedulers_initialized());
        cache.shutdown().await;
        pending.shutdown().await;
    }
    let backend = Arc::new(FaultStore::default());
    let (mut c, pending, _router) = setup(
        serde_json::json!({"schedule":{"schedulers-v2":[{"type":"grant-leader","args":["1"]},{"type":"balance-leader","args":[]}]}}),
        backend.clone(),
    );
    let host = Host::new(&c).await;
    let (ranges, cache) = ranges();
    backend
        .read_failures
        .store(10, std::sync::atomic::Ordering::SeqCst);
    let result = c
        .run(inputs(&host, &ranges, CancellationToken::new(), false))
        .await
        .map(|_| ());
    assert!(matches!(result, Err(CoordinatorLifecycleError::Restore(_))));
    assert_eq!(backend.reads.load(std::sync::atomic::Ordering::SeqCst), 10);
    assert!(!c.are_schedulers_initialized());
    assert_eq!(host.scanned.get(), 0);
    // A policy failure is nonfatal: another valid scheduler still starts.
    backend
        .writes_until_failure
        .store(1, std::sync::atomic::Ordering::SeqCst);
    let token = CancellationToken::new();
    let run = c
        .run(inputs(&host, &ranges, token.clone(), false))
        .await
        .unwrap();
    let (errors, ()) = tokio::join!(run.wait(), async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        token.cancel();
    });
    assert!(errors.is_empty());
    assert!(c.are_schedulers_initialized());
    assert!(host.scanned.get() > 0);
    assert!(
        c.get_cluster()
            .stores
            .get_store(1)
            .unwrap()
            .allow_leader_transfer_out()
    );
    assert_eq!(
        c.get_schedulers_controller().names(),
        vec!["balance-leader-scheduler"]
    );
    cache.shutdown().await;
    pending.shutdown().await;
}
#[tokio::test(start_paused = true)]
async fn task_115_run_until_stop_joins_real_scheduler() {
    use astersql_pd_schedule_schedulers::{
        EvictLeaderConfig, EvictLeaderScheduler, EvictLoopScheduler, LoopControl,
        ScheduleController,
    };
    let (mut c, pending, _router) = setup(serde_json::json!({}), Arc::new(MemoryStore::default()));
    let host = Host::new(&c).await;
    host.invalid.set(true);
    let (ranges, cache) = ranges();
    let policy = EvictLeaderScheduler::new(
        EvictLeaderConfig::new([(1, vec![KeyRange::default()])].into(), 3).unwrap(),
    );
    policy.prepare(&*host.cluster).unwrap();
    assert!(
        !host
            .cluster
            .stores
            .get_store(1)
            .unwrap()
            .allow_leader_transfer_in()
    );
    let mut config = ProviderSnapshot::<u64>::default();
    config.schedule.leader_schedule_limit = 4;
    let scheduler = EvictLoopScheduler {
        policy,
        config,
        rules: RuleManager::new(host.storage.clone(), host.paths.clone()),
        now_nanos: || 0,
        random_index: |_| 0,
        admission: host.operators.clone(),
        faults: vec![],
    };
    let cluster = host.cluster.clone();
    let operators = host.operators.clone();
    let mut input = inputs(&host, &ranges, CancellationToken::new(), false);
    input.scheduler_workers = Box::new(move |token| {
        vec![Box::pin(async move {
            let out =
                ScheduleController::new(scheduler, cluster, LoopControl::from_parent(&token), || 0)
                    .run(operators)
                    .await;
            assert!(out.cleanup.is_ok());
            assert!(out.errors.is_empty());
            Ok(())
        })]
    });
    let errors = c.run_until_stop(input).await.unwrap();
    assert!(matches!(
        errors.as_slice(),
        [CoordinatorLifecycleError::Patrol(
            PatrolError::InvalidInterval
        )]
    ));
    assert!(
        host.cluster
            .stores
            .get_store(1)
            .unwrap()
            .allow_leader_transfer_in()
    );
    cache.shutdown().await;
    pending.shutdown().await;
}
#[tokio::test(start_paused = true)]
async fn task_115_routing_fault_stops_workers() {
    let (mut c, pending, router) = setup(serde_json::json!({}), Arc::new(MemoryStore::default()));
    let host = Host::new(&c).await;
    let (ranges, cache) = ranges();
    drop(router);
    let op = Arc::new(Operator::new(
        "push",
        "",
        1,
        None,
        OpKind::LEADER,
        1,
        vec![OpStep::TransferLeader(TransferLeader {
            from_store: 1,
            to_store: 2,
            to_stores: vec![],
        })],
    ));
    c.get_operator_controller()
        .add_operators(c.get_cluster(), &[op])
        .unwrap();
    let errors = c
        .run_until_stop(inputs(&host, &ranges, CancellationToken::new(), false))
        .await
        .unwrap();
    assert!(matches!(
        errors.as_slice(),
        [CoordinatorLifecycleError::Routing(_)]
    ));
    assert!(host.cleared.get());
    assert!(c.get_operator_controller().get_operator(1).is_some());
    cache.shutdown().await;
    pending.shutdown().await;
}
#[tokio::test(start_paused = true)]
async fn task_115_repeated_stop_cleans_registered_policies() {
    let (mut c, pending, _router) = setup(
        serde_json::json!({"schedule":{"schedulers-v2":[{"type":"evict-leader","args":["1"]}]}}),
        Arc::new(MemoryStore::default()),
    );
    let host = Host::new(&c).await;
    let (ranges, cache) = ranges();
    let run = c
        .run(inputs(&host, &ranges, CancellationToken::new(), false))
        .await
        .unwrap();
    assert!(
        !host
            .cluster
            .stores
            .get_store(1)
            .unwrap()
            .allow_leader_transfer_in()
    );
    run.stop();
    run.stop();
    assert!(run.wait().await.is_empty());
    assert!(
        host.cluster
            .stores
            .get_store(1)
            .unwrap()
            .allow_leader_transfer_in()
    );
    assert_eq!(
        c.get_schedulers_controller().names(),
        vec!["evict-leader-scheduler"]
    );
    let ctx = c.get_schedulers_controller().context();
    assert!(
        !ctx.storage
            .load_scheduler_config(&ctx.paths, "evict-leader-scheduler")
            .await
            .unwrap()
            .is_empty()
    );
    cache.shutdown().await;
    pending.shutdown().await;
}
