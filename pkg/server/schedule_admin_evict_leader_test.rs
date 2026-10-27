// Copyright 2026 AsterSQL.
use crate::schedule_admin_evict_leader::serve;
use astersql_pd_core::{BasicCluster, StoreInfo, StoreMeta};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_schedulers::{EvictLeaderConfig, EvictLeaderScheduler, EvictRemoveError};
use astersql_pd_storage_endpoint::StorageEndpoint;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use serde_json::{Value, json};
use std::sync::Arc;
fn cluster() -> BasicCluster {
    let c = BasicCluster::new();
    for id in 0..=3 {
        let mut s = StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        });
        s.set_last_heartbeat_unix_nanos(1_000_000_000_000);
        c.stores.put_store(Arc::new(s));
    }
    c
}
fn scheduler() -> EvictLeaderScheduler {
    EvictLeaderScheduler::new(EvictLeaderConfig::default())
}
async fn call(
    s: &mut EvictLeaderScheduler,
    c: &BasicCluster,
    e: &StorageEndpoint,
    method: &str,
    path: &str,
    body: &str,
    remove: impl FnOnce(&str) -> Result<(), EvictRemoveError>,
) -> (u16, String, String) {
    let r = serve(
        s,
        c,
        e,
        &ClusterPaths::new(140),
        Request::builder()
            .method(method)
            .uri(path)
            .body(Body::from(body.to_owned()))
            .unwrap(),
        remove,
    )
    .await;
    let status = r.status().as_u16();
    let ct = r
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_owned())
        .unwrap_or_default();
    assert!(r.headers().get("allow").is_none());
    (
        status,
        String::from_utf8(to_bytes(r.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap(),
        ct,
    )
}
#[tokio::test]
async fn task_140_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_140.json"
    ))
    .unwrap();
    let c = cluster();
    let e = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let mut s = scheduler();
    for case in fixture["cases"].as_array().unwrap() {
        let before = s.encode_evict_config().unwrap();
        let (status, body, ct) = call(
            &mut s,
            &c,
            &e,
            case["method"].as_str().unwrap(),
            case["path"].as_str().unwrap(),
            case["input"].as_str().unwrap(),
            |name| {
                assert_eq!(name, "evict-leader-scheduler");
                Ok(())
            },
        )
        .await;
        assert_eq!(
            status,
            case["status"].as_u64().unwrap() as u16,
            "{}: {body}",
            case["name"]
        );
        if status >= 400 {
            assert_eq!(
                s.encode_evict_config().unwrap(),
                before,
                "{} changed state",
                case["name"]
            );
        }
        if let Some(expected) = case.get("json") {
            assert_eq!(
                serde_json::from_str::<Value>(&body).unwrap(),
                *expected,
                "{}",
                case["name"]
            );
            assert_eq!(ct, "application/json; charset=UTF-8");
        } else {
            assert_eq!(body, case["body"].as_str().unwrap());
            assert_eq!(
                ct,
                if status == 404 {
                    "text/plain; charset=utf-8"
                } else {
                    ""
                }
            );
        }
    }
    println!(
        "task140 HTTP cases={} passed",
        fixture["cases"].as_array().unwrap().len()
    );
}

use astersql_pd_storage_kv::{
    Commit, Compare, Lease, MemoryStore, MetaWatch, Mutation, RawTxnOp, RawTxnResponse, Read,
    Snapshot, StoreError,
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
async fn task_140_transactions_and_faults() {
    use astersql_pd_core_constant::Direction;
    use astersql_pd_schedule_core::BasicCluster as _;
    use std::sync::atomic::Ordering;
    let c = cluster();
    let backend = Arc::new(FaultStore::default());
    let e = StorageEndpoint::new(backend.clone());
    let paths = ClusterPaths::new(140);
    let mut s = scheduler();
    assert_eq!(
        call(
            &mut s,
            &c,
            &e,
            "POST",
            "/config",
            r#"{"store_id":1,"ranges":["a","z"]}"#,
            |_| Ok(())
        )
        .await
        .0,
        200
    );
    c.pause_leader_transfer(2, Direction::IN).unwrap();
    c.pause_leader_transfer(1, Direction::OUT).unwrap();
    let before = s.encode_evict_config().unwrap();
    for (fail, input) in [
        (true, r#"{"store_ids":[1,2],"batch":10,"ranges":["c","d"]}"#),
        (
            false,
            r#"{"store_ids":[1,2,999],"batch":10,"ranges":["c","d"]}"#,
        ),
    ] {
        backend.fail.store(fail, Ordering::SeqCst);
        let r = call(&mut s, &c, &e, "POST", "/config", input, |_| {
            panic!("update must not remove")
        })
        .await;
        assert_eq!(r.0, 400);
        assert!(!serde_json::from_str::<String>(&r.1).unwrap().is_empty());
        if !fail {
            assert_eq!(
                serde_json::from_str::<String>(&r.1).unwrap(),
                "[PD:core:ErrStoreNotFound]store 999 not found"
            )
        }
        assert_eq!(s.encode_evict_config().unwrap(), before);
        assert!(!c.get_store(2).unwrap().allow_leader_transfer_in());
        c.resume_leader_transfer(2, Direction::IN);
        assert!(c.get_store(2).unwrap().allow_leader_transfer_in());
        c.pause_leader_transfer(2, Direction::IN).unwrap();
        assert!(!c.get_store(1).unwrap().allow_leader_transfer_out());
        backend.fail.store(false, Ordering::SeqCst);
        assert_eq!(
            e.load_scheduler_config(&paths, "evict-leader-scheduler")
                .await
                .unwrap(),
            before
        );
    }
    backend.fail.store(true, Ordering::SeqCst);
    assert_eq!(
        call(&mut s, &c, &e, "DELETE", "/delete/1", "", |_| panic!(
            "save failed"
        ))
        .await
        .0,
        500
    );
    assert_eq!(s.encode_evict_config().unwrap(), before);
    assert!(!c.get_store(1).unwrap().allow_leader_transfer_in());
    backend.fail.store(false, Ordering::SeqCst);
    assert_eq!(
        call(&mut s, &c, &e, "DELETE", "/delete/1", "", |_| Err(
            EvictRemoveError::Failed("remove failed".into())
        ))
        .await
        .0,
        500
    );
    assert_eq!(s.encode_evict_config().unwrap(), before);
    assert!(!c.get_store(1).unwrap().allow_leader_transfer_in());
    let saved: Value = serde_json::from_slice(
        &e.load_scheduler_config(&paths, "evict-leader-scheduler")
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(saved["store-id-ranges"], json!({}));
    let r = call(&mut s, &c, &e, "DELETE", "/delete/1", "", |_| {
        Err(EvictRemoveError::NotFound)
    })
    .await;
    assert_eq!(r.0, 404);
    assert_eq!(
        serde_json::from_str::<Value>(&r.1).unwrap(),
        "[PD:scheduler:ErrSchedulerNotFound]scheduler not found"
    );
    assert!(s.config().stores().is_empty());
    assert!(c.get_store(1).unwrap().allow_leader_transfer_in());
    assert_eq!(
        call(
            &mut s,
            &c,
            &e,
            "POST",
            "/config",
            r#"{"store_id":1,"batch":100}"#,
            |_| Ok(())
        )
        .await
        .0,
        200
    );
    let mut restored = scheduler();
    restored.reload_evict_config(&c, &e, &paths).await.unwrap();
    assert_eq!(
        restored.encode_evict_config().unwrap(),
        s.encode_evict_config().unwrap()
    );
    println!(
        "task140 faults: 7 groups passed (save rollback, pause rollback, delete save, callback restore, NotFound, recovery, reload)"
    );
}
#[tokio::test]
async fn task_140_updated_scheduling_cases() {
    use astersql_pd_core::{KeyRange, Peer, RegionInfo, RegionMeta};
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_operator::{OpKind, OpStep};
    use astersql_pd_schedule_placement::RuleManager;
    use astersql_pd_schedule_schedulers::EvictLeaderContext;
    let mut c = cluster();
    let peers: Vec<_> = (1..=3)
        .map(|id| Peer {
            id,
            store_id: id,
            ..Default::default()
        })
        .collect();
    c.regions.check_and_put_region(Arc::new(RegionInfo::new(
        RegionMeta {
            id: 1,
            key_range: KeyRange {
                start_key: b"b".to_vec(),
                end_key: b"c".to_vec(),
            },
            peers: peers.clone(),
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![],
    )));
    let e = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let config = ProviderSnapshot::<()>::default();
    let paths = ClusterPaths::new(140);
    let mut rules = RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        paths.clone(),
    );
    rules
        .initialize(3, &[], "", false, &config, &[])
        .await
        .unwrap();
    for (request, expected) in [
        (r#"{"store_id":1,"ranges":["a","z"],"batch":1}"#, 1),
        (r#"{"store_id":1,"ranges":["d","z"]}"#, 0),
        (r#"{"store_id":1,"ranges":["b","c"],"batch":100}"#, 1),
        (r#"{"store_id":1,"ranges":["","b"]}"#, 0),
        (r#"{"store_id":1,"ranges":["",""]}"#, 1),
    ] {
        let mut active = scheduler();
        assert_eq!(
            call(&mut active, &c, &e, "POST", "/config", request, |_| Ok(()))
                .await
                .0,
            200
        );
        let encoded = active.encode_evict_config().unwrap();
        assert_eq!(
            e.load_scheduler_config(&paths, "evict-leader-scheduler")
                .await
                .unwrap(),
            encoded
        );
        let mut restored = scheduler();
        restored.reload_evict_config(&c, &e, &paths).await.unwrap();
        assert_eq!(restored.encode_evict_config().unwrap(), encoded);
        for s in [&active, &restored] {
            let mut random = |_: usize| 0;
            let mut shuffle = |_: &mut [usize]| {};
            let result = s
                .schedule(&mut EvictLeaderContext {
                    cluster: &c,
                    config: &config,
                    rules: &mut rules,
                    now_unix_nanos: 1_000_000_000_000,
                    random_index: &mut random,
                    shuffle: &mut shuffle,
                })
                .await;
            assert!(result.errors.is_empty());
            assert_eq!(result.operators.len(), expected, "{request}");
            for op in result.operators {
                assert_eq!(op.region_id(), 1);
                assert_eq!(op.kind(), OpKind::LEADER);
                assert_eq!(
                    op.priority_level(),
                    astersql_pd_core_constant::PriorityLevel::URGENT
                );
                assert_eq!(op.len(), 1);
                let OpStep::TransferLeader(step) = op.step(0).unwrap() else {
                    panic!("expected transfer")
                };
                assert_eq!(step.from_store, 1);
                assert_eq!(step.to_store, 2);
            }
        }
        active.clean(&c);
        restored.clean(&c);
    }
    println!("task140 scheduling: 5 HTTP update + reload cases; 10 real decisions passed");
}
#[tokio::test]
async fn task_140_body_read_and_clone() {
    let c = cluster();
    let e = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let paths = ClusterPaths::new(140);
    for input in [r#"{"store_id":1}"#, r#"{"store_id":"#] {
        let mut s = scheduler();
        let before = s.encode_evict_config().unwrap();
        let chunks = vec![
            Ok::<_, std::io::Error>(axum::body::Bytes::from(input.to_owned())),
            Err(std::io::Error::other("injected body read failure")),
        ];
        let r = serve(
            &mut s,
            &c,
            &e,
            &paths,
            Request::builder()
                .method("POST")
                .uri("/config")
                .body(Body::from_stream(futures_util::stream::iter(chunks)))
                .unwrap(),
            |_| panic!("body failure must not remove"),
        )
        .await;
        assert_eq!(r.status(), 500);
        assert_eq!(r.headers()["tidb-error-code"], "internal");
        let body: Value =
            serde_json::from_slice(&to_bytes(r.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert_eq!(body["code"], "internal");
        assert_eq!(body["msg"], "injected body read failure");
        assert_eq!(body["data"], json!({}));
        assert!(!body["stack"].as_array().unwrap().is_empty());
        assert_eq!(s.encode_evict_config().unwrap(), before);
        assert!(c.stores.get_store(1).unwrap().allow_leader_transfer_in());
        assert!(
            e.load_scheduler_config(&paths, "evict-leader-scheduler")
                .await
                .unwrap()
                .is_empty()
        );
    }
    let mut s = EvictLeaderScheduler::new(
        EvictLeaderConfig::new([(1, vec![])].into_iter().collect(), 3).unwrap(),
    );
    let before = s.encode_evict_config().unwrap();
    let r = call(&mut s, &c, &e, "GET", "/list", "", |_| Ok(())).await;
    assert_eq!(
        serde_json::from_str::<Value>(&r.1).unwrap(),
        json!({"store-id-ranges":{"1":null},"batch":3})
    );
    assert_eq!(s.encode_evict_config().unwrap(), before);
    println!(
        "task140 body/clone: 3 groups passed (complete read error, truncated read error, empty-slice clone)"
    );
}
