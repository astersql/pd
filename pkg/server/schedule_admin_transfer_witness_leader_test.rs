// Copyright 2026 AsterSQL.
use crate::schedule_admin_transfer_witness_leader::serve;
use astersql_pd_core::{
    BasicCluster, DownPeer, Peer, PeerRole, RegionInfo, RegionMeta, StoreInfo, StoreMeta,
};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_operator::{OpKind, OpStep};
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_schedule_schedulers::{
    EvictLeaderContext, TransferWitnessLeaderError, TransferWitnessLeaderScheduler,
};
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use axum::{
    body::{Body, to_bytes},
    http::Request,
    response::Response,
};
use serde_json::Value;
use std::sync::Arc;
async fn assert_response(response: Response) {
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers()["content-type"],
        "text/plain; charset=utf-8"
    );
    assert!(response.headers().get("allow").is_none());
    assert!(response.headers().get("tidb-error-code").is_none());
    assert_eq!(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .as_ref(),
        b"not implements"
    );
}

#[tokio::test]
async fn task_152_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_152.json"
    ))
    .unwrap();
    for queued in [0_usize, 1, 4] {
        let mut scheduler = TransferWitnessLeaderScheduler::new();
        for _ in 0..queued {
            scheduler
                .recv_region_info()
                .send(region(&serde_json::json!({})))
                .unwrap();
        }
        for case in fixture["cases"].as_array().unwrap() {
            let response = serve(
                &scheduler,
                Request::builder()
                    .method(case["method"].as_str().unwrap())
                    .uri(case["path"].as_str().unwrap())
                    .body(Body::from(case["input"].as_str().unwrap().to_owned()))
                    .unwrap(),
            );
            assert_eq!(
                response.status().as_u16(),
                case["status"].as_u64().unwrap() as u16,
                "{case}"
            );
            assert_eq!(
                response.headers()["content-type"],
                case["content_type"].as_str().unwrap(),
                "{case}"
            );
            assert!(response.headers().get("allow").is_none());
            assert!(response.headers().get("tidb-error-code").is_none());
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            assert_eq!(
                body.as_ref(),
                case["body"].as_str().unwrap().as_bytes(),
                "{case}"
            );
            assert!(
                serde_json::from_slice::<Value>(&body).is_err(),
                "response is plain text"
            );
        }
        let c = cluster(&serde_json::json!({})).await;
        let conf = ProviderSnapshot::<u64>::default();
        let mut rules = RuleManager::new(
            Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
            ClusterPaths::new(152),
        );
        let mut pick = |_| 0;
        let mut shuffle = |_: &mut [usize]| {};
        let mut ctx = EvictLeaderContext {
            cluster: &c,
            config: &conf,
            rules: &mut rules,
            now_unix_nanos: 1_000_000_000_000,
            random_index: &mut pick,
            shuffle: &mut shuffle,
        };
        for expected in [queued.min(3), queued.saturating_sub(3), 0] {
            let out = scheduler.schedule(&mut ctx).await;
            assert!(out.errors.is_empty());
            assert_eq!(out.operators.len(), expected);
        }
    }
    println!("task152 HTTP: 22 cases x 3 queue states = 66 passed");
}

#[tokio::test]
async fn task_152_unread_body_faults() {
    let scheduler = TransferWitnessLeaderScheduler::new();
    for method in ["POST", "GET", "DELETE", "HEAD"] {
        // Polling a failing or stalled external stream would violate BaseScheduler.
        let stream = futures_util::stream::poll_fn(
            |_| -> std::task::Poll<Option<Result<axum::body::Bytes, std::io::Error>>> {
                panic!(
                    "unsupported transfer_witness_leader endpoint must not poll the request body"
                )
            },
        );
        assert_response(serve(
            &scheduler,
            Request::builder()
                .method(method)
                .uri("/config")
                .body(Body::from_stream(stream))
                .unwrap(),
        ))
        .await;
    }
    println!("task152 unread-body faults: 4 passed");
}

fn region(case: &serde_json::Value) -> Arc<RegionInfo> {
    let peers: Vec<_> = (1..=3)
        .map(|id| Peer {
            id,
            store_id: id,
            role: if id == 2 && case["learner"] == true {
                PeerRole::Learner
            } else if id == 2 && case["demoting"] == true {
                PeerRole::DemotingVoter
            } else {
                PeerRole::Voter
            },
            is_witness: (id == 1 && case["ordinary"] != true)
                || (id == 2 && case["witness"] == true),
        })
        .collect();
    Arc::new(RegionInfo::new(
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
                peer: peers[2].clone(),
                down_seconds: 1000,
            }]
        } else {
            vec![]
        },
        if case["pending"] == true {
            vec![peers[1].clone()]
        } else {
            vec![]
        },
    ))
}
async fn cluster(case: &serde_json::Value) -> BasicCluster {
    let c = BasicCluster::new();
    for id in 1..=3 {
        if id > 1 && case["missing_stores"] == true {
            continue;
        }
        let mut s = StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        });
        s.set_last_heartbeat_unix_nanos(if id == 2 && case["disconnected"] == true {
            1
        } else {
            1_000_000_000_000
        });
        if id == 2 && case["offline"] == true {
            s.set_store_state(astersql_pd_core::StoreState::Offline, Some(false));
        }
        if id > 1 && case["budget"] == true {
            let limit = astersql_pd_storelimit::StoreRateLimit::new(0.000000001);
            assert!(
                limit
                    .take(
                        1000,
                        astersql_pd_storelimit::Type::TRANSFER_LEADER_IN,
                        PriorityLevel::URGENT
                    )
                    .await
            );
            s.set_store_limit(Some(astersql_pd_core::StoreLimiter::V1(limit)));
        }
        c.stores.put_store(Arc::new(s));
    }
    c
}
#[tokio::test]
async fn task_152_schedule_after_unsupported_update() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_103.json"
    ))
    .unwrap();
    for case in f["cases"].as_array().unwrap() {
        for after in [false, true] {
            let c = cluster(case).await;
            let conf = ProviderSnapshot::<u64>::default();
            let mut rules = RuleManager::new(
                Arc::new(astersql_pd_storage_endpoint::StorageEndpoint::new(
                    Arc::new(astersql_pd_storage_kv::MemoryStore::default()),
                )),
                astersql_pd_keypath::ClusterPaths::new(103),
            );
            let r = region(case);
            let mut s = TransferWitnessLeaderScheduler::new();
            s.recv_region_info().send(r.clone()).unwrap();
            if after {
                assert_response(serve(
                    &s,
                    Request::builder()
                        .method("POST")
                        .uri("/config")
                        .body(Body::from(
                            r#"{"disable":true,"batch-size":0,"regions":[]}"#,
                        ))
                        .unwrap(),
                ))
                .await;
            }
            let mut pick = |n| if case["last"] == true { n - 1 } else { 0 };
            let mut shuffle = |_: &mut [usize]| {};
            let mut ctx = EvictLeaderContext {
                cluster: &c,
                config: &conf,
                rules: &mut rules,
                now_unix_nanos: 1_000_000_000_000,
                random_index: &mut pick,
                shuffle: &mut shuffle,
            };
            let out = s.schedule(&mut ctx).await;
            assert_eq!(
                out.operators.len(),
                case["ops"].as_u64().unwrap() as usize,
                "{case}, {:?}",
                out.errors
            );
            assert_eq!(out.errors.len(), usize::from(case["ops"] == 0), "{case}");
            if case["no_leader"] == true {
                assert!(matches!(
                    out.errors.as_slice(),
                    [TransferWitnessLeaderError::Builder(
                        astersql_pd_schedule_operator::BuilderError::NoLeader
                    )]
                ));
            } else if case["witness"] == true || case["demoting"] == true {
                assert!(matches!(
                    out.errors.as_slice(),
                    [TransferWitnessLeaderError::Builder(
                        astersql_pd_schedule_operator::BuilderError::LeaderNotAllowed
                    )]
                ));
            } else if case["ops"] == 0 {
                assert!(matches!(
                    out.errors.as_slice(),
                    [TransferWitnessLeaderError::NoTarget]
                ));
            }
            if let Some(op) = out.operators.first() {
                assert_eq!(
                    op.kind(),
                    OpKind(OpKind::LEADER.0 | OpKind::WITNESS_LEADER.0)
                );
                assert_eq!(op.priority_level(), PriorityLevel::URGENT);
                assert_eq!(op.desc(), "transfer-witness-leader-scheduler");
                assert_eq!(op.region_id(), 1);
                assert_eq!(op.len(), 1);
                let OpStep::TransferLeader(step) = op.step(0).unwrap() else {
                    panic!()
                };
                assert_eq!(step.from_store, 1);
                assert_eq!(step.to_store, case["target"].as_u64().unwrap());
                assert_eq!(
                    step.to_stores,
                    serde_json::from_value::<Vec<u64>>(case["targets"].clone()).unwrap()
                );
                assert!(!step.is_finish(&r));
                assert_eq!(step.conf_ver_changed(&r), 0);
                for v2 in [false, true] {
                    let cmd = step.get_cmd(&r, v2).transfer_leader.unwrap();
                    assert_eq!(cmd.peer.unwrap().store_id, step.to_store);
                    assert_eq!(
                        cmd.peers
                            .iter()
                            .map(|p| p.as_ref().unwrap().store_id)
                            .collect::<Vec<_>>(),
                        step.to_stores
                    );
                }
                let influence = astersql_pd_schedule_operator::OpInfluence::new();
                step.influence(&influence, &r);
                assert_eq!(influence.get_store_influence(1).leader_count, -1);
                assert_eq!(influence.get_store_influence(step.to_store).leader_count, 1);
                let missing = BasicCluster::new();
                assert!(
                    step.check_in_progress(&missing, &conf, &r, true, 1_000_000_000_000)
                        .is_err()
                );
                assert!(
                    step.check_in_progress(&c, &conf, &r, true, 1_000_000_000_000)
                        .is_ok()
                );
                assert!(op.start());
                assert!(op.check(&r).is_some());
                for id in &step.to_stores {
                    let ack = RegionInfo::new(
                        r.meta().clone(),
                        r.get_store_peer(*id).cloned(),
                        vec![],
                        vec![],
                    );
                    assert!(step.is_finish(&ack));
                }
                let ack = RegionInfo::new(
                    r.meta().clone(),
                    r.get_store_peer(step.to_store).cloned(),
                    vec![],
                    vec![],
                );
                assert!(op.check(&ack).is_none());
                assert!(op.check_success());
                assert_eq!(r.leader().unwrap().store_id, 1);
            }
            assert!(s.schedule(&mut ctx).await.operators.is_empty());
        }
    }
    println!(
        "task152: {} source-derived cases x before/after = 28 decisions, both heartbeat modes passed",
        f["cases"].as_array().unwrap().len()
    );
}
#[tokio::test]
async fn task_152_persistence_faults() {
    let backend = Arc::new(FaultStore::default());
    let storage = Arc::new(StorageEndpoint::new(backend.clone()));
    let paths = ClusterPaths::new(152);
    let name = "transfer-witness-leader-scheduler";
    // BaseScheduler.EncodeConfig serializes nil, not an invented witness config.
    storage
        .save_scheduler_config(&paths, name, b"null")
        .await
        .unwrap();
    backend
        .fail
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(
        storage
            .save_scheduler_config(&paths, name, b"{}")
            .await
            .is_err()
    );
    assert!(storage.load_scheduler_config(&paths, name).await.is_err());
    let mut scheduler = TransferWitnessLeaderScheduler::new();
    scheduler
        .recv_region_info()
        .send(region(&serde_json::json!({})))
        .unwrap();
    for method in ["POST", "PUT", "DELETE", "GET"] {
        assert_response(serve(
            &scheduler,
            Request::builder()
                .method(method)
                .uri("/config")
                .body(Body::from(r#"{"disable":true}"#))
                .unwrap(),
        ))
        .await;
        assert!(scheduler.is_schedule_allowed());
    }
    let c = cluster(&serde_json::json!({})).await;
    let conf = ProviderSnapshot::<u64>::default();
    let mut rules = RuleManager::new(storage.clone(), paths.clone());
    let mut pick = |_| 0;
    let mut shuffle = |_: &mut [usize]| {};
    let mut ctx = EvictLeaderContext {
        cluster: &c,
        config: &conf,
        rules: &mut rules,
        now_unix_nanos: 1_000_000_000_000,
        random_index: &mut pick,
        shuffle: &mut shuffle,
    };
    let out = scheduler.schedule(&mut ctx).await;
    assert!(out.errors.is_empty());
    assert_eq!(out.operators.len(), 1);
    assert_eq!(out.operators[0].priority_level(), PriorityLevel::URGENT);
    let OpStep::TransferLeader(step) = out.operators[0].step(0).unwrap() else {
        panic!()
    };
    assert_eq!(step.to_stores, vec![2, 3]);
    assert!(scheduler.schedule(&mut ctx).await.operators.is_empty());
    backend
        .fail
        .store(false, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        storage.load_scheduler_config(&paths, name).await.unwrap(),
        b"null"
    );
    println!(
        "task152 persistence: real read/write I/O failure, 4 requests, unchanged storage and queued scheduling passed"
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
