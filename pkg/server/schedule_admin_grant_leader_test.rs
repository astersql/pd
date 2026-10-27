// Copyright 2026 AsterSQL.
use crate::schedule_admin_grant_leader::serve;
use astersql_pd_core::{BasicCluster, StoreInfo, StoreMeta};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_schedulers::{GrantLeaderConfig, GrantLeaderScheduler, GrantRemoveError};
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
fn scheduler() -> GrantLeaderScheduler {
    GrantLeaderScheduler::new(GrantLeaderConfig::default())
}
async fn call(
    s: &mut GrantLeaderScheduler,
    c: &BasicCluster,
    e: &StorageEndpoint,
    method: &str,
    path: &str,
    body: &str,
    remove: impl FnOnce(&str) -> Result<(), GrantRemoveError>,
) -> (u16, String, String) {
    let r = serve(
        s,
        c,
        e,
        &ClusterPaths::new(145),
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
async fn task_145_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_145.json"
    ))
    .unwrap();
    let c = cluster();
    let e = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let mut s = scheduler();
    for case in fixture["cases"].as_array().unwrap() {
        let before = s.encode_grant_config().unwrap();
        let (status, body, ct) = call(
            &mut s,
            &c,
            &e,
            case["method"].as_str().unwrap(),
            case["path"].as_str().unwrap(),
            case["input"].as_str().unwrap(),
            |name| {
                assert_eq!(name, "grant-leader-scheduler");
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
                s.encode_grant_config().unwrap(),
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
        "task145 HTTP cases={} passed",
        fixture["cases"].as_array().unwrap().len()
    );
}

#[tokio::test]
async fn task_145_transactions_and_faults() {
    use std::sync::atomic::Ordering;
    let c = cluster();
    let backend = Arc::new(FaultStore::default());
    let e = StorageEndpoint::new(backend.clone());
    let paths = ClusterPaths::new(145);
    let mut s = scheduler();
    assert_eq!(
        call(
            &mut s,
            &c,
            &e,
            "POST",
            "/config",
            r#"{"store_id":1}"#,
            |_| Ok(())
        )
        .await
        .0,
        200
    );
    let saved = s.encode_grant_config().unwrap();
    backend.fail.store(true, Ordering::SeqCst);
    // Go removes an existing entry on update-save failure, but keeps old disk state.
    let r = call(
        &mut s,
        &c,
        &e,
        "POST",
        "/config",
        r#"{"store_id":1}"#,
        |_| panic!("update"),
    )
    .await;
    assert_eq!(r.0, 500);
    assert_eq!(
        serde_json::from_str::<String>(&r.1).unwrap(),
        "metadata backend unavailable; mutation outcome may be unknown"
    );
    assert!(s.config().stores().is_empty());
    assert!(c.stores.get_store(1).unwrap().allow_leader_transfer_out());
    assert_eq!(
        call(
            &mut s,
            &c,
            &e,
            "POST",
            "/config",
            r#"{"store_id":2}"#,
            |_| Ok(())
        )
        .await
        .0,
        500
    );
    assert!(s.config().stores().is_empty());
    assert!(c.stores.get_store(2).unwrap().allow_leader_transfer_out());
    backend.fail.store(false, Ordering::SeqCst);
    assert_eq!(
        e.load_scheduler_config(&paths, "grant-leader-scheduler")
            .await
            .unwrap(),
        saved
    );
    s.reload_grant_config(&c, &e, &paths).await.unwrap();
    assert_eq!(s.encode_grant_config().unwrap(), saved);
    assert!(!c.stores.get_store(1).unwrap().allow_leader_transfer_out());
    backend.fail.store(true, Ordering::SeqCst);
    assert!(s.reload_grant_config(&c, &e, &paths).await.is_err());
    assert_eq!(s.encode_grant_config().unwrap(), saved);
    assert_eq!(
        call(&mut s, &c, &e, "DELETE", "/delete/1", "", |_| panic!(
            "save failed"
        ))
        .await
        .0,
        500
    );
    assert_eq!(s.encode_grant_config().unwrap(), saved);
    assert!(!c.stores.get_store(1).unwrap().allow_leader_transfer_out());
    backend.fail.store(false, Ordering::SeqCst);
    let r = call(&mut s, &c, &e, "DELETE", "/delete/1", "", |name| {
        assert_eq!(name, "grant-leader-scheduler");
        Err(GrantRemoveError::Failed("remove failed".into()))
    })
    .await;
    assert_eq!(r.0, 500);
    assert_eq!(
        serde_json::from_str::<Value>(&r.1).unwrap(),
        "remove failed"
    );
    assert_eq!(s.encode_grant_config().unwrap(), saved);
    assert!(!c.stores.get_store(1).unwrap().allow_leader_transfer_out());
    let disk: Value = serde_json::from_slice(
        &e.load_scheduler_config(&paths, "grant-leader-scheduler")
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(disk, json!({"store-id-ranges":{}}));
    let r = call(&mut s, &c, &e, "DELETE", "/delete/1", "", |_| {
        Err(GrantRemoveError::NotFound)
    })
    .await;
    assert_eq!(r.0, 404);
    assert_eq!(
        serde_json::from_str::<Value>(&r.1).unwrap(),
        "[PD:scheduler:ErrSchedulerNotFound]scheduler not found"
    );
    assert!(s.config().stores().is_empty());
    assert!(c.stores.get_store(1).unwrap().allow_leader_transfer_out());
    // Retained ranges are QueryUnescaped a second time; errors release OUT pause.
    for start in ["%25xx", "é%25xx"] {
        let mut s =
            GrantLeaderScheduler::new(GrantLeaderConfig::from_args(&["1", start, "z"]).unwrap());
        s.prepare(&c).unwrap();
        let before = s.encode_grant_config().unwrap();
        let r = call(
            &mut s,
            &c,
            &e,
            "POST",
            "/config",
            r#"{"store_id":1,"ranges":["a","z"]}"#,
            |_| Ok(()),
        )
        .await;
        assert_eq!(r.0, 400);
        assert_eq!(
            serde_json::from_str::<Value>(&r.1).unwrap(),
            "[PD:url:ErrQueryUnescape]inverse transformation of QueryEscape error: invalid URL escape \"%xx\""
        );
        assert_eq!(s.encode_grant_config().unwrap(), before);
        assert!(c.stores.get_store(1).unwrap().allow_leader_transfer_out());
    }
    println!(
        "task145 faults: 8 groups passed: existing/new save, reload success/failure, delete save, callback restore, NotFound, range failure"
    );
}
#[tokio::test]
async fn task_145_updated_scheduling_cases() {
    use astersql_pd_core::{KeyRange, Peer, RegionInfo, RegionMeta};
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_operator::{OpKind, OpStep};
    use astersql_pd_schedule_placement::RuleManager;
    use astersql_pd_schedule_schedulers::GrantLeaderContext;
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
    let paths = ClusterPaths::new(145);
    let mut rules = RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        paths.clone(),
    );
    rules
        .initialize(3, &[], "", false, &config, &[])
        .await
        .unwrap();
    for (request, expected) in [
        (r#"{"store_id":2,"ranges":["d","z"]}"#, 1),
        (r#"{"store_id":1}"#, 0),
        (r#"{"store_id":3}"#, 1),
    ] {
        let mut active = scheduler();
        assert_eq!(
            call(&mut active, &c, &e, "POST", "/config", request, |_| Ok(()))
                .await
                .0,
            200
        );
        let encoded = active.encode_grant_config().unwrap();
        assert_eq!(
            e.load_scheduler_config(&paths, "grant-leader-scheduler")
                .await
                .unwrap(),
            encoded
        );
        let mut restored = scheduler();
        restored.reload_grant_config(&c, &e, &paths).await.unwrap();
        assert_eq!(restored.encode_grant_config().unwrap(), encoded);
        for s in [&active, &restored] {
            let mut random = |_: usize| 0;
            let mut shuffle = |_: &mut [usize]| {};
            let result = s
                .schedule(&mut GrantLeaderContext {
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
                    astersql_pd_core_constant::PriorityLevel::HIGH
                );
                assert_eq!(op.len(), 1);
                let OpStep::TransferLeader(step) = op.step(0).unwrap() else {
                    panic!("expected transfer")
                };
                assert_eq!(step.from_store, 1);
                assert_eq!(
                    step.to_store,
                    serde_json::from_str::<Value>(request).unwrap()["store_id"]
                        .as_u64()
                        .unwrap()
                );
            }
        }
        active.clean(&c);
        restored.clean(&c);
    }
    println!("task145 scheduling: 3 HTTP update + reload cases; 6 real decisions passed");
}
#[tokio::test]
async fn task_145_body_read_and_clone() {
    let c = cluster();
    let e = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let paths = ClusterPaths::new(145);
    for input in [r#"{"store_id":1}"#, r#"{"store_id":"#] {
        let mut s = scheduler();
        let before = s.encode_grant_config().unwrap();
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
        assert_eq!(s.encode_grant_config().unwrap(), before);
        assert!(c.stores.get_store(1).unwrap().allow_leader_transfer_out());
        assert!(
            e.load_scheduler_config(&paths, "grant-leader-scheduler")
                .await
                .unwrap()
                .is_empty()
        );
    }
    let mut s =
        GrantLeaderScheduler::new(GrantLeaderConfig::new([(1, vec![])].into_iter().collect()));
    let before = s.encode_grant_config().unwrap();
    let r = call(&mut s, &c, &e, "GET", "/list", "", |_| Ok(())).await;
    assert_eq!(
        serde_json::from_str::<Value>(&r.1).unwrap(),
        json!({"store-id-ranges":{"1":null}})
    );
    assert_eq!(s.encode_grant_config().unwrap(), before);
    println!(
        "task145 body/clone: 3 groups passed (complete read error, truncated read error, empty-slice clone)"
    );
}
