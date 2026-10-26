// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{BasicCluster, StoreInfo, StoreMeta};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_types::*;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use std::sync::Arc;
fn context() -> FactoryContext {
    FactoryContext::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        ClusterPaths::new(107),
        JobTime(0),
    )
}
fn registry() -> SchedulerRegistry {
    let mut r = SchedulerRegistry::default();
    r.register_builtins().unwrap();
    r
}
fn cluster() -> Arc<BasicCluster> {
    let c = Arc::new(BasicCluster::new());
    for id in 1..=3 {
        c.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        })));
    }
    c
}
#[tokio::test]
async fn task_107_behavior_cases() {
    let ctx = context();
    let r = registry();
    let cluster = cluster();
    let mut c = SchedulerController::new(Some(cluster), ctx, serde_json::json!({"other":7}));
    let s = r
        .create_args(&BALANCE_LEADER_SCHEDULER, &[], c.context())
        .unwrap();
    let name = s.name();
    c.add(s, &[], false).await.unwrap();
    assert!(c.exists(&name).unwrap());
    assert_eq!(c.names(), vec![name.clone()]);
    assert!(matches!(
        c.add(
            r.create_args(&BALANCE_LEADER_SCHEDULER, &[], c.context())
                .unwrap(),
            &[],
            false
        )
        .await,
        Err(ControlError::Exists(_))
    ));
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_107.json"
    ))
    .unwrap();
    for row in fixture["pause"].as_array().unwrap() {
        let now = row["at"].as_i64().unwrap();
        if let Some(ttl) = row["ttl"].as_i64() {
            c.pause_or_resume(&name, ttl, now).unwrap();
        }
        assert_eq!(
            c.is_paused(&name, now).unwrap(),
            row["paused"].as_bool().unwrap()
        );
        assert_eq!(
            c.delay(&name, now).unwrap(),
            (
                row["delay_at"].as_i64().unwrap(),
                row["delay_until"].as_i64().unwrap()
            )
        );
    }
    c.pause_or_resume("all", 10, 100).unwrap();
    assert_eq!(c.delay(&name, 100).unwrap(), (100, 110));
    c.remove(&name, false).await.unwrap();
    assert!(matches!(c.exists(&name), Err(ControlError::NotFound(_))));
    assert!(matches!(
        c.remove(&name, false).await,
        Err(ControlError::NotFound(_))
    ));
    assert_eq!(c.configuration()["other"], 7);
    assert_eq!(
        c.configuration()["schedule"]["schedulers-v2"][0]["disable"],
        true
    );
    let s = r.create_args(&LABEL_SCHEDULER, &[], c.context()).unwrap();
    let name = s.name();
    c.add(s, &[], true).await.unwrap();
    assert!(c.names().is_empty());
    assert_eq!(c.handler_names(), vec![name.clone()]);
    assert!(c.exists(&name).unwrap());
    assert!(matches!(
        c.reload(&name).await,
        Err(ControlError::NotFound(_))
    ));
    c.remove(&name, true).await.unwrap();
    assert!(c.handler_names().is_empty());
    let mut c = SchedulerController::new(None, context(), serde_json::json!({}));
    assert!(matches!(
        c.pause_or_resume("all", 1, 0),
        Err(ControlError::NotBootstrapped)
    ));
    assert!(matches!(
        c.remove("missing", false).await,
        Err(ControlError::NotBootstrapped)
    ));
    assert!(matches!(
        c.reload("missing").await,
        Err(ControlError::NotBootstrapped)
    ));
}
#[tokio::test]
async fn task_107_reload_and_prepare_rollback() {
    let ctx = context();
    let r = registry();
    let cluster = cluster();
    let mut c = SchedulerController::new(Some(cluster.clone()), ctx, serde_json::json!({}));
    let name = "evict-leader-scheduler";
    c.add(
        r.create_args(&EVICT_LEADER_SCHEDULER, &["1"], c.context())
            .unwrap(),
        &["1"],
        false,
    )
    .await
    .unwrap();
    assert!(
        !cluster
            .stores
            .get_store(1)
            .unwrap()
            .allow_leader_transfer_in()
    );
    c.pause_or_resume(name, 20, 100).unwrap();
    let before = c.get(name).unwrap().scheduler.encode_config().unwrap();
    c.context()
        .storage
        .save_scheduler_config(&c.context().paths, name, b"{")
        .await
        .unwrap();
    assert!(c.reload(name).await.is_err());
    assert_eq!(
        c.get(name).unwrap().scheduler.encode_config().unwrap(),
        before
    );
    assert_eq!(c.delay(name, 101).unwrap(), (100, 120));
    c.context()
        .storage
        .save_scheduler_config(
            &c.context().paths,
            name,
            br#"{"store-id-ranges":{"2":[]},"batch":5}"#,
        )
        .await
        .unwrap();
    c.reload(name).await.unwrap();
    c.reload(name).await.unwrap();
    assert!(
        cluster
            .stores
            .get_store(1)
            .unwrap()
            .allow_leader_transfer_in()
    );
    assert!(
        !cluster
            .stores
            .get_store(2)
            .unwrap()
            .allow_leader_transfer_in()
    );
    c.remove(name, false).await.unwrap();
    assert!(
        cluster
            .stores
            .get_store(2)
            .unwrap()
            .allow_leader_transfer_in()
    );
    for handler in [false, true] {
        assert!(
            c.add(
                r.create_args(&EVICT_LEADER_SCHEDULER, &["1,999"], c.context())
                    .unwrap(),
                &["1,999"],
                handler
            )
            .await
            .is_err()
        );
        assert!(
            cluster
                .stores
                .get_store(1)
                .unwrap()
                .allow_leader_transfer_in()
        );
        assert!(c.exists(name).is_err());
        assert!(
            c.context()
                .storage
                .load_scheduler_config(&c.context().paths, name)
                .await
                .unwrap()
                .is_empty()
        );
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
async fn task_107_all_factories_and_indexes() {
    let r = registry();
    let rows: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_106.json"
    ))
    .unwrap();
    for handler_only in [false, true] {
        let mut c = SchedulerController::new(Some(cluster()), context(), serde_json::json!({}));
        for row in rows["cases"].as_array().unwrap() {
            let typ = CheckerSchedulerType::from(row["type"].as_str().unwrap());
            let args: Vec<_> = row["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            let s = r.create_args(&typ, &args, c.context()).unwrap();
            let name = s.name();
            c.add(s, &args, handler_only).await.unwrap();
            assert!(c.handler(&name).is_some());
            assert!(c.exists(&name).unwrap());
            if !handler_only {
                c.pause_or_resume(&name, 10, 100).unwrap();
                c.reload(&name).await.unwrap();
                assert_eq!(c.delay(&name, 101).unwrap(), (100, 110));
                assert!(!c.is_disabled(&name).unwrap());
            }
            c.remove(&name, handler_only).await.unwrap();
            assert!(c.exists(&name).is_err());
            assert!(
                c.cleanup_errors.is_empty(),
                "{name}: {:?}",
                c.cleanup_errors
            );
        }
    }
    // A populated handler-only index takes precedence over the regular policy index.
    let mut c = SchedulerController::new(Some(cluster()), context(), serde_json::json!({}));
    for (typ, handler) in [(LABEL_SCHEDULER, false), (SHUFFLE_LEADER_SCHEDULER, true)] {
        c.add(r.create_args(&typ, &[], c.context()).unwrap(), &[], handler)
            .await
            .unwrap();
    }
    assert_eq!(c.handler_names(), vec!["shuffle-leader-scheduler"]);
    assert!(c.handler("label-scheduler").is_none());
    c.remove("shuffle-leader-scheduler", true).await.unwrap();
    assert_eq!(c.handler_names(), vec!["label-scheduler"]);
    println!("36 factory/mode lifecycle cases + mixed handler-index precedence passed");
}
#[tokio::test]
async fn task_107_storage_faults() {
    use std::sync::atomic::Ordering::SeqCst;
    let r = registry();
    let backend = Arc::new(FaultStore::default());
    let cluster = cluster();
    let ctx = FactoryContext::new(
        Arc::new(StorageEndpoint::new(backend.clone())),
        ClusterPaths::new(107),
        JobTime(0),
    );
    let mut c =
        SchedulerController::new(Some(cluster.clone()), ctx, serde_json::json!({"other":7}));
    let name = "evict-leader-scheduler";
    backend.writes_until_failure.store(1, SeqCst);
    assert!(
        c.add(
            r.create_args(&EVICT_LEADER_SCHEDULER, &["1"], c.context())
                .unwrap(),
            &["1"],
            false
        )
        .await
        .is_err()
    );
    assert!(c.exists(name).is_err());
    assert!(
        cluster
            .stores
            .get_store(1)
            .unwrap()
            .allow_leader_transfer_in()
    );
    c.add(
        r.create_args(&EVICT_LEADER_SCHEDULER, &["1"], c.context())
            .unwrap(),
        &["1"],
        false,
    )
    .await
    .unwrap();
    let before = c.get(name).unwrap().scheduler.encode_config().unwrap();
    backend.fail.store(true, SeqCst);
    assert!(c.reload(name).await.is_err());
    assert_eq!(
        c.get(name).unwrap().scheduler.encode_config().unwrap(),
        before
    );
    backend.fail.store(false, SeqCst);
    for write in [1, 2] {
        backend.writes_until_failure.store(write, SeqCst);
        assert!(c.remove(name, false).await.is_err());
        assert!(c.exists(name).unwrap());
        assert!(
            !cluster
                .stores
                .get_store(1)
                .unwrap()
                .allow_leader_transfer_in()
        );
        assert_eq!(
            c.context()
                .storage
                .load_scheduler_config(&c.context().paths, name)
                .await
                .unwrap(),
            before
        );
    }
    c.remove(name, false).await.unwrap();
    assert!(
        cluster
            .stores
            .get_store(1)
            .unwrap()
            .allow_leader_transfer_in()
    );
    // Handler restore failures must preserve pre-existing persisted history.
    c.context()
        .storage
        .save_scheduler_config(&c.context().paths, name, &before)
        .await
        .unwrap();
    assert!(
        c.add(
            r.create_args(&EVICT_LEADER_SCHEDULER, &["1,999"], c.context())
                .unwrap(),
            &["1,999"],
            true
        )
        .await
        .is_err()
    );
    let persisted = c
        .context()
        .storage
        .load_scheduler_config(&c.context().paths, name)
        .await
        .unwrap();
    assert!(!persisted.is_empty());
    assert!(c.exists(name).is_err());
    assert!(
        cluster
            .stores
            .get_store(1)
            .unwrap()
            .allow_leader_transfer_in()
    );
    backend.fail.store(true, SeqCst);
    assert!(
        c.add(
            r.create_args(&LABEL_SCHEDULER, &[], c.context()).unwrap(),
            &[],
            true
        )
        .await
        .is_err()
    );
    assert!(c.exists("label-scheduler").is_err());
    backend.fail.store(false, SeqCst);
    let typ = BALANCE_LEADER_SCHEDULER;
    c.add(r.create_args(&typ, &[], c.context()).unwrap(), &[], false)
        .await
        .unwrap();
    backend.writes_until_failure.store(1, SeqCst);
    assert!(c.remove(typ.as_str(), false).await.is_err());
    assert!(c.exists(typ.as_str()).unwrap());
    assert!(c.is_disabled(typ.as_str()).unwrap());
    c.remove(typ.as_str(), false).await.unwrap();
    c.add(r.create_args(&typ, &[], c.context()).unwrap(), &[], false)
        .await
        .unwrap();
    assert!(!c.is_disabled(typ.as_str()).unwrap());
    assert_eq!(c.configuration()["other"], 7);
    println!(
        "add write, handler read, reload read, remove config/delete faults and retries passed"
    );
}
