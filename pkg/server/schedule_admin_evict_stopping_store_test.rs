// Copyright 2026 AsterSQL.
use crate::schedule_admin_evict_stopping_store::serve;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_schedulers::{EvictStoppingConfig, EvictStoppingScheduler};
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use serde_json::Value;
use std::sync::Arc;
async fn call(
    s: &mut EvictStoppingScheduler,
    e: &StorageEndpoint,
    method: &str,
    path: &str,
    body: &str,
) -> (u16, String, String) {
    let r = serve(
        s,
        e,
        &ClusterPaths::new(142),
        false,
        Request::builder()
            .method(method)
            .uri(path)
            .body(Body::from(body.to_owned()))
            .unwrap(),
    )
    .await;
    assert!(r.headers().get("allow").is_none());
    let status = r.status().as_u16();
    let ct = r
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_owned())
        .unwrap_or_default();
    (
        status,
        String::from_utf8(to_bytes(r.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap(),
        ct,
    )
}
#[tokio::test]
async fn task_142_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_142.json"
    ))
    .unwrap();
    let e = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let mut scheduler = EvictStoppingScheduler::new(EvictStoppingConfig::default());
    for c in fixture["cases"].as_array().unwrap() {
        let before = serde_json::to_value(scheduler.config()).unwrap();
        let (status, body, ct) = call(
            &mut scheduler,
            &e,
            c["method"].as_str().unwrap(),
            c["path"].as_str().unwrap(),
            c["input"].as_str().unwrap(),
        )
        .await;
        assert_eq!(
            status,
            c["status"].as_u64().unwrap() as u16,
            "{}: {body}",
            c["name"]
        );
        if let Some(expected) = c.get("json") {
            assert_eq!(
                serde_json::from_str::<Value>(&body).unwrap(),
                *expected,
                "{}",
                c["name"]
            );
            assert_eq!(ct, "application/json; charset=UTF-8");
        } else {
            assert_eq!(body, c["body"].as_str().unwrap());
            assert_eq!(
                ct,
                if status == 404 {
                    "text/plain; charset=utf-8"
                } else {
                    ""
                }
            );
        }
        if status != 200 {
            assert_eq!(serde_json::to_value(scheduler.config()).unwrap(), before);
        }
    }
    println!(
        "task142 HTTP cases={} passed",
        fixture["cases"].as_array().unwrap().len()
    );
}

use astersql_pd_storage_kv::{
    Commit, Compare, Lease, MetaWatch, Mutation, RawTxnOp, RawTxnResponse, Read, Snapshot,
    StoreError,
};
/// Only the I/O boundary is faulted; normal operations use the canonical backend.
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
async fn task_142_faults_and_body_errors() {
    use std::sync::atomic::Ordering;
    let backend = Arc::new(FaultStore::default());
    let e = StorageEndpoint::new(backend.clone());
    let mut scheduler = EvictStoppingScheduler::new(EvictStoppingConfig::default());
    let paths = ClusterPaths::new(142);
    assert_eq!(
        call(&mut scheduler, &e, "POST", "/config", r#"{"batch":5}"#)
            .await
            .0,
        200
    );
    let saved = e
        .load_scheduler_config(&paths, "evict-stopping-store-scheduler")
        .await
        .unwrap();
    backend.fail.store(true, Ordering::SeqCst);
    let (status, body, _) = call(&mut scheduler, &e, "POST", "/config", r#"{"batch":10}"#).await;
    assert_eq!(status, 500);
    assert_eq!(
        serde_json::from_str::<String>(&body).unwrap(),
        "metadata backend unavailable; mutation outcome may be unknown"
    );
    assert_eq!(scheduler.config().batch(), 5);
    backend.fail.store(false, Ordering::SeqCst);
    assert_eq!(
        e.load_scheduler_config(&paths, "evict-stopping-store-scheduler")
            .await
            .unwrap(),
        saved
    );
    assert_eq!(
        call(&mut scheduler, &e, "POST", "/config", r#"{"batch":10}"#)
            .await
            .0,
        200
    );
    assert_eq!(scheduler.config().batch(), 10);
    for input in ["", "{", "[]", "true", "12", "\"bad\""] {
        let before = serde_json::to_value(scheduler.config()).unwrap();
        let r = serve(
            &mut scheduler,
            &e,
            &paths,
            false,
            Request::builder()
                .method("POST")
                .uri("/config")
                .body(Body::from(input))
                .unwrap(),
        )
        .await;
        assert_eq!(r.status(), 400);
        assert_eq!(r.headers()["tidb-error-code"], "input");
        let v: Value =
            serde_json::from_slice(&to_bytes(r.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert_eq!(v["code"], "input");
        assert!(v["data"]["Offset"].is_number());
        assert_eq!(serde_json::to_value(scheduler.config()).unwrap(), before);
    }
    for prefix in [r#"{"batch":1}"#, r#"{"batch":"#] {
        let stream = futures_util::stream::iter(vec![
            Ok::<_, std::io::Error>(axum::body::Bytes::from(prefix)),
            Err(std::io::Error::other("task142 read fault")),
        ]);
        let r = serve(
            &mut scheduler,
            &e,
            &paths,
            false,
            Request::builder()
                .method("POST")
                .uri("/config")
                .body(Body::from_stream(stream))
                .unwrap(),
        )
        .await;
        assert_eq!(r.status(), 500);
        assert_eq!(r.headers()["tidb-error-code"], "internal");
        let v: Value =
            serde_json::from_slice(&to_bytes(r.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert_eq!(v["code"], "internal");
        assert_eq!(v["msg"], "task142 read fault");
        assert!(!v["stack"].as_array().unwrap().is_empty());
        assert_eq!(scheduler.config().batch(), 10);
    }
    println!("task142 fault/body groups=10 passed");
}
#[tokio::test]
async fn task_142_disabled_and_legacy_batch() {
    let e = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let paths = ClusterPaths::new(142);
    for batch in [3, 100] {
        let config: EvictStoppingConfig =
            serde_json::from_value(serde_json::json!({"batch":batch,"evicted-stores":[1,2]}))
                .unwrap();
        let mut scheduler = EvictStoppingScheduler::new(config);
        let r = serve(
            &mut scheduler,
            &e,
            &paths,
            true,
            Request::builder()
                .method("POST")
                .uri("/config")
                .body(Body::from(r#"{"disabled":false}"#))
                .unwrap(),
        )
        .await;
        assert_eq!(r.status(), 200);
        let persisted: Value = serde_json::from_slice(
            &e.load_scheduler_config(&paths, "evict-stopping-store-scheduler")
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            persisted,
            serde_json::json!({"batch":batch,"evicted-stores":[1,2],"disabled":true})
        );
        let (_, body, _) = call(&mut scheduler, &e, "GET", "/list", "").await;
        assert_eq!(
            serde_json::from_str::<Value>(&body).unwrap(),
            serde_json::json!({"batch":batch,"evicted-stores":[1,2],"disabled":false})
        );
    }
    println!("task142 disabled/legacy groups=2 passed");
}

use astersql_pd_core::{BasicCluster, StoreInfo, StoreMeta, StoreStats};
use astersql_pd_schedule_core::BasicCluster as _;
use astersql_pd_schedule_schedulers::EvictLeaderContext;
const NOW: i128 = 1_000_000_000_000;
fn cluster() -> BasicCluster {
    let c = BasicCluster::new();
    for id in 1..=5 {
        c.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            last_heartbeat: NOW as i64,
            ..Default::default()
        })));
    }
    c
}
fn stopping(c: &BasicCluster, id: u64, value: bool) {
    let mut s = c.stores.get_store(id).unwrap().as_ref().clone();
    s.set_new_store_stats(StoreStats {
        is_stopping: value,
        ..Default::default()
    });
    c.stores.put_store(Arc::new(s));
}

#[tokio::test]
async fn task_142_updated_scheduling() {
    use astersql_pd_core::{KeyRange, Peer, RegionInfo, RegionMeta, RegionOption};
    use astersql_pd_core_constant::PriorityLevel;
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_operator::{OpInfluence, OpKind, OpStep};
    use astersql_pd_schedule_placement::RuleManager;
    for batch in [1, 3, 5, 10] {
        let mut c = cluster();
        for id in 0..150 {
            let peers: Vec<_> = (1..=3)
                .map(|s| Peer {
                    id: id * 10 + s,
                    store_id: s,
                    ..Default::default()
                })
                .collect();
            let region = Arc::new(RegionInfo::new(
                RegionMeta {
                    id: id + 1,
                    key_range: KeyRange {
                        start_key: vec![id as u8],
                        end_key: vec![id as u8 + 1],
                    },
                    peers: peers.clone(),
                    ..Default::default()
                },
                Some(peers[0].clone()),
                vec![],
                vec![],
            ));
            let (o, v, b) = c.regions.set_region(region.clone());
            c.regions.update_sub_tree(region, o.as_deref(), &v, b);
        }
        stopping(&c, 1, true);
        let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
        let paths = ClusterPaths::new(142);
        let config = ProviderSnapshot::<()>::default();
        let mut rules = RuleManager::new(
            Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
            paths.clone(),
        );
        rules
            .initialize(3, &[], "", false, &config, &[])
            .await
            .unwrap();
        let mut draws = 0;
        let mut random = |bound: usize| {
            let i = draws % bound;
            draws += 1;
            i
        };
        let mut shuffle = |_: &mut [usize]| {};
        let mut scheduler = EvictStoppingScheduler::new(EvictStoppingConfig::default());
        scheduler.observe(&c, &storage, &paths).await.unwrap();
        assert_eq!(
            call(
                &mut scheduler,
                &storage,
                "POST",
                "/config",
                &serde_json::json!({"batch":batch as f64+if batch==10 {0.0} else {0.9}})
                    .to_string()
            )
            .await
            .0,
            200
        );
        assert_eq!(scheduler.config().batch(), batch);
        assert_eq!(scheduler.config().evict_store(), 1);
        let saved = storage
            .load_scheduler_config(&paths, "evict-stopping-store-scheduler")
            .await
            .unwrap();
        let restored: EvictStoppingConfig = serde_json::from_slice(&saved).unwrap();
        assert_eq!(restored.batch(), batch);
        assert_eq!(restored.evicted_stores(), &[1]);
        let (_, listed, _) = call(&mut scheduler, &storage, "GET", "/list", "").await;
        assert_eq!(
            serde_json::from_str::<Value>(&listed).unwrap(),
            serde_json::json!({"batch":batch,"evicted-stores":[1],"disabled":false})
        );
        let mut ctx = EvictLeaderContext {
            cluster: &c,
            config: &config,
            rules: &mut rules,
            now_unix_nanos: NOW,
            random_index: &mut random,
            shuffle: &mut shuffle,
        };
        for id in [2, 3] {
            c.stores
                .pause_leader_transfer(id, astersql_pd_core_constant::Direction::IN)
                .unwrap();
        }
        let (blocked, error) = scheduler.schedule(&mut ctx, &storage, &paths).await;
        assert!(error.is_none());
        assert!(blocked.operators.is_empty());
        assert_eq!(scheduler.config().evict_store(), 1);
        for id in [2, 3] {
            c.stores
                .resume_leader_transfer(id, astersql_pd_core_constant::Direction::IN);
        }
        let (result, error) = scheduler.schedule(&mut ctx, &storage, &paths).await;
        assert!(error.is_none());
        assert!(result.errors.is_empty());
        assert_eq!(result.operators.len(), batch);
        let mut ids = std::collections::HashSet::new();
        for op in result.operators {
            assert!(ids.insert(op.region_id()));
            assert_eq!(op.desc(), "evict-stopping-store-scheduler");
            assert_eq!(op.kind(), OpKind::LEADER);
            assert_eq!(op.priority_level(), PriorityLevel::URGENT);
            assert_eq!(op.len(), 1);
            let OpStep::TransferLeader(step) = op.step(0).unwrap() else {
                panic!()
            };
            assert_eq!(step.from_store, 1);
            assert_eq!(step.to_stores, vec![2, 3]);
            assert!(step.to_stores.contains(&step.to_store));
            let r = c.get_region(op.region_id()).unwrap();
            assert_eq!(step.conf_ver_changed(&r), 0);
            let influence = OpInfluence::new();
            step.influence(&influence, &r);
            assert_eq!(influence.get_store_influence(1).leader_count, -1);
            assert_eq!(influence.get_store_influence(step.to_store).leader_count, 1);
            for v2 in [false, true] {
                let command = step.get_cmd(&r, v2).transfer_leader.unwrap();
                assert_eq!(command.peer.unwrap().store_id, step.to_store);
                assert_eq!(command.peers.len(), 2);
            }
            assert!(op.start());
            assert!(op.check(&r).is_some());
            let applied = r.clone_with([RegionOption::Leader(
                r.get_store_peer(step.to_store).cloned(),
            )]);
            assert!(op.check(&applied).is_none());
            assert!(op.check_success());
        }
        stopping(&c, 1, false);
        scheduler.schedule(&mut ctx, &storage, &paths).await;
        let (result, error) = scheduler.schedule(&mut ctx, &storage, &paths).await;
        assert!(error.is_none());
        assert!(result.operators.is_empty());
    }
    println!("task142 real scheduling groups=4 passed; 19 operators checked");
}
