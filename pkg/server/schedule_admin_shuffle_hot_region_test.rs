// Copyright 2026 AsterSQL.
use crate::schedule_admin_shuffle_hot_region::serve;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_schedulers::{ShuffleHotConfig, ShuffleHotScheduler};
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
fn scheduler() -> ShuffleHotScheduler {
    ShuffleHotScheduler::new(ShuffleHotConfig::default())
}
fn request(method: &str, path: &str, body: impl Into<Body>) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .body(body.into())
        .unwrap()
}
#[tokio::test]
async fn task_149_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_149.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let mut s = scheduler();
        let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
        let paths = ClusterPaths::new(149);
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
                .load_scheduler_config(&paths, "shuffle-hot-region-scheduler")
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(durable, serde_json::to_value(s.config()).unwrap(), "{case}");
    }
    println!("task149: 38 source-derived HTTP cases passed");
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
async fn task_149_persistence_and_body_faults() {
    let backend = Arc::new(FaultStore::default());
    let storage = StorageEndpoint::new(backend.clone());
    let paths = ClusterPaths::new(149);
    let mut s = scheduler();
    s.persist(&storage, &paths).await.unwrap();
    let old = s.encode_config().unwrap();
    backend.fail.store(true, Ordering::SeqCst);
    let r = serve(
        &mut s,
        &storage,
        &paths,
        request("POST", "/config", r#"{"limit":8}"#),
    )
    .await;
    assert_eq!(r.status(), 500);
    assert_eq!(
        to_bytes(r.into_body(), usize::MAX).await.unwrap().as_ref(),
        b"\"metadata backend unavailable; mutation outcome may be unknown\""
    );
    assert_eq!(s.encode_config().unwrap(), old);
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
                b"{\n  \"limit\": 1\n}"
            );
        }
    }
    backend.fail.store(false, Ordering::SeqCst);
    assert_eq!(
        storage
            .load_scheduler_config(&paths, "shuffle-hot-region-scheduler")
            .await
            .unwrap(),
        old
    );
    s.reload(&storage, &paths).await.unwrap();
    assert_eq!(s.config().limit, 1);
    let r = serve(
        &mut s,
        &storage,
        &paths,
        request("POST", "/config", r#"{"limit":8}"#),
    )
    .await;
    assert_eq!(r.status(), 200);
    let mut restored = scheduler();
    restored.reload(&storage, &paths).await.unwrap();
    assert_eq!(restored.config().limit, 8);
    for partial in ["", r#"{"limit":99}"#] {
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
        assert_eq!(s.config().limit, 8);
        restored.reload(&storage, &paths).await.unwrap();
        assert_eq!(restored.config().limit, 8);
    }
    println!(
        "task149: save rollback/reload/retry, 3 unread routes and 2 request I/O faults passed"
    );
}
use astersql_pd_core::{
    BasicCluster, DownPeer, Peer, RegionInfo, RegionMeta, StoreInfo, StoreLabel, StoreMeta,
};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_operator::{BuilderError, OpInfluence, OpKind, OpStep};
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_schedule_schedulers::{ShuffleHotContext, ShuffleHotError};
use astersql_pd_statistics::{HotPeerStat, StoreLoadDetail, StoreLoadPred, StoreSummaryInfo};
#[tokio::test]
async fn task_149_schedule_after_http_update() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_100.json"
    ))
    .unwrap();
    let mut count = 0;
    for case in fixture["cases"].as_array().unwrap() {
        for placement in [false, true] {
            for v2 in [false, true] {
                let mut c = BasicCluster::new();
                for id in 1..=5 {
                    if case["missing_source"] == true && id == 1 {
                        continue;
                    }
                    if case["no_target"] == true && id > 3 {
                        continue;
                    }
                    let mut s = StoreInfo::new(StoreMeta {
                        labels: vec![StoreLabel {
                            key: "zone".into(),
                            value: format!(
                                "z{}",
                                if case["placement_target"] == true && id == 5 {
                                    2
                                } else {
                                    id
                                }
                            ),
                        }],
                        id,
                        ..Default::default()
                    });
                    s.set_last_heartbeat_unix_nanos(100);
                    if case["offline"] == true && id > 3 {
                        s.set_store_state(astersql_pd_core::StoreState::Offline, Some(false));
                    }
                    if case["evicted"] == true && id > 3 {
                        s.slow_trend_evicted();
                    }
                    c.stores.put_store(Arc::new(s));
                }
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
                    if case["no_leader"] == true {
                        None
                    } else {
                        Some(peers[0].clone())
                    },
                    if case["down"] == true {
                        vec![DownPeer {
                            peer: peers[1].clone(),
                            down_seconds: 1,
                        }]
                    } else {
                        vec![]
                    },
                    if case["pending"] == true {
                        vec![peers[1].clone()]
                    } else {
                        vec![]
                    },
                ));
                if case["missing"] != true {
                    let (o, v, b) = c.regions.set_region(r.clone());
                    c.regions.update_sub_tree(r.clone(), o.as_deref(), &v, b);
                }
                let mut config = ProviderSnapshot::<()>::default();
                config.replication.max_replicas = 3;
                config.replication.location_labels = vec!["zone".into()].into();
                config.replication.enable_placement_rules = placement;
                let mut rules = RuleManager::new(
                    Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
                    ClusterPaths::new(100),
                );
                rules
                    .initialize(3, &["zone".into()], "", false, &config, &[])
                    .await
                    .unwrap();
                let mut hot_peers = vec![HotPeerStat {
                    region_id: 1,
                    store_id: 2,
                    ..Default::default()
                }]; // Move actual leader, not sampled follower.
                if case["bad_peer"] == true {
                    hot_peers.insert(
                        0,
                        HotPeerStat {
                            region_id: 99,
                            ..Default::default()
                        },
                    );
                }
                if case["empty"] == true {
                    hot_peers.clear();
                }
                let detail = StoreLoadDetail {
                    store_summary_info: StoreSummaryInfo {
                        store_info: c.stores.get_store(2).unwrap().as_ref().clone(),
                        ..Default::default()
                    },
                    load_pred: StoreLoadPred::default(),
                    hot_peers,
                };
                let mut resources: [Vec<StoreLoadDetail>; 4] =
                    std::array::from_fn(|_| vec![detail.clone()]);
                // Prove dispatch uses exactly the selected resource snapshot.
                let selected = case["resource"].as_u64().unwrap() as usize;
                for (index, details) in resources.iter_mut().enumerate() {
                    if index != selected {
                        details.clear();
                    }
                }
                let mut allocate = || {
                    if case["allocation"] == true {
                        Err(BuilderError::Allocation("task100 fault".into()))
                    } else {
                        Ok(100)
                    }
                };
                let mut ctx = ShuffleHotContext {
                    cluster: &c,
                    config: &config,
                    rules: &mut rules,
                    now: 100,
                    supports_conf_change_v2: v2,
                    allocate: &mut allocate,
                };
                let mut s = scheduler();
                let endpoint = StorageEndpoint::new(Arc::new(MemoryStore::default()));
                let paths = ClusterPaths::new(149);
                s.persist(&endpoint, &paths).await.unwrap();
                assert!(!s.is_schedule_allowed([1, 0, 0], 10, 10));
                let response = serve(
                    &mut s,
                    &endpoint,
                    &paths,
                    request("POST", "/config", r#"{"limit":2}"#),
                )
                .await;
                assert_eq!(response.status(), 200);
                assert!(s.is_schedule_allowed([1, 0, 0], 10, 10));
                assert!(!s.is_schedule_allowed([2, 0, 0], 10, 10));
                assert!(!s.is_schedule_allowed([1, 10, 0], 10, 10));
                assert!(!s.is_schedule_allowed([1, 0, 10], 10, 10));
                s.reload(&endpoint, &paths).await.unwrap();
                assert_eq!(s.config().limit, 2);
                let out = s
                    .schedule(
                        &resources,
                        [
                            selected + 4,
                            case["random_peer"].as_u64().unwrap_or(0) as usize,
                            case["random"].as_u64().unwrap_or(0) as usize,
                        ],
                        &mut ctx,
                    )
                    .await;
                assert_eq!(
                    out.operators.len(),
                    case["expected"].as_u64().unwrap() as usize,
                    "{case} placement={placement} v2={v2}, errors={:?}",
                    out.errors
                );
                if case["allocation"] == true {
                    assert!(matches!(
                        out.errors.as_slice(),
                        [ShuffleHotError::Builder(BuilderError::Allocation(_))]
                    ));
                }
                for op in &out.operators {
                    let dst = case["target"].as_u64().unwrap();
                    assert_eq!(op.region_id(), 1);
                    assert_eq!(op.desc(), "random-move-hot-leader");
                    assert_eq!(op.priority_level(), PriorityLevel::LOW);
                    assert_eq!(op.kind(), OpKind::REGION | OpKind::LEADER);
                    let steps: Vec<_> = (0..op.len()).map(|i| op.step(i).unwrap()).collect();
                    assert!(steps.iter().any(
                        |s| matches!(s,OpStep::AddLearner(a) if a.to_store==dst && a.peer_id==100)
                    ));
                    assert!(
                        steps
                            .iter()
                            .any(|s| matches!(s,OpStep::RemovePeer(a) if a.from_store==1))
                    );
                    assert_eq!(
                        steps
                            .iter()
                            .filter_map(|s| if let OpStep::TransferLeader(t) = s {
                                Some(t.to_store)
                            } else {
                                None
                            })
                            .last(),
                        Some(dst)
                    );
                    assert_eq!(op.conf_ver_changed(&r), 0);
                    let influence = OpInfluence::new();
                    op.total_influence(&influence, Some(&r));
                    assert_eq!(influence.get_store_influence(1).region_count, -1);
                    assert_eq!(influence.get_store_influence(dst).region_count, 1);
                    assert_eq!(influence.get_store_influence(1).leader_count, -1);
                    assert_eq!(influence.get_store_influence(dst).leader_count, 1);
                    let add = steps
                        .iter()
                        .find_map(|s| {
                            if let OpStep::AddLearner(a) = s {
                                Some(a)
                            } else {
                                None
                            }
                        })
                        .unwrap();
                    let cmd = add.get_cmd(&r, v2).unwrap();
                    let change = if v2 {
                        cmd.change_peer_v2.unwrap().changes[0].clone().unwrap()
                    } else {
                        cmd.change_peer.unwrap()
                    };
                    assert_eq!(change.peer.unwrap().store_id, dst);
                }
                let response = serve(
                    &mut s,
                    &endpoint,
                    &paths,
                    request("POST", "/config", r#"{"limit":0}"#),
                )
                .await;
                assert_eq!(response.status(), 200);
                assert!(!s.is_schedule_allowed([0, 0, 0], 10, 10));
                count += 1;
            }
        }
    }
    println!("task_149: {count} source-derived behavior cases");
}
