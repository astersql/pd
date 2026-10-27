// Copyright 2026 AsterSQL.
use crate::schedule_admin_evict_slow_store::serve;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_schedulers::{
    EvictDiskConfig, EvictDiskScheduler, EvictNetworkConfig, EvictNetworkScheduler,
};
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use serde_json::Value;
use std::sync::Arc;
fn schedulers() -> (EvictDiskScheduler, EvictNetworkScheduler) {
    (
        EvictDiskScheduler::new(EvictDiskConfig::default()),
        EvictNetworkScheduler::new(EvictNetworkConfig::default()),
    )
}
async fn call(
    d: &mut EvictDiskScheduler,
    n: &mut EvictNetworkScheduler,
    e: &StorageEndpoint,
    method: &str,
    path: &str,
    body: &str,
) -> (u16, String, String) {
    let r = serve(
        d,
        n,
        e,
        &ClusterPaths::new(141),
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
async fn task_141_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_141.json"
    ))
    .unwrap();
    let e = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let (mut d, mut n) = schedulers();
    for c in fixture["cases"].as_array().unwrap() {
        let before = serde_json::to_value(n.config()).unwrap();
        let (status, body, ct) = call(
            &mut d,
            &mut n,
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
            assert_eq!(serde_json::to_value(n.config()).unwrap(), before);
        }
        assert_eq!(d.config().batch(), n.config().disk.batch());
        assert_eq!(d.config().recovery_sec(), n.config().disk.recovery_sec());
    }
    println!(
        "task141 HTTP cases={} passed",
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
async fn task_141_faults_and_body_errors() {
    use std::sync::atomic::Ordering;
    let backend = Arc::new(FaultStore::default());
    let e = StorageEndpoint::new(backend.clone());
    let (mut d, mut n) = schedulers();
    let paths = ClusterPaths::new(141);
    assert_eq!(
        call(
            &mut d,
            &mut n,
            &e,
            "POST",
            "/config",
            r#"{"batch":5,"recovery-duration":10}"#
        )
        .await
        .0,
        200
    );
    let saved = e
        .load_scheduler_config(&paths, "evict-slow-store-scheduler")
        .await
        .unwrap();
    backend.fail.store(true, Ordering::SeqCst);
    let (status, body, _) = call(
        &mut d,
        &mut n,
        &e,
        "POST",
        "/config",
        r#"{"batch":100,"recovery-duration":0,"enable-network-slow-store":true}"#,
    )
    .await;
    assert_eq!(status, 500);
    assert_eq!(
        serde_json::from_str::<String>(&body).unwrap(),
        "metadata backend unavailable; mutation outcome may be unknown"
    );
    assert_eq!(d.config().batch(), 5);
    assert_eq!(d.config().recovery_sec(), 10);
    assert_eq!(n.config().disk.batch(), 5);
    assert!(n.config().enabled);
    backend.fail.store(false, Ordering::SeqCst);
    assert_eq!(
        e.load_scheduler_config(&paths, "evict-slow-store-scheduler")
            .await
            .unwrap(),
        saved
    );
    assert_eq!(
        call(&mut d, &mut n, &e, "POST", "/config", "{}").await.0,
        200
    );
    let persisted: Value = serde_json::from_slice(
        &e.load_scheduler_config(&paths, "evict-slow-store-scheduler")
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(persisted["enable-network-slow-store"], true);
    for input in ["", "{", "[]", "true", "12", "\"bad\""] {
        let before = serde_json::to_value(n.config()).unwrap();
        let r = serve(
            &mut d,
            &mut n,
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
        let value: Value =
            serde_json::from_slice(&to_bytes(r.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert_eq!(value["code"], "input");
        assert!(
            value["msg"]
                .as_str()
                .unwrap()
                .starts_with(if input == "" || input == "{" {
                    "unexpected end"
                } else {
                    "json: cannot unmarshal"
                })
        );
        assert!(value["data"]["Offset"].is_number());
        assert_eq!(serde_json::to_value(n.config()).unwrap(), before);
    }
    for prefix in [r#"{"batch":100}"#, r#"{"batch":"#] {
        let stream = futures_util::stream::iter(vec![
            Ok::<_, std::io::Error>(axum::body::Bytes::from(prefix)),
            Err(std::io::Error::other("task141 read fault")),
        ]);
        let r = serve(
            &mut d,
            &mut n,
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
        assert_eq!(v["msg"], "task141 read fault");
        assert!(v["stack"].as_array().unwrap().len() > 0);
        assert_eq!(d.config().batch(), 5);
    }
    println!("task141 fault/body groups=10 passed");
}

use astersql_pd_core::{StoreInfo, StoreMeta, StoreStats};
use astersql_pd_schedule_core::BasicCluster as _;
use astersql_pd_schedule_schedulers::EvictLeaderContext;
const NOW: i128 = 1_000_000_000_000;
fn cluster() -> astersql_pd_core::BasicCluster {
    let c = astersql_pd_core::BasicCluster::new();
    for id in 1..=3 {
        c.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            last_heartbeat: NOW as i64,
            ..Default::default()
        })));
    }
    c
}
fn score(c: &astersql_pd_core::BasicCluster, id: u64, value: u64) {
    let mut s = c.get_store(id).unwrap().as_ref().clone();
    s.set_new_store_stats(StoreStats {
        slow_score: value,
        ..Default::default()
    });
    c.stores.put_store(Arc::new(s));
}
#[tokio::test]
async fn task_141_updated_scheduling() {
    use astersql_pd_core::{KeyRange, Peer, RegionInfo, RegionMeta, RegionOption};
    use astersql_pd_core_constant::PriorityLevel;
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_operator::{OpInfluence, OpKind, OpStep};
    use astersql_pd_schedule_placement::RuleManager;
    for batch in [1, 3, 5, 100] {
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
        score(&c, 1, 100);
        let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
        let paths = ClusterPaths::new(141);
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
        let (mut scheduler, mut network) = schedulers();
        scheduler.observe(&c, &storage, &paths, NOW).await.unwrap();
        let captured = scheduler.config().last_capture();
        let input=serde_json::json!({"batch":batch,"recovery-duration":0,"enable-network-slow-store":true}).to_string();
        assert_eq!(
            call(
                &mut scheduler,
                &mut network,
                &storage,
                "POST",
                "/config",
                &input
            )
            .await
            .0,
            200
        );
        assert_eq!(scheduler.config().last_capture(), captured);
        assert_eq!(scheduler.config().evict_store(), 1);
        let bytes = storage
            .load_scheduler_config(&paths, "evict-slow-store-scheduler")
            .await
            .unwrap();
        let restored: EvictNetworkConfig = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored.disk.batch(), batch);
        assert_eq!(restored.disk.evict_store(), 1);
        assert!(restored.enabled);
        let (status, body, _) =
            call(&mut scheduler, &mut network, &storage, "GET", "/list", "").await;
        assert_eq!(status, 200);
        assert_eq!(
            serde_json::from_str::<Value>(&body).unwrap()["evict-stores"],
            serde_json::json!([1])
        );
        let mut ctx = EvictLeaderContext {
            cluster: &c,
            config: &config,
            rules: &mut rules,
            now_unix_nanos: NOW,
            random_index: &mut random,
            shuffle: &mut shuffle,
        };
        let (result, error) = scheduler.schedule(&mut ctx, &storage, &paths).await;
        assert!(error.is_none());
        assert!(result.errors.is_empty());
        assert_eq!(result.operators.len(), batch);
        let mut ids = std::collections::HashSet::new();
        for op in result.operators {
            assert!(ids.insert(op.region_id()));
            assert_eq!(op.desc(), "evict-slow-store-scheduler");
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
        score(&c, 1, 0);
        let (result, error) = scheduler.schedule(&mut ctx, &storage, &paths).await;
        assert!(error.is_none());
        assert!(result.operators.is_empty());
    }
    println!("task141: 4/4 real operator batch cases (1, 3, 5, 100)");
}

fn network_score(c: &astersql_pd_core::BasicCluster, scores: &[(u64, u64)]) {
    let mut store = c.stores.get_store(1).unwrap().as_ref().clone();
    store.set_new_store_stats(StoreStats {
        network_slow_scores: scores.iter().copied().collect(),
        ..Default::default()
    });
    c.stores.put_store(Arc::new(store));
}
#[tokio::test]
async fn task_141_network_updated_scheduling() {
    use astersql_pd_core::{KeyRange, Peer, RegionInfo, RegionMeta, RegionOption};
    use astersql_pd_core_constant::PriorityLevel;
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_operator::{OpInfluence, OpKind, OpStep};
    use astersql_pd_schedule_placement::RuleManager;
    for batch in [1, 3, 5, 100] {
        let mut c = cluster();
        for id in 4..=5 {
            c.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
                id,
                last_heartbeat: NOW as i64,
                ..Default::default()
            })));
        }
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
        network_score(&c, &[(2, 95), (3, 10), (4, 10)]);
        for _ in 0..6 {
            c.stores.trigger_network_slow_evict(1);
        }
        let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
        let paths = ClusterPaths::new(141);
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
        let (mut disk, mut scheduler) = schedulers();
        assert_eq!(call(&mut disk,&mut scheduler,&storage,"POST","/config",&serde_json::json!({"enable-network-slow-store":true,"recovery-duration":0,"batch":batch}).to_string()).await.0,200);
        let mut ctx = EvictLeaderContext {
            cluster: &c,
            config: &config,
            rules: &mut rules,
            now_unix_nanos: NOW,
            random_index: &mut random,
            shuffle: &mut shuffle,
        };
        let (result, error) = scheduler.schedule(&mut ctx, &storage, &paths).await;
        assert!(error.is_empty());
        assert!(result.errors.is_empty());
        assert_eq!(result.operators.len(), batch);
        let mut ids = std::collections::HashSet::new();
        for op in result.operators {
            assert!(ids.insert(op.region_id()));
            assert_eq!(op.desc(), "evict-slow-store-scheduler");
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
        let clocks = scheduler.recovering().clone();
        let paused = scheduler.config().paused.clone();
        let evicted = scheduler.config().evicted.clone();
        assert_eq!(
            call(&mut disk, &mut scheduler, &storage, "POST", "/config", "{}")
                .await
                .0,
            200
        );
        assert_eq!(*scheduler.recovering(), clocks);
        assert_eq!(scheduler.config().paused, paused);
        assert_eq!(scheduler.config().evicted, evicted);
        let restored: EvictNetworkConfig = serde_json::from_slice(
            &storage
                .load_scheduler_config(&paths, "evict-slow-store-scheduler")
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(restored.paused, vec![1]);
        assert_eq!(restored.evicted, vec![1]);
        network_score(&c, &[]);
        scheduler.schedule(&mut ctx, &storage, &paths).await;
        let (result, error) = scheduler.schedule(&mut ctx, &storage, &paths).await;
        assert!(error.is_empty());
        assert!(result.operators.is_empty());
    }
    println!("task141 network: 4/4 real operator batch cases (1, 3, 5, 100)");
}

#[tokio::test]
async fn task_141_disabled_envelope() {
    let (mut d, mut n) = schedulers();
    let e = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let paths = ClusterPaths::new(141);
    let r = serve(
        &mut d,
        &mut n,
        &e,
        &paths,
        true,
        Request::builder()
            .method("POST")
            .uri("/config")
            .body(Body::from(r#"{"disabled":false,"batch":5}"#))
            .unwrap(),
    )
    .await;
    assert_eq!(r.status(), 200);
    let value: Value = serde_json::from_slice(
        &e.load_scheduler_config(&paths, "evict-slow-store-scheduler")
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(value["disabled"], true);
    assert_eq!(value["batch"], 5);
    let r = serve(
        &mut d,
        &mut n,
        &e,
        &paths,
        true,
        Request::builder().uri("/list").body(Body::empty()).unwrap(),
    )
    .await;
    let value: Value =
        serde_json::from_slice(&to_bytes(r.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(value["disabled"], false);
    println!("task141 disabled envelope=1 passed");
}
