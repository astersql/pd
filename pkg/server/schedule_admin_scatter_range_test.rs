// Copyright 2026 AsterSQL.
use crate::schedule_admin_scatter_range::serve;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_schedulers::{ScatterRangeConfig, ScatterRangeScheduler};
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
fn scheduler() -> ScatterRangeScheduler {
    ScatterRangeScheduler::new(ScatterRangeConfig::from_args(&["test", "a", "b"]).unwrap())
}
fn request(method: &str, path: &str, body: impl Into<Body>) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .body(body.into())
        .unwrap()
}
#[tokio::test]
async fn task_148_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_148.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let mut s = scheduler();
        let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
        let paths = ClusterPaths::new(148);
        s.persist(&storage, &paths).await.unwrap();
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
                .load_scheduler_config(&paths, s.name())
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(durable, serde_json::to_value(s.config()).unwrap(), "{case}");
    }
    println!("task148: 31 source-derived HTTP cases passed");
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
async fn task_148_persistence_and_body_faults() {
    let backend = Arc::new(FaultStore::default());
    let storage = StorageEndpoint::new(backend.clone());
    let paths = ClusterPaths::new(148);
    let mut s = scheduler();
    s.persist(&storage, &paths).await.unwrap();
    let old = s.encode_config().unwrap();
    backend.fail.store(true, Ordering::SeqCst);
    let r = serve(
        &mut s,
        &storage,
        &paths,
        request("POST", "/config", r#"{"start-key":"x","end-key":"z"}"#),
    )
    .await;
    assert_eq!(r.status(), 500);
    assert_eq!(
        to_bytes(r.into_body(), usize::MAX).await.unwrap().as_ref(),
        b"\"metadata backend unavailable; mutation outcome may be unknown\"null"
    );
    assert_eq!(s.config().start_key, "x");
    backend.fail.store(false, Ordering::SeqCst);
    assert_eq!(
        storage
            .load_scheduler_config(&paths, s.name())
            .await
            .unwrap(),
        old
    );
    s.reload(&storage, &paths).await.unwrap();
    assert_eq!(s.config().start_key, "a");
    let r = serve(
        &mut s,
        &storage,
        &paths,
        request("POST", "/config", r#"{"start-key":"x"}"#),
    )
    .await;
    assert_eq!(r.status(), 200);
    s.reload(&storage, &paths).await.unwrap();
    assert_eq!(s.config().start_key, "x");
    for partial in ["", r#"{"start-key":"BAD"}"#] {
        let stream = futures_util::stream::iter(vec![
            Ok(axum::body::Bytes::from(partial.to_owned())),
            Err(std::io::Error::other("injected body read failure")),
        ]);
        let before = s.encode_config().unwrap();
        let r = serve(
            &mut s,
            &storage,
            &paths,
            request("POST", "/config", Body::from_stream(stream)),
        )
        .await;
        assert_eq!(r.status(), 500);
        assert_eq!(r.headers()["tidb-error-code"], "internal");
        let v: Value =
            serde_json::from_slice(&to_bytes(r.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert_eq!(v["code"], "internal");
        assert_eq!(v["msg"], "injected body read failure");
        assert_eq!(v["data"], serde_json::json!({}));
        assert!(v["stack"].is_array());
        assert_eq!(s.encode_config().unwrap(), before);
    }
    for (method, path) in [("GET", "/list"), ("POST", "/missing"), ("GET", "/config")] {
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
        assert_eq!(
            r.status(),
            if path == "/list" {
                200
            } else if path == "/missing" {
                404
            } else {
                405
            }
        );
    }
    let r = serve(
        &mut s,
        &storage,
        &paths,
        request(
            "POST",
            "/config",
            r#"{"start-key":"<&>\u2028","end-key":"\u0000"}"#,
        ),
    )
    .await;
    assert_eq!(r.status(), 200);
    let r = serve(&mut s, &storage, &paths, request("GET", "/list?x=1", "")).await;
    assert_eq!(to_bytes(r.into_body(),usize::MAX).await.unwrap().as_ref(),b"{\n  \"range-name\": \"test\",\n  \"start-key\": \"\\u003c\\u0026\\u003e\\u2028\",\n  \"end-key\": \"\\u0000\"\n}");
    println!(
        "task148: failed save retains local keys, old durable bytes and reload/retry; 2 body read faults; 3 unread routes; exact escaped listing passed"
    );
}

use astersql_pd_core::{
    BasicCluster, KeyRange, Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta, StoreStats,
};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_filter::AffinityFilter;
use astersql_pd_schedule_operator::{OpInfluence, OpKind, OpStep};
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_schedule_schedulers::RegionBalanceContext;
use astersql_pd_statistics::StatisticsSnapshot;
#[tokio::test]
async fn task_148_schedule_after_http_update() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_148.json"
    ))
    .unwrap();
    let mut count = 0;
    for case in fixture["scheduling_cases"].as_array().unwrap() {
        for placement in [false, true] {
            for v2 in [false, true] {
                for updated in [false, true] {
                    let mut cluster = BasicCluster::new();
                    for id in 1..=5 {
                        let mut store = StoreInfo::new(StoreMeta {
                            id,
                            ..Default::default()
                        });
                        store.set_last_heartbeat_unix_nanos(1_000_000_000_000);
                        // Global scores deliberately disagree with the range; nonzero used enables projection.
                        store.set_region_size(1000);
                        store.set_region_count(1000);
                        store.set_leader_count(if id == 5 { 1000 } else { 0 });
                        store.update_raw_stats(StoreStats {
                            capacity: 6 * (1 << 40),
                            available: 6 * (1 << 40) - (1000 << 20),
                            used_size: 1000 << 20,
                            ..Default::default()
                        });
                        cluster.stores.put_store(Arc::new(store));
                    }
                    for id in 1..=24 {
                        let peers: Vec<_> = (1..=3)
                            .map(|s| Peer {
                                id: id * 10 + s,
                                store_id: s,
                                ..Default::default()
                            })
                            .collect();
                        let leader = 0;
                        let mut region = RegionInfo::new(
                            RegionMeta {
                                id,
                                key_range: KeyRange {
                                    start_key: format!("a{id:03}").into_bytes(),
                                    end_key: format!("a{:03}", id + 1).into_bytes(),
                                },
                                peers: peers.clone(),
                                ..Default::default()
                            },
                            Some(peers[leader].clone()),
                            vec![],
                            vec![],
                        );
                        region.stats_mut().approximate_size = 1;
                        let r = Arc::new(region);
                        let (o, v, b) = cluster.regions.set_region(r.clone());
                        cluster.regions.update_sub_tree(r, o.as_deref(), &v, b);
                    }
                    let mut config = ProviderSnapshot::<()>::default();
                    config.schedule.tolerant_size_ratio = 10000.;
                    config.schedule.leader_schedule_limit = case["leader"].as_u64().unwrap();
                    config.schedule.region_schedule_limit = case["region"].as_u64().unwrap();
                    config.schedule.leader_schedule_policy = "count".into();
                    config.schedule.region_score_formula_version = "v1".into();
                    config.replication.enable_placement_rules = placement;
                    config.replication.max_replicas = 3;
                    let storage = Arc::new(MemoryStore::default());
                    let endpoint = Arc::new(StorageEndpoint::new(storage.clone()));
                    let paths = ClusterPaths::new(148);
                    let mut rules = RuleManager::new(endpoint.clone(), paths.clone());
                    rules
                        .initialize(3, &[], "", false, &config, &[])
                        .await
                        .unwrap();
                    let labels = astersql_pd_schedule_labeler::RegionLabeler::load(
                        StorageEndpoint::new(storage),
                        paths,
                        0,
                    )
                    .await
                    .unwrap();
                    let key_ranges = astersql_pd_schedule_keyrange::Manager::new();
                    let influence = OpInfluence::new();
                    let before = influence.snapshot();
                    let stats = StatisticsSnapshot::default();
                    let mut index = |_: usize| 0;
                    let mut shuffle = |_: &mut [usize]| {};
                    let mut next = 1000;
                    let mut allocate = || {
                        next += 1;
                        Ok(next)
                    };
                    let mut scheduler = ScatterRangeScheduler::new(
                        ScatterRangeConfig::from_args(&["test", "x", "z"]).unwrap(),
                    );
                    scheduler
                        .persist(&endpoint, &ClusterPaths::new(148))
                        .await
                        .unwrap();
                    if updated {
                        let r = serve(
                            &mut scheduler,
                            &endpoint,
                            &ClusterPaths::new(148),
                            request("POST", "/config", r#"{"start-key":"a","end-key":"b"}"#),
                        )
                        .await;
                        assert_eq!(r.status(), 200);
                        assert_eq!(
                            to_bytes(r.into_body(), usize::MAX).await.unwrap().as_ref(),
                            b"null"
                        );
                    }
                    let durable: ScatterRangeConfig = serde_json::from_slice(
                        &endpoint
                            .load_scheduler_config(&ClusterPaths::new(148), scheduler.name())
                            .await
                            .unwrap(),
                    )
                    .unwrap();
                    assert_eq!(&durable, scheduler.config());
                    let result = scheduler
                        .schedule(
                            &mut RegionBalanceContext {
                                cluster: &cluster,
                                config: &config,
                                rules: &mut rules,
                                affinity: AffinityFilter {
                                    manager: None,
                                    labels: &labels,
                                    now: 1000,
                                },
                                statistics: &stats,
                                hot_degree: 1,
                                key_ranges: &key_ranges,
                                influence: &influence,
                                now_unix_nanos: 1_000_000_000_000,
                                random_index: &mut index,
                                shuffle: &mut shuffle,
                                allocate: &mut allocate,
                                supports_conf_change_v2: v2,
                                allow_empty_regions: false,
                            },
                            case["count"].as_u64().unwrap(),
                        )
                        .await;
                    let expected = if updated {
                        case["expected"].as_str().unwrap()
                    } else {
                        "none"
                    };
                    assert_eq!(
                        !result.operators.is_empty(),
                        matches!(expected, "leader" | "region"),
                        "{case} placement={placement} errors={:?}",
                        result.errors
                    );
                    assert_eq!(!result.errors.is_empty(), expected == "error", "{case}");
                    assert!(result.plans.is_empty());
                    assert_eq!(influence.snapshot(), before);
                    assert_eq!(config.schedule.tolerant_size_ratio, 10000.);
                    if let Some(op) = result.operators.first() {
                        assert!(op.region_id() <= 24);
                        assert_eq!(op.desc(), format!("scatter-range-{expected}-test"));
                        assert!(op.kind().0 & OpKind::RANGE.0 != 0);
                        if expected == "leader" {
                            assert_eq!(op.kind(), OpKind::LEADER | OpKind::RANGE);
                            let OpStep::TransferLeader(step) = op.step(0).unwrap() else {
                                panic!("transfer expected")
                            };
                            assert_eq!(step.from_store, 1);
                            assert!([2, 3].contains(&step.to_store));
                        } else {
                            assert!(op.kind().0 & OpKind::REGION.0 != 0);
                            assert!(op.len() >= 3);
                            let OpStep::AddLearner(add) = op.step(0).unwrap() else {
                                panic!("add learner expected")
                            };
                            assert!([4, 5].contains(&add.to_store));
                            assert!(add.peer_id > 1000);
                            let region = cluster.regions.get_region(op.region_id()).unwrap();
                            let delta = OpInfluence::new();
                            for i in 0..op.len() {
                                op.step(i).unwrap().influence(&delta, &region);
                            }
                            assert_eq!(delta.get_store_influence(add.to_store).region_count, 1);
                            assert_eq!(
                                delta
                                    .snapshot()
                                    .values()
                                    .map(|v| v.region_count)
                                    .sum::<i64>(),
                                0
                            );
                            assert_eq!(op.step(0).unwrap().conf_ver_changed(&region), 0);
                        }
                        // Only the first child operator is decorated, as in Go.
                        for other in result.operators.iter().skip(1) {
                            assert_eq!(other.kind().0 & OpKind::RANGE.0, 0);
                            assert_eq!(other.desc(), "scatter-range-leader");
                        }
                    }
                    count += 1;
                }
            }
        }
    }
    println!("task_148 effective behavior cases: {count}");
}
