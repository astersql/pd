// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_cache::{TTLUint64, TtlDuration};
use astersql_pd_core::{
    BasicCluster, KeyRange, Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta,
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_hbstream::routing::StreamRouter;
use astersql_pd_schedule_scatter::ScatterBatchMetrics;
use astersql_pd_schedule_schedulers::{BuiltinScheduler, FactoryContext, JobTime};
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
fn setup(
    config: serde_json::Value,
    backend: Arc<dyn astersql_pd_storage_kv::MetaStore>,
) -> (Coordinator<ProviderSnapshot<()>>, Arc<TTLUint64<()>>) {
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
    let (heartbeat, _router) = StreamRouter::new(Arc::new(astersql_pd_core::StoresInfo::new()));
    let pending = Arc::new(TTLUint64::new(
        tokio::runtime::Handle::current(),
        CancellationToken::new(),
        Duration::from_secs(60),
        TtlDuration::from_nanos(180_000_000_000),
    ));
    let c = Coordinator::new(CoordinatorInputs {
        cluster,
        config: Arc::new(ProviderSnapshot::<()>::default()),
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
    (c, pending)
}
#[tokio::test]
async fn task_114_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_114.json"
    ))
    .unwrap();
    for handler_only in [false, true] {
        for row in fixture["cases"].as_array().unwrap() {
            let (mut c, pending) = setup(
                serde_json::json!({"other":7,"schedule":{"schedulers-v2":row["configs"]}}),
                Arc::new(MemoryStore::default()),
            );
            let ctx = c.get_schedulers_controller().context();
            for (name, config) in row["persisted"].as_object().unwrap() {
                let bytes = if let Some(s) = config.as_str() {
                    s.as_bytes().to_vec()
                } else {
                    serde_json::to_vec(config).unwrap()
                };
                ctx.storage
                    .save_scheduler_config(&ctx.paths, name, &bytes)
                    .await
                    .unwrap();
            }
            assert!(!c.are_schedulers_initialized());
            let report = c
                .init_schedulers(!handler_only, &CancellationToken::new())
                .await
                .unwrap();
            assert!(c.are_schedulers_initialized());
            assert_eq!(
                report.valid_configs.len() as u64,
                row["valid"].as_u64().unwrap(),
                "{row}"
            );
            let controller = c.get_schedulers_controller();
            assert_eq!(
                serde_json::json!(controller.handler_names()),
                row["names"],
                "{row}"
            );
            if handler_only {
                assert!(controller.names().is_empty());
            }
            assert_eq!(controller.configuration()["other"], 7);
            assert_eq!(
                controller.configuration()["schedule"]["schedulers-v2"]
                    .as_array()
                    .unwrap()
                    .len(),
                report.valid_configs.len()
            );
            let ctx = controller.context();
            let saved: serde_json::Value =
                ctx.storage.load_config(&ctx.paths).await.unwrap().unwrap();
            assert_eq!(&saved, controller.configuration());
            if let Some(batch) = row.get("batch") {
                let policy = controller.handler("balance-leader-scheduler").unwrap();
                let encoded: serde_json::Value =
                    serde_json::from_slice(&policy.encode_config().unwrap()).unwrap();
                assert_eq!(&encoded["batch"], batch);
            }
            assert!(
                c.get_cluster()
                    .stores
                    .get_store(1)
                    .unwrap()
                    .allow_leader_transfer_in()
            );
            pending.shutdown().await;
        }
    }
    println!("20 source-derived restoration cases passed");
}
#[tokio::test]
async fn task_114_cancellation_and_storage_faults() {
    use std::sync::atomic::Ordering::SeqCst;
    let backend = Arc::new(FaultStore::default());
    let (mut c, pending) = setup(
        serde_json::json!({"schedule":{"schedulers-v2":[{"type":"label"}]}}),
        backend.clone(),
    );
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(matches!(
        c.init_schedulers(true, &cancelled).await,
        Err(SchedulerRestoreError::Cancelled)
    ));
    assert!(!c.are_schedulers_initialized());
    assert!(c.get_schedulers_controller().names().is_empty());
    backend.reads.store(0, SeqCst);
    backend.fail.store(true, SeqCst);
    assert!(
        c.init_schedulers(true, &CancellationToken::new())
            .await
            .is_err()
    );
    assert!(!c.are_schedulers_initialized());
    assert_eq!(backend.reads.load(SeqCst), 10);
    backend.fail.store(false, SeqCst);
    backend.read_failures.store(2, SeqCst);
    // The scheduler write succeeds; the final whole-config write fails nonfatally.
    backend.writes_until_failure.store(2, SeqCst);
    let report = c
        .init_schedulers(true, &CancellationToken::new())
        .await
        .unwrap();
    assert!(report.persistence_error.is_some());
    assert!(c.are_schedulers_initialized());
    assert_eq!(c.get_schedulers_controller().names(), ["label-scheduler"]);
    let report = c
        .init_schedulers(true, &CancellationToken::new())
        .await
        .unwrap();
    assert!(report.persistence_error.is_none());
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

#[tokio::test]
async fn task_114_failed_add_cleanup() {
    use std::sync::atomic::Ordering::SeqCst;
    for need_run in [false, true] {
        let backend = Arc::new(FaultStore::default());
        let (mut c, pending) = setup(
            serde_json::json!({"schedule":{"schedulers-v2":[{"type":"grant-leader","args":["1"]}]}}),
            backend.clone(),
        );
        backend.writes_until_failure.store(1, SeqCst);
        let report = c
            .init_schedulers(need_run, &CancellationToken::new())
            .await
            .unwrap();
        assert!(c.are_schedulers_initialized());
        assert!(report.valid_configs.is_empty());
        assert!(!report.skipped.is_empty());
        assert!(c.get_schedulers_controller().handler_names().is_empty());
        assert!(
            c.get_cluster()
                .stores
                .get_store(1)
                .unwrap()
                .allow_leader_transfer_out()
        );
        pending.shutdown().await;
    }
}
#[tokio::test]
async fn task_114_all_builtins_restore() {
    use astersql_pd_schedule_types::{CheckerSchedulerType, scheduler_type_compatible};
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_106.json"
    ))
    .unwrap();
    for need_run in [false, true] {
        for row in fixture["cases"].as_array().unwrap() {
            let typ = CheckerSchedulerType::from(row["type"].as_str().unwrap());
            let (mut c, pending) = setup(
                serde_json::json!({"schedule":{"schedulers-v2":[{"type":scheduler_type_compatible(&typ).unwrap(),"args":row["args"]}]}}),
                Arc::new(MemoryStore::default()),
            );
            let report = c
                .init_schedulers(need_run, &CancellationToken::new())
                .await
                .unwrap();
            assert_eq!(report.valid_configs.len(), 1, "{row}: {:?}", report.skipped);
            assert_eq!(
                c.get_schedulers_controller().handler_names(),
                [row["name"].as_str().unwrap()]
            );
            assert_eq!(
                c.get_schedulers_controller()
                    .handler(row["name"].as_str().unwrap())
                    .unwrap()
                    .scheduler_type(),
                typ
            );
            pending.shutdown().await;
        }
    }
    println!("36 real built-in/mode startup cases passed");
}
#[tokio::test]
async fn task_114_restart_operator_result() {
    use astersql_pd_schedule_operator::{OpKind, OpStep};
    use astersql_pd_schedule_placement::RuleManager;
    use astersql_pd_schedule_schedulers::GrantLeaderContext;
    let backend = Arc::new(MemoryStore::default());
    let initial =
        serde_json::json!({"schedule":{"schedulers-v2":[{"type":"grant-leader","args":["3"]}]}});
    let mut outputs = vec![];
    let mut configuration = initial;
    for _ in 0..2 {
        let (mut c, pending) = setup(configuration, backend.clone());
        c.init_schedulers(true, &CancellationToken::new())
            .await
            .unwrap();
        let ctx = c.get_schedulers_controller().context();
        let policy = c
            .get_schedulers_controller()
            .handler("grant-leader-scheduler")
            .unwrap();
        let BuiltinScheduler::GrantLeader(scheduler) = &policy.policy else {
            panic!("real grant policy expected")
        };
        let config = ProviderSnapshot::<()>::default();
        let mut rules = RuleManager::new(ctx.storage.clone(), ctx.paths.clone());
        rules
            .initialize(3, &[], "", false, &config, &[])
            .await
            .unwrap();
        let mut index = |_: usize| 0;
        let mut shuffle = |_: &mut [usize]| {};
        let result = scheduler
            .schedule(&mut GrantLeaderContext {
                cluster: &**c.get_cluster(),
                config: &config,
                rules: &mut rules,
                now_unix_nanos: 0,
                random_index: &mut index,
                shuffle: &mut shuffle,
            })
            .await;
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(result.operators.len(), 1);
        let op = &result.operators[0];
        assert_eq!(op.kind(), OpKind::LEADER);
        assert_eq!(op.region_id(), 1);
        assert_eq!(op.len(), 1);
        let OpStep::TransferLeader(step) = op.step(0).unwrap() else {
            panic!("transfer leader expected")
        };
        assert_eq!((step.from_store, step.to_store), (1, 3));
        outputs.push((op.region_id(), step.from_store, step.to_store));
        configuration = ctx.storage.load_config(&ctx.paths).await.unwrap().unwrap();
        pending.shutdown().await;
    }
    assert_eq!(outputs[0], outputs[1]);
}
