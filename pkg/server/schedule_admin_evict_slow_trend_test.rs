// Copyright 2026 AsterSQL.
use crate::schedule_admin_evict_slow_trend::serve;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_schedulers::{SlowTrendConfig, SlowTrendScheduler};
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::{
    Commit, Compare, Lease, MemoryStore, MetaWatch, Mutation, RawTxnOp, RawTxnResponse, Read,
    Snapshot, StoreError,
};
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use serde_json::Value;
use std::sync::{Arc, atomic::Ordering};
fn scheduler() -> SlowTrendScheduler {
    SlowTrendScheduler::new(SlowTrendConfig::default())
}
fn request(method: &str, path: &str, body: impl Into<Body>) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .body(body.into())
        .unwrap()
}
#[tokio::test]
async fn task_153_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_153.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let mut s = scheduler();
        let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
        let paths = ClusterPaths::new(153);
        s.update_recovery(1800, &storage, &paths).await.unwrap();
        let response = serve(
            &mut s,
            &storage,
            &paths,
            request(
                case["method"].as_str().unwrap(),
                case["path"].as_str().unwrap(),
                Body::from(case["input"].as_str().unwrap().to_owned()),
            ),
        )
        .await;
        assert_eq!(
            response.status().as_u16(),
            case["status"].as_u64().unwrap() as u16,
            "{case}"
        );
        assert!(response.headers().get("allow").is_none());
        assert_eq!(
            response
                .headers()
                .get("tidb-error-code")
                .map(|v| v.to_str().unwrap()),
            case["code"].as_str()
        );
        if case["status"] != 405 {
            assert_eq!(
                response.headers()["content-type"],
                if case["status"] == 404 {
                    "text/plain; charset=utf-8"
                } else {
                    "application/json; charset=UTF-8"
                }
            );
        }
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        if case["body"].is_object() {
            assert_eq!(
                serde_json::from_slice::<Value>(&body).unwrap(),
                case["body"],
                "{case}"
            );
        } else {
            assert_eq!(
                body.as_ref(),
                case["body"].as_str().unwrap().as_bytes(),
                "{case}"
            );
        }
        if !case["config"].is_null() {
            assert_eq!(
                serde_json::to_value(s.config()).unwrap(),
                case["config"],
                "{case}"
            );
        }
        let durable: Value = serde_json::from_slice(
            &storage
                .load_scheduler_config(&paths, "evict-slow-trend-scheduler")
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(durable, serde_json::to_value(s.config()).unwrap(), "{case}");
    }
    println!("task153: 39 source-derived HTTP cases passed");
}

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
async fn task_153_persistence_and_body_faults() {
    let backend = Arc::new(FaultStore::default());
    let storage = StorageEndpoint::new(backend.clone());
    let paths = ClusterPaths::new(153);
    let mut s = scheduler();
    s.update_recovery(1800, &storage, &paths).await.unwrap();
    let old = storage
        .load_scheduler_config(&paths, "evict-slow-trend-scheduler")
        .await
        .unwrap();
    backend.fail.store(true, Ordering::SeqCst);
    let r = serve(
        &mut s,
        &storage,
        &paths,
        request("POST", "/config", r#"{"recovery-duration":8}"#),
    )
    .await;
    assert_eq!(r.status(), 500);
    assert_eq!(
        to_bytes(r.into_body(), usize::MAX).await.unwrap().as_ref(),
        b"\"metadata backend unavailable; mutation outcome may be unknown\""
    );
    assert_eq!(
        serde_json::to_value(s.config()).unwrap(),
        serde_json::from_slice::<Value>(&old).unwrap()
    );
    // GET/list and rejected routes never touch a failed store or read a body.
    for (method, path, status) in [
        ("GET", "/list", 200),
        ("GET", "/config", 405),
        ("POST", "/unknown", 404),
    ] {
        let stream = futures_util::stream::poll_fn(
            |_| -> std::task::Poll<Option<Result<axum::body::Bytes, std::io::Error>>> {
                panic!("route must not poll body")
            },
        );
        let r = serve(
            &mut s,
            &storage,
            &paths,
            request(method, path, Body::from_stream(stream)),
        )
        .await;
        assert_eq!(r.status(), status);
        if status == 200 {
            assert_eq!(
                to_bytes(r.into_body(), usize::MAX).await.unwrap().as_ref(),
                b"{\n  \"recovery-duration\": 1800,\n  \"evict-by-trend-stores\": null\n}"
            );
        }
    }
    backend.fail.store(false, Ordering::SeqCst);
    assert_eq!(
        storage
            .load_scheduler_config(&paths, "evict-slow-trend-scheduler")
            .await
            .unwrap(),
        old
    );

    assert_eq!(s.config().recovery_sec, 1800);
    let r = serve(
        &mut s,
        &storage,
        &paths,
        request("POST", "/config", r#"{"recovery-duration":8}"#),
    )
    .await;
    assert_eq!(r.status(), 200);
    let restored = SlowTrendScheduler::new(
        serde_json::from_slice(
            &storage
                .load_scheduler_config(&paths, "evict-slow-trend-scheduler")
                .await
                .unwrap(),
        )
        .unwrap(),
    );
    assert_eq!(restored.config().recovery_sec, 8);
    for partial in ["", r#"{"recovery-duration":99}"#] {
        let stream = futures_util::stream::iter(vec![
            Ok(axum::body::Bytes::from(partial.to_owned())),
            Err(std::io::Error::other("injected body read failure")),
        ]);
        let r = serve(
            &mut s,
            &storage,
            &paths,
            request("POST", "/config", Body::from_stream(stream)),
        )
        .await;
        assert_eq!(r.status(), 500);
        assert_eq!(r.headers()["tidb-error-code"], "internal");
        let body: Value =
            serde_json::from_slice(&to_bytes(r.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert_eq!(body["code"], "internal");
        assert_eq!(body["msg"], "injected body read failure");
        assert_eq!(body["data"], serde_json::json!({}));
        assert!(!body["stack"].as_array().unwrap().is_empty());
        assert_eq!(s.config().recovery_sec, 8);
        let restored = SlowTrendScheduler::new(
            serde_json::from_slice(
                &storage
                    .load_scheduler_config(&paths, "evict-slow-trend-scheduler")
                    .await
                    .unwrap(),
            )
            .unwrap(),
        );
        assert_eq!(restored.config().recovery_sec, 8);
    }
    println!(
        "task153: save rollback/reload/retry, 3 unread routes and 2 request I/O faults passed"
    );
}

use astersql_pd_core::{BasicCluster, SlowTrend, StoreInfo, StoreMeta, StoreStats};
use astersql_pd_schedule_schedulers::EvictLeaderContext;
fn cluster() -> BasicCluster {
    let c = BasicCluster::new();
    for id in 1..=3 {
        c.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            last_heartbeat: 0,
            ..Default::default()
        })));
        trend(
            &c,
            id,
            if id == 1 { 100. } else { 10. },
            if id == 1 { 1. } else { 0. },
            -1.,
        );
    }
    c
}
fn trend(c: &BasicCluster, id: u64, value: f64, cause: f64, result: f64) {
    let mut s = c.stores.get_store(id).unwrap().as_ref().clone();
    s.set_new_store_stats(StoreStats {
        slow_trend: Some(SlowTrend {
            cause_value: value,
            cause_rate: cause,
            result_rate: result,
            ..Default::default()
        }),
        ..Default::default()
    });
    c.stores.put_store(Arc::new(s));
}
fn heartbeats(c: &BasicCluster, now: i64) {
    for id in 2..=3 {
        let mut s = c.stores.get_store(id).unwrap().as_ref().clone();
        s.set_last_heartbeat_unix_nanos(now);
        c.stores.put_store(Arc::new(s));
    }
}
use astersql_pd_schedule_core::BasicCluster as _;
const NOW: i128 = 1_000_000_000_000;
#[tokio::test]
async fn task_153_schedule_after_http_update() {
    use astersql_pd_core::{KeyRange, Peer, RegionInfo, RegionMeta, RegionOption};
    use astersql_pd_core_constant::PriorityLevel;
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_operator::{OpInfluence, OpKind, OpStep};
    use astersql_pd_schedule_placement::RuleManager;
    for batch in [3] {
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
        heartbeats(&c, NOW as i64);
        let backend = Arc::new(FaultStore::default());
        let storage = StorageEndpoint::new(backend.clone());
        let paths = ClusterPaths::new(153);
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
        let mut scheduler = SlowTrendScheduler::new(SlowTrendConfig {
            recovery_sec: 1800,
            ..Default::default()
        });
        let mut ctx = EvictLeaderContext {
            cluster: &c,
            config: &config,
            rules: &mut rules,
            now_unix_nanos: NOW,
            random_index: &mut random,
            shuffle: &mut shuffle,
        };
        assert_eq!(
            serve(
                &mut scheduler,
                &storage,
                &paths,
                request("POST", "/config", r#"{"recovery-duration":100}"#)
            )
            .await
            .status(),
            200
        );
        for id in [2, 3] {
            c.stores
                .pause_leader_transfer(id, astersql_pd_core_constant::Direction::IN)
                .unwrap();
        }
        let (blocked, error) = scheduler
            .schedule(&mut ctx, &storage, &paths, NOW as i64, false, 0.0)
            .await;
        assert!(error.is_none());
        assert!(blocked.operators.is_empty());
        assert_eq!(scheduler.config().evicted_store(), 1);
        for id in [2, 3] {
            c.stores
                .resume_leader_transfer(id, astersql_pd_core_constant::Direction::IN);
        }
        let (result, error) = scheduler
            .schedule(&mut ctx, &storage, &paths, NOW as i64, false, 0.0)
            .await;
        assert!(error.is_none());
        assert!(result.errors.is_empty());
        assert_eq!(result.operators.len(), batch);
        let mut ids = std::collections::HashSet::new();
        for op in result.operators {
            assert!(ids.insert(op.region_id()));
            assert_eq!(op.desc(), "evict-slow-trend-scheduler");
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
        let history = scheduler.candidates().clone();
        backend.fail.store(true, Ordering::SeqCst);
        let failed = serve(
            &mut scheduler,
            &storage,
            &paths,
            request("POST", "/config", r#"{"recovery-duration":0}"#),
        )
        .await;
        assert_eq!(failed.status(), 500);
        assert_eq!(scheduler.config().recovery_sec, 100);
        assert_eq!(scheduler.config().evicted_stores, vec![1]);
        assert_eq!(scheduler.candidates(), &history);
        backend.fail.store(false, Ordering::SeqCst);
        let saved: SlowTrendConfig = serde_json::from_slice(
            &storage
                .load_scheduler_config(&paths, "evict-slow-trend-scheduler")
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(saved.recovery_sec, 100);
        assert_eq!(saved.evicted_stores, vec![1]);
        let listed = serve(
            &mut scheduler,
            &storage,
            &paths,
            request("GET", "/list", ""),
        )
        .await;
        assert_eq!(
            to_bytes(listed.into_body(), usize::MAX)
                .await
                .unwrap()
                .as_ref(),
            b"{\n  \"recovery-duration\": 100,\n  \"evict-by-trend-stores\": null\n}"
        );
        trend(&c, 1, 10., 0., 0.);
        let (_, error) = scheduler
            .schedule(&mut ctx, &storage, &paths, NOW as i64, false, 0.0)
            .await;
        assert!(error.is_none());
        assert_eq!(scheduler.config().evicted_store(), 1);
        assert_eq!(
            serve(
                &mut scheduler,
                &storage,
                &paths,
                request(
                    "POST",
                    "/config",
                    r#"{"recovery-duration":0,"evict-by-trend-stores":[99]}"#
                )
            )
            .await
            .status(),
            200
        );
        assert_eq!(scheduler.candidates(), &history);
        assert_eq!(scheduler.config().evicted_stores, vec![1]);
        let saved: SlowTrendConfig = serde_json::from_slice(
            &storage
                .load_scheduler_config(&paths, "evict-slow-trend-scheduler")
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(saved.recovery_sec, 0);
        assert_eq!(saved.evicted_stores, vec![1]);
        scheduler
            .schedule(&mut ctx, &storage, &paths, NOW as i64, false, 0.0)
            .await;
        let (result, error) = scheduler
            .schedule(&mut ctx, &storage, &paths, NOW as i64, false, 0.0)
            .await;
        assert!(error.is_none());
        assert!(result.operators.is_empty());
        assert_eq!(scheduler.config().evicted_store(), 0);
        assert!(!c.stores.get_store(1).unwrap().is_evicted_as_slow_trend());
    }
    println!("task_153: 1/1 real operator batch case (3)");
}

#[tokio::test]
async fn task_153_captured_candidate_preserved() {
    let c = cluster();
    let mut s = scheduler();
    let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let paths = ClusterPaths::new(153);
    assert!(
        !s.observe(&c, &storage, &paths, NOW as i64, false, 0.0)
            .await
            .unwrap()
    );
    let history = s.candidates().clone();
    assert_eq!(history.current.unwrap().store_id, 1);
    assert_eq!(
        serve(
            &mut s,
            &storage,
            &paths,
            request("POST", "/config", r#"{"recovery-duration":9}"#)
        )
        .await
        .status(),
        200
    );
    assert_eq!(s.candidates(), &history);
    assert_eq!(s.config().evicted_store(), 0);
    heartbeats(&c, NOW as i64);
    assert!(
        s.observe(&c, &storage, &paths, NOW as i64, false, 0.0)
            .await
            .unwrap()
    );
    assert_eq!(s.candidates().last, history.last);
    assert_eq!(s.candidates().current, None);
    assert_eq!(s.config().evicted_store(), 1);
    assert_eq!(s.config().recovery_sec, 9);
    assert!(c.stores.get_store(1).unwrap().is_evicted_as_slow_trend());
    println!("task153: captured candidate survives HTTP update and confirms eviction");
}
