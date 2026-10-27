// Copyright 2026 AsterSQL.
use crate::schedule_admin_grant_hot_region::serve;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_schedulers::{GrantHotConfig, GrantHotScheduler};
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use serde_json::Value;
use std::sync::Arc;
async fn call(
    s: &mut GrantHotScheduler,
    e: &StorageEndpoint,
    method: &str,
    path: &str,
    body: &str,
) -> (u16, String, String) {
    let r = serve(
        s,
        e,
        &ClusterPaths::new(143),
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
async fn task_143_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_143.json"
    ))
    .unwrap();
    let e = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let mut scheduler = GrantHotScheduler::new(GrantHotConfig::default());
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
        "task143 HTTP cases={} passed",
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
async fn task_143_faults_and_body_errors() {
    use std::sync::atomic::Ordering;
    let backend = Arc::new(FaultStore::default());
    let e = StorageEndpoint::new(backend.clone());
    let paths = ClusterPaths::new(143);
    let mut s = GrantHotScheduler::new(GrantHotConfig::default());
    let first = r#"{"store-id":"1,2","store-leader-id":"2"}"#;
    let next = r#"{"store-id":"3,4","store-leader-id":"5"}"#;
    assert_eq!(call(&mut s, &e, "POST", "/config", first).await.0, 200);
    let saved = e
        .load_scheduler_config(&paths, "grant-hot-region-scheduler")
        .await
        .unwrap();
    backend.fail.store(true, Ordering::SeqCst);
    let (status, body, _) = call(&mut s, &e, "POST", "/config", next).await;
    assert_eq!(status, 500);
    assert_eq!(
        serde_json::from_str::<String>(&body).unwrap(),
        "metadata backend unavailable; mutation outcome may be unknown"
    );
    assert_eq!(s.config().store_ids, vec![3, 4, 5]);
    assert_eq!(s.config().leader_id, 0);
    let (_, listed, _) = call(&mut s, &e, "GET", "/list", "").await;
    assert_eq!(
        serde_json::from_str::<Value>(&listed).unwrap(),
        serde_json::json!({"store-id":[3,4,5],"store-leader-id":0})
    );
    assert!(s.reload(&e, &paths).await.is_err());
    assert_eq!(s.config().leader_id, 0);
    backend.fail.store(false, Ordering::SeqCst);
    assert_eq!(
        e.load_scheduler_config(&paths, "grant-hot-region-scheduler")
            .await
            .unwrap(),
        saved
    );
    s.reload(&e, &paths).await.unwrap();
    assert_eq!(
        s.config(),
        &GrantHotConfig {
            store_ids: vec![1, 2],
            leader_id: 2
        }
    );
    assert_eq!(call(&mut s, &e, "POST", "/config", next).await.0, 200);
    let mut restored = GrantHotScheduler::new(GrantHotConfig::default());
    restored.reload(&e, &paths).await.unwrap();
    assert_eq!(restored.config(), s.config());
    for input in ["", "{", "[]", "true", "12", "\"bad\""] {
        let before = s.config().clone();
        let r = serve(
            &mut s,
            &e,
            &paths,
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
        assert_eq!(s.config(), &before);
    }
    for prefix in [first, r#"{"store-id":"#] {
        let stream = futures_util::stream::iter(vec![
            Ok::<_, std::io::Error>(axum::body::Bytes::from(prefix)),
            Err(std::io::Error::other("task143 read fault")),
        ]);
        let before = s.config().clone();
        let r = serve(
            &mut s,
            &e,
            &paths,
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
        assert_eq!(v["msg"], "task143 read fault");
        assert!(!v["stack"].as_array().unwrap().is_empty());
        assert_eq!(s.config(), &before);
    }
    println!("task143 fault/body groups=10 passed");
}

#[tokio::test]
async fn task_143_updated_scheduling() {
    use astersql_pd_core::{BasicCluster, Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta};
    use astersql_pd_core_constant::{Direction, PriorityLevel};
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_operator::{BuilderError, OpInfluence, OpKind, OpStep};
    use astersql_pd_schedule_placement::RuleManager;
    use astersql_pd_schedule_schedulers::{GrantHotContext, GrantHotError};
    use astersql_pd_statistics::{HotPeerStat, StoreLoadDetail, StoreSummaryInfo};
    use std::collections::HashSet;
    for v2 in [false, true] {
        let mut c = BasicCluster::new();
        for id in 1..=5 {
            c.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
                id,
                last_heartbeat: 100,
                ..Default::default()
            })));
        }
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
                ..Default::default()
            },
            Some(peers[0].clone()),
            vec![],
            vec![],
        ));
        let (o, v, b) = c.regions.set_region(region.clone());
        c.regions
            .update_sub_tree(region.clone(), o.as_deref(), &v, b);
        let e = StorageEndpoint::new(Arc::new(MemoryStore::default()));
        let paths = ClusterPaths::new(143);
        let config = ProviderSnapshot::<()>::default();
        let mut rules = RuleManager::new(
            Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
            paths.clone(),
        );
        rules
            .initialize(3, &[], "", false, &config, &[])
            .await
            .unwrap();
        let active = HashSet::new();
        let mut allocate = || Ok::<_, BuilderError>(100);
        let mut ctx = GrantHotContext {
            cluster: &c,
            config: &config,
            rules: &mut rules,
            now: 100,
            supports_conf_change_v2: v2,
            allocate: &mut allocate,
            active_regions: &active,
        };
        let mut s = GrantHotScheduler::new(GrantHotConfig::default());
        for (stores, leader, source, branch, target) in [
            ("2,3", 2, 1, true, 2),
            ("2,3", 3, 1, true, 3),
            ("2,4", 4, 2, false, 4),
            ("2,5", 5, 2, false, 5),
        ] {
            assert_eq!(
                call(
                    &mut s,
                    &e,
                    "POST",
                    "/config",
                    &format!(r#"{{"store-id":"{stores}","store-leader-id":"{leader}"}}"#)
                )
                .await
                .0,
                200
            );
            let mut restored = GrantHotScheduler::new(GrantHotConfig::default());
            restored.reload(&e, &paths).await.unwrap();
            assert_eq!(restored.config(), s.config());
            let detail = StoreLoadDetail {
                store_summary_info: StoreSummaryInfo {
                    store_info: c.stores.get_store(source).unwrap().as_ref().clone(),
                    ..Default::default()
                },
                hot_peers: vec![HotPeerStat {
                    region_id: 1,
                    store_id: source,
                    ..Default::default()
                }],
                load_pred: Default::default(),
            };
            let out = restored.dispatch(&[detail], branch, 0, &mut ctx).await;
            assert_eq!(out.operators.len(), 1, "{:?}", out.errors);
            assert!(out.errors.is_empty());
            let op = &out.operators[0];
            assert_eq!(op.region_id(), 1);
            assert_eq!(op.priority_level(), PriorityLevel::HIGH);
            assert_eq!(op.conf_ver_changed(&region), 0);
            if branch {
                assert_eq!(op.kind(), OpKind::LEADER);
                assert_eq!(op.len(), 1);
                let OpStep::TransferLeader(step) = op.step(0).unwrap() else {
                    panic!()
                };
                assert_eq!((step.from_store, step.to_store), (1, target));
                assert_eq!(
                    step.get_cmd(&region, v2)
                        .transfer_leader
                        .unwrap()
                        .peer
                        .unwrap()
                        .store_id,
                    target
                );
                c.stores
                    .pause_leader_transfer(target, Direction::IN)
                    .unwrap();
                assert!(matches!(
                    restored.transfer(1, source, true, 0, &mut ctx).await,
                    Err(GrantHotError::NoDestination)
                ));
                c.stores.resume_leader_transfer(target, Direction::IN);
            } else {
                assert_eq!(
                    op.kind().0 & (OpKind::REGION | OpKind::LEADER).0,
                    (OpKind::REGION | OpKind::LEADER).0
                );
                let steps: Vec<_> = (0..op.len()).map(|i| op.step(i).unwrap()).collect();
                assert!(
                    steps
                        .iter()
                        .any(|s| matches!(s,OpStep::RemovePeer(r) if r.from_store==2))
                );
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
                assert_eq!((add.to_store, add.peer_id), (target, 100));
                let cmd = add.get_cmd(&region, v2).unwrap();
                let change = if v2 {
                    cmd.change_peer_v2.unwrap().changes[0].clone().unwrap()
                } else {
                    cmd.change_peer.unwrap()
                };
                assert_eq!(change.peer.unwrap().store_id, target);
                let influence = OpInfluence::new();
                op.total_influence(&influence, Some(&region));
                assert_eq!(influence.get_store_influence(2).region_count, -1);
                assert_eq!(influence.get_store_influence(target).region_count, 1);
            }
        }
    }
    println!("task143 scheduling groups=8 passed (4 HTTP updates x 2 protocol modes)");
}
