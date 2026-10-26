// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_types::*;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use serde_json::Value;
use std::sync::Arc;
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_106.json"
    ))
    .unwrap()
}
fn context() -> FactoryContext {
    FactoryContext::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        ClusterPaths::new(106),
        JobTime(0),
    )
}
#[tokio::test]
async fn task_106_behavior_cases() {
    let mut registry = SchedulerRegistry::default();
    registry.register_builtins().unwrap();
    registry.register_builtins().unwrap(); // Go Register's once semantics.
    assert_eq!(registry.registered_types().count(), 18);
    let ctx = context();
    let fixture = fixture();
    for case in fixture["cases"].as_array().unwrap() {
        let typ = CheckerSchedulerType::from(case["type"].as_str().unwrap());
        let args: Vec<_> = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let scheduler = registry.create_args(&typ, &args, &ctx).unwrap();
        assert_eq!(scheduler.name(), case["name"].as_str().unwrap());
        assert_eq!(scheduler.scheduler_type(), typ);
        let data: Value = serde_json::from_slice(&scheduler.encode_config().unwrap()).unwrap();
        for (key, value) in case["expected"].as_object().unwrap() {
            assert_eq!(&data[key], value, "{typ}: {key}");
        }
        if case["expected_null"] == true {
            assert_eq!(data, Value::Null);
        }
        if let Some(job) = case["job_expected"].as_object() {
            assert_eq!(data.as_array().unwrap().len(), 1);
            for (key, val) in job {
                assert_eq!(&data[0][key], val, "job {key}");
            }
        }
        let owned: Vec<_> = args.iter().map(|s| s.to_string()).collect();
        let decoded: Value = registry.decode_slice(&typ, &owned).unwrap();
        assert!(decoded.is_object() || decoded.is_array());
        scheduler.save_config(&ctx).await.unwrap();
        let restored = registry.restore(&scheduler.name(), &ctx).await.unwrap();
        assert_eq!(restored.scheduler_type(), typ);
        assert_eq!(
            restored.encode_config().unwrap(),
            scheduler.encode_config().unwrap()
        );
    }
    println!("18 real factories: args, fixed defaults, save and restore passed");
}
#[test]
fn task_106_errors_and_names() {
    let mut registry = SchedulerRegistry::default();
    registry.register_builtins().unwrap();
    let ctx = context();
    for case in fixture()["invalid"].as_array().unwrap() {
        let typ = CheckerSchedulerType::from(case[0].as_str().unwrap());
        let args: Vec<_> = case[1]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(registry.create_args(&typ, &args, &ctx).is_err(), "{case}");
    }
    assert!(matches!(
        registry.register_type(BALANCE_LEADER_SCHEDULER),
        Err(FactoryError::Duplicate(_))
    ));
    let mut partial = SchedulerRegistry::default();
    partial.register_type(LABEL_SCHEDULER).unwrap();
    assert!(partial.register_builtins().is_err());
    assert_eq!(partial.registered_types().count(), 1);
    for typ in ["plugin", "unknown"] {
        assert!(registry.register_type(typ.into()).is_err());
        assert!(registry.create_args(&typ.into(), &[], &ctx).is_err());
    }
    assert_eq!(registry.find_type_by_name("unknown"), None);
    assert_eq!(
        registry.find_type_by_name("prefix-transfer-witness-leader-scheduler"),
        Some(TRANSFER_WITNESS_LEADER_SCHEDULER)
    );
    assert_eq!(
        registry.find_type_by_name("old-scatter-range-a"),
        Some(SCATTER_RANGE_SCHEDULER)
    );
    assert!(
        registry
            .create_json(&BALANCE_LEADER_SCHEDULER, b"{\"batch\":101}", &ctx)
            .is_err()
    );
    assert!(
        registry
            .create_json(
                &BALANCE_HOT_REGION_SCHEDULER,
                b"{\"read-priorities\":[\"invalid\"]}",
                &ctx
            )
            .is_err()
    );
    assert!(
        registry
            .create_json(&SCATTER_RANGE_SCHEDULER, b"{}", &ctx)
            .is_err()
    );
}

use astersql_pd_storage_kv::{
    Commit, Compare, Lease, MetaWatch, Mutation, RawTxnOp, RawTxnResponse, Read, Snapshot,
    StoreError,
};
#[derive(Default)]
struct FaultStore {
    store: MemoryStore,
    fail: std::sync::atomic::AtomicBool,
}
#[async_trait::async_trait]
impl astersql_pd_storage_kv::MetaStore for FaultStore {
    async fn read(&self, r: Read) -> astersql_pd_storage_kv::Result<Snapshot> {
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
async fn task_106_defaults_and_storage_faults() {
    let mut registry = SchedulerRegistry::default();
    registry.register_builtins().unwrap();
    let backend = Arc::new(FaultStore::default());
    let ctx = FactoryContext::new(
        Arc::new(StorageEndpoint::new(backend.clone())),
        ClusterPaths::new(106),
        JobTime(0),
    );
    let mut scheduler = registry
        .create_json(&BALANCE_LEADER_SCHEDULER, b"{}", &ctx)
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&scheduler.encode_config().unwrap()).unwrap()["batch"],
        4
    );
    scheduler.save_config(&ctx).await.unwrap();
    scheduler.set_disabled(true);
    backend
        .fail
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(scheduler.save_config(&ctx).await.is_err());
    assert!(registry.restore(&scheduler.name(), &ctx).await.is_err());
    assert!(scheduler.is_disabled());
    backend
        .fail
        .store(false, std::sync::atomic::Ordering::SeqCst);
    assert!(
        !registry
            .restore(&scheduler.name(), &ctx)
            .await
            .unwrap()
            .is_disabled()
    );
    scheduler.save_config(&ctx).await.unwrap();
    assert!(
        registry
            .restore(&scheduler.name(), &ctx)
            .await
            .unwrap()
            .is_disabled()
    );
    assert!(registry.restore("unknown", &ctx).await.is_err());
    assert!(registry.restore("label-scheduler", &ctx).await.is_err());
    ctx.storage
        .save_scheduler_config(&ctx.paths, &scheduler.name(), b"{")
        .await
        .unwrap();
    assert!(registry.restore(&scheduler.name(), &ctx).await.is_err());
    for typ in [
        BALANCE_LEADER_SCHEDULER,
        BALANCE_WITNESS_SCHEDULER,
        EVICT_LEADER_SCHEDULER,
    ] {
        let s = registry.create_json(&typ, b"{\"batch\":0}", &ctx).unwrap();
        let v: Value = serde_json::from_slice(&s.encode_config().unwrap()).unwrap();
        assert_eq!(
            v["batch"],
            if typ == EVICT_LEADER_SCHEDULER { 3 } else { 4 }
        );
    }
    let old = registry
        .create_json(&BALANCE_HOT_REGION_SCHEDULER, b"{\"disabled\":true}", &ctx)
        .unwrap();
    let data: Value = serde_json::from_slice(&old.encode_config().unwrap()).unwrap();
    assert_eq!(data["rank-formula-version"], "");
    assert_eq!(data["read-priorities"], serde_json::json!(["byte", "key"]));
    assert_eq!(
        data["write-leader-priorities"],
        serde_json::json!(["key", "byte"])
    );
    assert!(old.is_disabled());
    assert!(
        registry
            .create_json(
                &TRANSFER_WITNESS_LEADER_SCHEDULER,
                b"malformed ignored decoder",
                &ctx
            )
            .is_ok()
    );
    assert!(
        registry
            .create_json(&BALANCE_LEADER_SCHEDULER, b"{\"disabled\":42}", &ctx)
            .is_err()
    );
    assert!(
        registry
            .create_json(&LABEL_SCHEDULER, b"{\"disabled\":42}", &ctx)
            .is_ok()
    );
    for typ in [
        BALANCE_LEADER_SCHEDULER,
        SHUFFLE_REGION_SCHEDULER,
        LABEL_SCHEDULER,
    ] {
        for bytes in [b"{".as_slice(), b"[]"] {
            assert!(registry.create_json(&typ, bytes, &ctx).is_err());
        }
    }
    println!(
        "storage write/read faults, missing/corrupt restore, zero-batch and legacy hot defaults passed"
    );
}
#[tokio::test]
async fn task_106_factory_policy_executes() {
    use astersql_pd_core::{BasicCluster, Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta};
    use astersql_pd_core_constant::Direction;
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_operator::{OpKind, OpStep};
    use astersql_pd_schedule_placement::RuleManager;
    let ctx = context();
    let mut registry = SchedulerRegistry::default();
    registry.register_builtins().unwrap();
    let mut cluster = BasicCluster::new();
    for id in 1..=3 {
        let mut store = StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        });
        store.set_last_heartbeat_unix_nanos(1_000_000_000_000);
        cluster.stores.put_store(Arc::new(store));
    }
    cluster
        .stores
        .pause_leader_transfer(1, Direction::IN)
        .unwrap();
    let peers: Vec<_> = (1..=3)
        .map(|id| Peer {
            id,
            store_id: id,
            ..Default::default()
        })
        .collect();
    let r = Arc::new(RegionInfo::new(
        RegionMeta {
            id: 1,
            peers: peers.clone(),
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![],
    ));
    let (old, overlaps, changed) = cluster.regions.set_region(r.clone());
    cluster
        .regions
        .update_sub_tree(r, old.as_deref(), &overlaps, changed);
    let config = ProviderSnapshot::<()>::default();
    let mut rules = RuleManager::new(ctx.storage.clone(), ctx.paths.clone());
    rules
        .initialize(3, &[], "", false, &config, &[])
        .await
        .unwrap();
    let created = registry
        .create_args(&SHUFFLE_LEADER_SCHEDULER, &[], &ctx)
        .unwrap();
    created.save_config(&ctx).await.unwrap();
    let restored = registry.restore(&created.name(), &ctx).await.unwrap();
    for s in [created, restored] {
        let BuiltinScheduler::ShuffleLeader(s) = s.policy else {
            panic!("wrong factory")
        };
        let result = s
            .schedule(&mut EvictLeaderContext {
                cluster: &cluster,
                config: &config,
                rules: &mut rules,
                now_unix_nanos: 1_000_000_000_000,
                random_index: &mut |_| 0,
                shuffle: &mut |_| {},
            })
            .await;
        assert!(result.errors.is_empty());
        assert_eq!(result.operators.len(), 1);
        let op = &result.operators[0];
        assert_eq!(op.kind(), OpKind::LEADER | OpKind::ADMIN);
        assert_eq!(op.len(), 1);
        let OpStep::TransferLeader(step) = op.step(0).unwrap() else {
            panic!("wrong step")
        };
        assert_eq!((step.from_store, step.to_store), (1, 2));
    }
    println!("factory and restored shuffle policy both produced real transfer 1 -> 2");
}
