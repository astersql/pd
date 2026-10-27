// Copyright 2026 AsterSQL.
use astersql_pd_cache::{TTLString, TTLUint64, TtlDuration};
use astersql_pd_core::{
    BasicCluster, KeyRange, Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta,
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule::*;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_hbstream::routing::StreamRouter;
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
            ClusterPaths::new(136),
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
        cluster_id: 136,
        conf_change_v2: false,
        waiting_random: &|| 0.5,
        scheduler_workers: Box::new(|_| vec![]),
    }
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

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_136.json"
    ))
    .unwrap()
}
fn operation(c: &Coordinator<ProviderSnapshot<()>>) -> Arc<Operator> {
    let r = c.get_cluster().regions.get_region(1).unwrap();
    Arc::new(Operator::new(
        "restart",
        "",
        1,
        r.meta().epoch,
        OpKind::LEADER,
        1,
        vec![OpStep::TransferLeader(TransferLeader {
            from_store: 1,
            to_store: 3,
            to_stores: vec![],
        })],
    ))
}
async fn decision(c: &Coordinator<ProviderSnapshot<()>>, expected: u64) {
    use astersql_pd_schedule_schedulers::{BuiltinScheduler, GrantLeaderContext};
    let ctx = c.get_schedulers_controller().context();
    let mut rules = RuleManager::new(ctx.storage.clone(), ctx.paths.clone());
    rules
        .initialize(3, &[], "", false, &**c.get_config(), &[])
        .await
        .unwrap();
    let policy = c
        .get_schedulers_controller()
        .handler("grant-leader-scheduler")
        .unwrap();
    let BuiltinScheduler::GrantLeader(policy) = &policy.policy else {
        panic!("grant policy")
    };
    let result = policy
        .schedule(&mut GrantLeaderContext {
            cluster: &**c.get_cluster(),
            config: &**c.get_config(),
            rules: &mut rules,
            now_unix_nanos: 0,
            random_index: &mut |_| 0,
            shuffle: &mut |_| {},
        })
        .await;
    assert!(result.errors.is_empty());
    assert_eq!(result.operators.len(), 1);
    let op = &result.operators[0];
    assert_eq!(op.region_id(), 1);
    assert_eq!(op.kind(), OpKind::LEADER);
    let OpStep::TransferLeader(step) = op.step(0).unwrap() else {
        panic!("transfer")
    };
    assert_eq!((step.from_store, step.to_store), (1, expected));
}
#[tokio::test]
async fn task_136_behavior_cases() {
    use astersql_pd_schedule_operator::STARTED;
    use astersql_pd_schedule_schedulers::HotLoadProjection;
    use astersql_pd_statistics::{Influence, summary_store_infos};
    let f = fixture();
    for case in f["cases"].as_array().unwrap() {
        let need_run = case["need_run"].as_bool().unwrap();
        let backend = Arc::new(MemoryStore::default());
        let (mut old, pending, _router) = setup(f["configuration"].clone(), backend.clone());
        let ctx = old.get_schedulers_controller().context();
        ctx.storage
            .save_scheduler_config(
                &ctx.paths,
                "grant-leader-scheduler",
                &serde_json::to_vec(&f["persisted_grant"]).unwrap(),
            )
            .await
            .unwrap();
        old.init_schedulers(need_run, &CancellationToken::new())
            .await
            .unwrap();
        decision(&old, f["expected"]["target"].as_u64().unwrap()).await;
        if need_run {
            old.get_schedulers_controller_mut()
                .pause_or_resume("grant-leader-scheduler", 600, 100)
                .unwrap();
            assert!(
                old.get_schedulers_controller()
                    .is_paused("grant-leader-scheduler", 101)
                    .unwrap()
            );
        }
        let op = operation(&old);
        old.get_operator_controller()
            .add_operators(old.get_cluster(), &[op.clone()])
            .unwrap();
        assert_eq!(op.status(), STARTED);
        let influence = old
            .get_operator_controller()
            .get_op_influence(Some(old.get_cluster()), &[]);
        assert_eq!(
            influence.get_store_influence(1).leader_count,
            f["expected"]["old_leader_deltas"][0].as_i64().unwrap()
        );
        assert_eq!(
            influence.get_store_influence(3).leader_count,
            f["expected"]["old_leader_deltas"][1].as_i64().unwrap()
        );
        // The hot scheduler's canonical pending state is explicitly host-owned in Rust.
        let mut hot = HotLoadProjection::default();
        hot.try_add_pending_influence(
            op.clone(),
            vec![1],
            3,
            Influence {
                loads: vec![10.; 8],
                hot_peer_count: 1.,
            },
            Duration::from_secs(30),
        )
        .unwrap();
        let stores: Vec<_> = old
            .get_cluster()
            .stores
            .get_stores()
            .iter()
            .map(|s| s.as_ref().clone())
            .collect();
        let mut summary = summary_store_infos(&stores);
        hot.summary_pending_influence(&mut summary, None, &Now);
        assert_eq!(summary[&1].pending_sum.as_ref().unwrap().loads[0], -10.);
        assert_eq!(summary[&3].pending_sum.as_ref().unwrap().loads[0], 10.);
        let ctx = old.get_schedulers_controller().context();
        let config: serde_json::Value = ctx.storage.load_config(&ctx.paths).await.unwrap().unwrap();
        let before = ctx
            .storage
            .load_all_scheduler_configs(&ctx.paths)
            .await
            .unwrap();
        old.get_schedulers_controller_mut().clean_registered().await;
        assert_eq!(op.status(), STARTED, "stop is not synthetic success/cancel");
        drop(old);
        drop(hot);
        pending.shutdown().await;
        let (mut new, new_pending, _new_router) = setup(config.clone(), backend);
        new.init_schedulers(need_run, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(new.get_schedulers_controller().configuration(), &config);
        assert_eq!(
            config["schedule"]["schedulers-v2"][1]["disable"],
            f["expected"]["disabled"]
        );
        assert!(
            new.get_schedulers_controller()
                .handler("balance-leader-scheduler")
                .is_none()
        );
        if need_run {
            assert_eq!(
                new.get_schedulers_controller()
                    .is_paused("grant-leader-scheduler", 101)
                    .unwrap(),
                f["expected"]["pause_restored"]
            );
            assert_eq!(
                new.get_schedulers_controller()
                    .delay("grant-leader-scheduler", 101)
                    .unwrap(),
                (0, 0)
            );
        } else {
            assert!(new.get_schedulers_controller().names().is_empty());
        }
        assert_eq!(
            new.get_operator_controller().operators().len() as u64,
            f["expected"]["new_operator_count"].as_u64().unwrap()
        );
        assert!(
            new.get_operator_controller()
                .get_op_influence(Some(new.get_cluster()), &[])
                .snapshot()
                .is_empty()
        );
        let mut new_hot = HotLoadProjection::default();
        let mut summary = summary_store_infos(&stores);
        new_hot.summary_pending_influence(&mut summary, None, &Now);
        assert!(summary.values().all(|s| s.pending_sum.is_none()));
        assert_eq!(
            new_hot.region_pendings.len() as u64,
            f["expected"]["new_pending_count"].as_u64().unwrap()
        );
        let ctx = new.get_schedulers_controller().context();
        assert_eq!(
            ctx.storage
                .load_all_scheduler_configs(&ctx.paths)
                .await
                .unwrap(),
            before
        );
        decision(&new, 3).await;
        let fresh = operation(&new);
        new.get_operator_controller()
            .add_operators(new.get_cluster(), &[fresh.clone()])
            .unwrap();
        assert!(!Arc::ptr_eq(&fresh, &op));
        assert_eq!(fresh.status(), STARTED);
        new.get_schedulers_controller_mut().clean_registered().await;
        new_pending.shutdown().await;
        println!("task136 restart case={} passed", case["mode"]);
    }
}
#[tokio::test]
async fn task_136_persistence_failure_recovery() {
    use std::sync::atomic::Ordering::SeqCst;
    let f = fixture();
    // Independent scheduler write fails: skip it, undo Prepare, persist cleaned list.
    let backend = Arc::new(FaultStore::default());
    let cfg =
        serde_json::json!({"schedule":{"schedulers-v2":[{"type":"grant-leader","args":["3"]}]}});
    let (mut c, pending, _router) = setup(cfg.clone(), backend.clone());
    backend.writes_until_failure.store(1, SeqCst);
    let report = c
        .init_schedulers(true, &CancellationToken::new())
        .await
        .unwrap();
    assert!(report.valid_configs.is_empty());
    assert!(!report.skipped.is_empty());
    assert!(c.get_schedulers_controller().handler_names().is_empty());
    assert!(
        c.get_cluster()
            .stores
            .get_store(3)
            .unwrap()
            .allow_leader_transfer_out()
    );
    let ctx = c.get_schedulers_controller().context();
    let saved: serde_json::Value = ctx.storage.load_config(&ctx.paths).await.unwrap().unwrap();
    assert_eq!(saved["schedule"]["schedulers-v2"], serde_json::json!([]));
    drop(c);
    pending.shutdown().await;
    let (mut c, pending, _router) = setup(saved, backend.clone());
    c.init_schedulers(true, &CancellationToken::new())
        .await
        .unwrap();
    assert!(c.get_schedulers_controller().handler_names().is_empty());
    drop(c);
    pending.shutdown().await;
    // Retry needs an explicit configuration request; a cleaned list is not resurrected.
    let (mut c, pending, _router) = setup(cfg, backend.clone());
    c.init_schedulers(true, &CancellationToken::new())
        .await
        .unwrap();
    decision(&c, 3).await;
    c.get_schedulers_controller_mut().clean_registered().await;
    drop(c);
    pending.shutdown().await;
    // Whole configuration write fails after the independent write; init is nonfatal.
    let (mut c, pending, _router) = setup(f["configuration"].clone(), backend.clone());
    backend.writes_until_failure.store(2, SeqCst);
    let report = c
        .init_schedulers(true, &CancellationToken::new())
        .await
        .unwrap();
    assert!(report.persistence_error.is_some());
    assert!(c.are_schedulers_initialized());
    let ctx = c.get_schedulers_controller().context();
    let stale_config: serde_json::Value =
        ctx.storage.load_config(&ctx.paths).await.unwrap().unwrap();
    assert_ne!(&stale_config, c.get_schedulers_controller().configuration());
    c.get_schedulers_controller_mut().clean_registered().await;
    drop(c);
    pending.shutdown().await;
    let (mut c, pending, _router) = setup(stale_config, backend.clone());
    // Repeated storage load errors are fatal to this attempt and do not initialize.
    backend.fail.store(true, SeqCst);
    assert!(
        c.init_schedulers(true, &CancellationToken::new())
            .await
            .is_err()
    );
    assert!(!c.are_schedulers_initialized());
    assert!(c.get_schedulers_controller().handler_names().is_empty());
    backend.fail.store(false, SeqCst);
    assert!(
        c.init_schedulers(true, &CancellationToken::new())
            .await
            .unwrap()
            .persistence_error
            .is_none()
    );
    decision(&c, 3).await;
    c.get_schedulers_controller_mut().clean_registered().await;
    pending.shutdown().await;
    println!("task136 fault cases=5 passed");
}
#[tokio::test(start_paused = true)]
async fn task_136_stop_join_before_replacement() {
    use astersql_pd_schedule_operator::STARTED;
    let backend = Arc::new(MemoryStore::default());
    let configuration =
        serde_json::json!({"schedule":{"schedulers-v2":[{"type":"evict-leader","args":["1"]}]}});
    let (mut old, pending, _router) = setup(configuration, backend.clone());
    let op = operation(&old);
    old.get_operator_controller()
        .add_operators(old.get_cluster(), &[op.clone()])
        .unwrap();
    let host = Host::new(&old).await;
    host.block.set(true);
    let (ranges, cache) = ranges();
    let run = old
        .run(inputs(&host, &ranges, CancellationToken::new(), false))
        .await
        .unwrap();
    let stop = run.cancellation();
    let finished = Cell::new(false);
    let (errors, ()) = tokio::join!(
        async {
            let errors = run.wait().await;
            finished.set(true);
            errors
        },
        async {
            host.entered.notified().await;
            stop.cancel();
            stop.cancel();
            tokio::time::advance(Duration::from_millis(100)).await;
            assert!(!finished.get());
            assert!(!host.cleared.get());
            assert!(
                !host
                    .cluster
                    .stores
                    .get_store(1)
                    .unwrap()
                    .allow_leader_transfer_in()
            );
            host.release.add_permits(1);
        }
    );
    assert!(errors.is_empty());
    assert!(finished.get());
    assert!(host.cleared.get());
    assert!(
        host.cluster
            .stores
            .get_store(1)
            .unwrap()
            .allow_leader_transfer_in()
    );
    assert_eq!(op.status(), STARTED);
    let scanned = host.scanned.get();
    tokio::time::advance(Duration::from_secs(120)).await;
    tokio::task::yield_now().await;
    assert_eq!(
        host.scanned.get(),
        scanned,
        "joined old instance must perform no more work"
    );
    let ctx = old.get_schedulers_controller().context();
    let persisted: serde_json::Value = ctx.storage.load_config(&ctx.paths).await.unwrap().unwrap();
    drop(old);
    drop(host);
    cache.shutdown().await;
    pending.shutdown().await;
    let (mut new, pending, _router) = setup(persisted, backend);
    new.init_schedulers(true, &CancellationToken::new())
        .await
        .unwrap();
    assert!(
        !new.get_cluster()
            .stores
            .get_store(1)
            .unwrap()
            .allow_leader_transfer_in()
    );
    assert!(new.get_operator_controller().operators().is_empty());
    new.get_schedulers_controller_mut().clean_registered().await;
    pending.shutdown().await;
    println!("task136 joined replacement cases=1 passed");
}

struct Now;
impl astersql_pd_schedule_operator::Clock for Now {
    fn now(&self) -> std::time::Instant {
        std::time::Instant::now()
    }
}
