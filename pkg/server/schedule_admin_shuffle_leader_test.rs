// Copyright 2026 AsterSQL.
use crate::schedule_admin_shuffle_leader::serve;
use astersql_pd_schedule_schedulers::{ShuffleLeaderConfig, ShuffleLeaderScheduler};
use axum::{
    body::{Body, to_bytes},
    http::Request,
    response::Response,
};
use serde_json::Value;

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
async fn task_150_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_150.json"
    ))
    .unwrap();
    for config in [
        ShuffleLeaderConfig::default(),
        ShuffleLeaderConfig::new(vec![]),
        ShuffleLeaderConfig::from_args(&["a%00", "z+"]).unwrap(),
    ] {
        let scheduler = ShuffleLeaderScheduler::new(config);
        let before = scheduler.encode_config().unwrap();
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
            assert_eq!(scheduler.encode_config().unwrap(), before, "{case}");
        }
    }
    println!("task150 HTTP: 19 cases x 3 config states = 57 passed");
}

#[tokio::test]
async fn task_150_unread_body_faults() {
    let scheduler = ShuffleLeaderScheduler::new(ShuffleLeaderConfig::from_args(&[]).unwrap());
    let before = scheduler.encode_config().unwrap();
    for method in ["POST", "GET", "DELETE", "HEAD"] {
        // Polling a failing or stalled external stream would violate BaseScheduler.
        let stream = futures_util::stream::poll_fn(
            |_| -> std::task::Poll<Option<Result<axum::body::Bytes, std::io::Error>>> {
                panic!("unsupported shuffle_leader endpoint must not poll the request body")
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
        assert_eq!(scheduler.encode_config().unwrap(), before);
    }
    println!("task150 unread-body faults: 4 passed");
}

use astersql_pd_core::{
    BasicCluster, DownPeer, KeyRange, Peer, PeerRole, RegionInfo, RegionMeta, StoreInfo,
    StoreLabel, StoreMeta,
};
use astersql_pd_core_constant::{Direction, PriorityLevel};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::{ProviderSnapshot, REJECT_LEADER};
use astersql_pd_schedule_operator::{OpKind, OpStep};
use astersql_pd_schedule_placement::{
    LabelConstraint, LabelConstraintOp, PeerRoleType, Rule, RuleManager,
};
use astersql_pd_schedule_schedulers::EvictLeaderContext;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use std::sync::Arc;

#[tokio::test]
async fn task_150_schedule_after_unsupported_update() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_101.json"
    ))
    .unwrap();
    for case in f["cases"].as_array().unwrap() {
        for (v2, after) in [(false, false), (false, true), (true, false), (true, true)] {
            let mut c = BasicCluster::new();
            let label = StoreLabel {
                key: "noleader".into(),
                value: "true".into(),
            };
            for id in 1..=if case["no_stores"] == true { 0 } else { 3 } {
                let mut s = StoreInfo::new(StoreMeta {
                    id,
                    node_state: if id == 2 && case["offline"] == true {
                        astersql_pd_core::NodeState::Removing
                    } else {
                        Default::default()
                    },
                    labels: if id == 2 && case["special"] == true {
                        vec![StoreLabel {
                            key: "specialUse".into(),
                            value: "hotRegion".into(),
                        }]
                    } else if id == 1 || (id == 2 && case["reject_target"] == true) {
                        vec![label.clone()]
                    } else {
                        vec![]
                    },
                    ..Default::default()
                });
                s.set_last_heartbeat_unix_nanos(if id == 2 && case["disconnected"] == true {
                    1
                } else {
                    1_000_000_000_000
                });
                if id == 2 && case["limit"] == true {
                    let limit = astersql_pd_storelimit::StoreRateLimit::new(0.000000001);
                    assert!(
                        limit
                            .take(
                                1000,
                                astersql_pd_storelimit::Type::TRANSFER_LEADER_IN,
                                PriorityLevel::MEDIUM
                            )
                            .await
                    );
                    s.set_store_limit(Some(astersql_pd_core::StoreLimiter::V1(limit)));
                }
                c.stores.put_store(Arc::new(s));
            }
            let peers: Vec<_> = (1..=3)
                .map(|id| Peer {
                    id,
                    store_id: id,
                    role: if id == 2 && case["learner"] == true {
                        PeerRole::Learner
                    } else {
                        PeerRole::Voter
                    },
                    is_witness: id == 2 && case["witness"] == true,
                    ..Default::default()
                })
                .collect();
            let ids = |s: &str| -> Vec<u64> {
                case[s]
                    .as_array()
                    .map(|a| a.iter().map(|v| v.as_u64().unwrap()).collect())
                    .unwrap_or_default()
            };
            let r = Arc::new(RegionInfo::new(
                RegionMeta {
                    id: 1,
                    key_range: KeyRange {
                        start_key: b"a".to_vec(),
                        end_key: b"b".to_vec(),
                    },
                    peers: peers.clone(),
                    ..Default::default()
                },
                Some(peers[0].clone()),
                ids("down")
                    .iter()
                    .map(|i| DownPeer {
                        peer: peers[*i as usize - 1].clone(),
                        down_seconds: 100,
                    })
                    .collect(),
                ids("pending")
                    .iter()
                    .map(|i| peers[*i as usize - 1].clone())
                    .collect(),
            ));
            if case["missing"] != true {
                let (o, v, b) = c.regions.set_region(r.clone());
                c.regions.update_sub_tree(r.clone(), o.as_deref(), &v, b);
            }
            if case["select_source"] != true {
                c.stores.pause_leader_transfer(1, Direction::IN).unwrap();
            }
            for id in ids("pause") {
                c.stores.pause_leader_transfer(id, Direction::IN).unwrap();
            }
            if case["pause_out"] == true {
                c.stores.pause_leader_transfer(1, Direction::OUT).unwrap();
            }
            let mut config = ProviderSnapshot::<()>::default();
            if case["reject"] != false {
                config
                    .label_properties
                    .insert(REJECT_LEADER.into(), vec![label]);
            }
            let mut rules = RuleManager::new(
                Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
                ClusterPaths::new(150),
            );
            rules
                .initialize(3, &[], "", false, &config, &[])
                .await
                .unwrap();
            if case["placement"] == true || case["placement_allow"] == true {
                config.replication.enable_placement_rules = true;
                let rule = Rule {
                    group_id: "pd".into(),
                    id: "default".into(),
                    role: PeerRoleType::Voter,
                    count: 3,
                    label_constraints: vec![LabelConstraint {
                        key: "noleader".into(),
                        op: if case["placement"] == true {
                            LabelConstraintOp::In
                        } else {
                            LabelConstraintOp::NotIn
                        },
                        values: vec!["true".into()],
                    }],
                    ..Default::default()
                };
                rules
                    .set_rule(
                        rule,
                        0,
                        &config,
                        &c.stores
                            .get_stores()
                            .iter()
                            .map(|s| s.as_ref().clone())
                            .collect::<Vec<_>>(),
                    )
                    .await
                    .unwrap();
            }
            let ranges = case["ranges"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|p| KeyRange {
                            start_key: p[0].as_str().unwrap().as_bytes().to_vec(),
                            end_key: p[1].as_str().unwrap().as_bytes().to_vec(),
                        })
                        .collect()
                })
                .unwrap_or_else(|| vec![KeyRange::default()]);
            let scheduler = ShuffleLeaderScheduler::new(ShuffleLeaderConfig::new(ranges));
            let before = scheduler.encode_config().unwrap();
            let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
            let paths = ClusterPaths::new(150);
            storage
                .save_scheduler_config(&paths, "shuffle-leader-scheduler", &before)
                .await
                .unwrap();
            if after {
                assert_response(serve(
                    &scheduler,
                    Request::builder()
                        .method("POST")
                        .uri("/config")
                        .body(Body::from(if case["target"].as_u64().unwrap() == 0 {
                            r#"{"ranges":[]}"#
                        } else {
                            r#"{"ranges":[{"start-key":"78","end-key":"7a"}]}"#
                        }))
                        .unwrap(),
                ))
                .await;
            }
            assert_eq!(scheduler.encode_config().unwrap(), before);
            assert_eq!(
                storage
                    .load_scheduler_config(&paths, "shuffle-leader-scheduler")
                    .await
                    .unwrap(),
                before
            );
            let mut index = |n: usize| if case["last"] == true { n - 1 } else { 0 };
            let result = scheduler
                .schedule(&mut EvictLeaderContext {
                    cluster: &c,
                    config: &config,
                    rules: &mut rules,
                    now_unix_nanos: 1_000_000_000_000,
                    random_index: &mut index,
                    shuffle: &mut |_| {},
                })
                .await;
            let target = case["target"].as_u64().unwrap();
            assert_eq!(
                result.operators.len(),
                usize::from(target != 0),
                "{case}: {:?}",
                result.errors
            );
            assert_eq!(!result.errors.is_empty(), case["error"] == true, "{case}");
            if case["error"] == true {
                assert!(
                    matches!(
                        result.errors[0],
                        astersql_pd_schedule_operator::BuilderError::LeaderNotAllowed
                    ),
                    "{case}: {:?}",
                    result.errors
                );
            }
            for op in result.operators {
                assert_eq!(op.desc(), "shuffle-leader-scheduler");
                assert_eq!(op.kind(), OpKind::LEADER | OpKind::ADMIN);
                assert_eq!(op.priority_level(), PriorityLevel::LOW);
                assert_eq!(op.len(), 1);
                let OpStep::TransferLeader(step) = op.step(0).unwrap() else {
                    panic!("expected transfer")
                };
                assert_eq!((step.from_store, step.to_store), (1, target));
                assert!(step.to_stores.is_empty());
                assert_eq!(step.conf_ver_changed(&r), 0);
                let influence = astersql_pd_schedule_operator::OpInfluence::new();
                step.influence(&influence, &r);
                assert_eq!(influence.get_store_influence(1).leader_count, -1);
                assert_eq!(influence.get_store_influence(target).leader_count, 1);
                let cmd = step.get_cmd(&r, v2).transfer_leader.unwrap();
                assert_eq!(cmd.peer.unwrap().store_id, target);
                assert!(cmd.peers.is_empty());
                assert!(op.start());
                assert!(op.check(&r).is_some());
                let applied = r.clone_with([astersql_pd_core::RegionOption::Leader(Some(
                    peers[target as usize - 1].clone(),
                ))]);
                assert!(op.check(&applied).is_none());
                assert!(op.check_success());
            }
        }
    }
    println!(
        "task150: {} source-derived cases x before/after x 2 heartbeat modes = 96 decisions passed",
        f["cases"].as_array().unwrap().len()
    );
}

#[tokio::test]
async fn task_150_persistence_faults() {
    let backend = Arc::new(FaultStore::default());
    let storage = StorageEndpoint::new(backend.clone());
    let paths = ClusterPaths::new(150);
    let s = ShuffleLeaderScheduler::new(ShuffleLeaderConfig::from_args(&["a", "b"]).unwrap());
    let before = s.encode_config().unwrap();
    storage
        .save_scheduler_config(&paths, "shuffle-leader-scheduler", &before)
        .await
        .unwrap();
    backend
        .fail
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(
        storage
            .save_scheduler_config(&paths, "shuffle-leader-scheduler", b"{}")
            .await
            .is_err()
    );
    for (count, limit, expected) in [
        (0, 0, false),
        (2, 3, true),
        (3, 3, false),
        (4, 3, false),
        (u64::MAX, u64::MAX, false),
    ] {
        assert_eq!(
            ShuffleLeaderScheduler::is_schedule_allowed(count, limit),
            expected
        );
    }
    for method in ["POST", "PUT", "DELETE", "GET"] {
        assert_response(serve(
            &s,
            Request::builder()
                .method(method)
                .uri("/config")
                .body(Body::from(r#"{"ranges":[]}"#))
                .unwrap(),
        ))
        .await;
        assert_eq!(s.encode_config().unwrap(), before);
    }
    backend
        .fail
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let restored = storage
        .load_scheduler_config(&paths, "shuffle-leader-scheduler")
        .await
        .unwrap();
    assert_eq!(restored, before);
    let restored = ShuffleLeaderScheduler::new(serde_json::from_slice(&restored).unwrap());
    assert_eq!(restored.encode_config().unwrap(), before);
    println!(
        "task150 persistence: actual I/O failure, 4 unsupported requests, restored unchanged; 5 admission boundaries passed"
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
