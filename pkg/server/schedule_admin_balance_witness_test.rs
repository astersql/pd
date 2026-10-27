// Copyright 2026 AsterSQL.
use crate::schedule_admin_balance_witness::serve;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_schedulers::BalanceWitnessScheduler;
use astersql_pd_storage_endpoint::StorageEndpoint;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use serde_json::Value;
use std::sync::Arc;

async fn call(
    s: &mut BalanceWitnessScheduler,
    e: &StorageEndpoint,
    method: &str,
    path: &str,
    body: &str,
) -> (u16, String, String) {
    let response = serve(
        s,
        e,
        &ClusterPaths::new(139),
        Request::builder()
            .method(method)
            .uri(path)
            .body(Body::from(body.to_owned()))
            .unwrap(),
    )
    .await;
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_owned())
        .unwrap_or_default();
    (
        status,
        String::from_utf8(
            to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap(),
        content_type,
    )
}
#[tokio::test]
async fn task_139_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_139.json"
    ))
    .unwrap();
    let endpoint = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let mut scheduler = BalanceWitnessScheduler::default();
    for case in fixture["cases"].as_array().unwrap() {
        let (status, body, content_type) = call(
            &mut scheduler,
            &endpoint,
            case["method"].as_str().unwrap(),
            case["path"].as_str().unwrap(),
            case["input"].as_str().unwrap(),
        )
        .await;
        assert_eq!(
            status,
            case["status"].as_u64().unwrap() as u16,
            "{}: {body}",
            case["name"]
        );
        if let Some(expected) = case.get("json") {
            assert_eq!(
                serde_json::from_str::<Value>(&body).unwrap(),
                *expected,
                "{}",
                case["name"]
            );
            assert_eq!(content_type, "application/json; charset=UTF-8");
        } else {
            assert_eq!(body, case["body"].as_str().unwrap());
            assert_eq!(
                content_type,
                if status == 404 {
                    "text/plain; charset=utf-8"
                } else {
                    ""
                }
            );
        }
        assert_eq!(
            scheduler.config.batch(),
            case["batch"].as_u64().unwrap() as usize
        );
    }
    println!(
        "task139 HTTP cases={} passed",
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
async fn task_139_persistence_failure() {
    use std::sync::atomic::Ordering;
    let backend = Arc::new(FaultStore::default());
    let endpoint = StorageEndpoint::new(backend.clone());
    let paths = ClusterPaths::new(139);
    let mut scheduler = BalanceWitnessScheduler::default();
    let old = scheduler.encode_witness_config().unwrap();
    endpoint
        .save_scheduler_config(&paths, "balance-witness-scheduler", &old)
        .await
        .unwrap();
    backend.fail.store(true, Ordering::SeqCst);
    let (status, body, _) = call(
        &mut scheduler,
        &endpoint,
        "POST",
        "/config",
        r#"{"batch":1}"#,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        "Config is updated."
    );
    assert_eq!(scheduler.config.batch(), 1);
    let (status, body, _) = call(
        &mut scheduler,
        &endpoint,
        "POST",
        "/config",
        r#"{"batch":1}"#,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        "Config is the same with origin, so do nothing."
    );
    let (status, body, _) = call(&mut scheduler, &endpoint, "GET", "/list", "").await;
    assert_eq!(status, 200);
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap()["batch"], 1);
    backend.fail.store(false, Ordering::SeqCst);
    assert_eq!(
        endpoint
            .load_scheduler_config(&paths, "balance-witness-scheduler")
            .await
            .unwrap(),
        old
    );
    scheduler
        .reload_witness_config(&endpoint, &paths)
        .await
        .unwrap();
    assert_eq!(scheduler.config.batch(), 4);
    let (status, _, _) = call(
        &mut scheduler,
        &endpoint,
        "POST",
        "/config",
        r#"{"batch":10}"#,
    )
    .await;
    assert_eq!(status, 200);
    scheduler.config = astersql_pd_schedule_schedulers::BalanceWitnessConfig::default();
    scheduler
        .reload_witness_config(&endpoint, &paths)
        .await
        .unwrap();
    assert_eq!(scheduler.config.batch(), 10);
    println!("task139 fault cases=4 passed (publish, unchanged, old reload, recovered save)");
}

#[tokio::test]
async fn task_139_body_read_failure() {
    let endpoint = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let mut scheduler = BalanceWitnessScheduler::default();
    for (partial, expected) in [(r#"{"batch":2}"#, 200), (r#"{"batch":"#, 500)] {
        let chunks = futures_util::stream::iter(vec![
            Ok(axum::body::Bytes::from(partial)),
            Err(std::io::Error::other("injected read error")),
        ]);
        let response = serve(
            &mut scheduler,
            &endpoint,
            &ClusterPaths::new(139),
            Request::builder()
                .method("POST")
                .uri("/config")
                .body(Body::from_stream(chunks))
                .unwrap(),
        )
        .await;
        assert_eq!(response.status(), expected);
        assert_eq!(scheduler.config.batch(), 2);
    }
    println!("task139 body I/O cases=2 passed");
}

use astersql_pd_core::{
    BasicCluster as CoreCluster, KeyRange, Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta,
};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_operator::{OpInfluence, OpKind, OpStep};
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_schedule_schedulers::{BalanceWitnessConfig, WitnessBalanceContext};
#[tokio::test]
async fn task_139_updated_scheduling_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_139.json"
    ))
    .unwrap();
    for case in fixture["decision_cases"].as_array().unwrap() {
        let mut cluster = CoreCluster::new();
        for (id, count) in [(1, 7), (2, 8), (3, 9), (4, 14)] {
            let mut s = StoreInfo::new(StoreMeta {
                id,
                ..Default::default()
            });
            s.set_witness_count(count);
            s.set_last_heartbeat_unix_nanos(1_000_000_000_000);
            cluster.stores.put_store(Arc::new(s));
        }
        let peers: Vec<_> = (1..=4)
            .map(|id| Peer {
                id: id + 10,
                store_id: id,
                is_witness: id == 4,
                ..Default::default()
            })
            .collect();
        let region = Arc::new(RegionInfo::new(
            RegionMeta {
                id: 1,
                key_range: KeyRange {
                    start_key: b"a".to_vec(),
                    end_key: b"b".to_vec(),
                },
                peers: peers.clone(),
                ..Default::default()
            },
            Some(peers[2].clone()),
            vec![],
            vec![],
        ));
        let (o, v, b) = cluster.regions.set_region(region.clone());
        cluster.regions.update_sub_tree(region, o.as_deref(), &v, b);
        let mut config = ProviderSnapshot::<()>::default();
        config.schedule.tolerant_size_ratio = 2.5;
        config.replication.max_replicas = 4;
        config.replication.enable_placement_rules = false;
        let endpoint = StorageEndpoint::new(Arc::new(MemoryStore::default()));
        let paths = ClusterPaths::new(139);
        let mut rules = RuleManager::new(
            Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
            paths.clone(),
        );
        rules
            .initialize(4, &[], "", false, &config, &[])
            .await
            .unwrap();
        let ranges: Vec<serde_json::Value> = case["ranges"].as_array().unwrap().iter().map(|r| {
            let hex = |s: &str| s.as_bytes().iter().map(|b| format!("{b:02x}")).collect::<String>();
            serde_json::json!({"start-key":hex(r[0].as_str().unwrap()),"end-key":hex(r[1].as_str().unwrap())})
        }).collect();
        let request = serde_json::json!({"batch":case["batch"],"ranges":ranges}).to_string();
        let mut active = BalanceWitnessScheduler::default();
        assert_eq!(
            call(&mut active, &endpoint, "POST", "/config", &request)
                .await
                .0,
            200
        );
        let encoded = active.encode_witness_config().unwrap();
        let json: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(json["batch"], case["batch"]);
        assert_eq!(
            json["ranges"].as_array().unwrap().len(),
            case["ranges"].as_array().unwrap().len()
        );
        let decoded: BalanceWitnessConfig = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded.ranges(), active.config.ranges());
        assert_eq!(
            call(&mut active, &endpoint, "POST", "/config", &request)
                .await
                .1,
            "\"Config is the same with origin, so do nothing.\""
        );
        assert_eq!(
            endpoint
                .load_scheduler_config(&paths, "balance-witness-scheduler")
                .await
                .unwrap(),
            encoded
        );
        let mut restored = BalanceWitnessScheduler::default();
        restored
            .reload_witness_config(&endpoint, &paths)
            .await
            .unwrap();
        assert_eq!(restored.encode_witness_config().unwrap(), encoded);
        let influence = OpInfluence::new();
        let mut random = |_: usize| 0;
        let mut shuffle = |_: &mut [usize]| {};
        let mut ctx = WitnessBalanceContext {
            cluster: &cluster,
            config: &config,
            rules: &mut rules,
            influence: &influence,
            now_unix_nanos: 1_000_000_000_000,
            random_index: &mut random,
            shuffle: &mut shuffle,
            supports_conf_change_v2: true,
        };
        for scheduler in [&mut active, &mut restored] {
            let result = scheduler.schedule(&mut ctx, true).await;
            assert_eq!(
                result.operators.len(),
                case["expected"].as_u64().unwrap() as usize,
                "{}",
                case["name"]
            );
            assert!(result.errors.is_empty());
            for op in result.operators {
                assert_eq!(op.region_id(), 1);
                assert_eq!(op.kind(), OpKind::WITNESS | OpKind::REGION);
                let mut to_witness = Vec::new();
                let mut to_non_witness = Vec::new();
                for i in 0..op.len() {
                    match op.step(i).unwrap() {
                        OpStep::BatchSwitchWitness(s) => {
                            to_witness.extend(s.to_witnesses.iter().map(|p| p.store_id));
                            to_non_witness.extend(s.to_non_witnesses.iter().map(|p| p.store_id));
                        }
                        OpStep::BecomeWitness(s) => to_witness.push(s.store_id),
                        OpStep::BecomeNonWitness(s) => to_non_witness.push(s.store_id),
                        _ => {}
                    }
                }
                assert_eq!(to_witness, vec![1]);
                assert_eq!(to_non_witness, vec![4]);
            }
        }
    }
    println!("6/6 configuration-to-decision and restored-decision cases passed");
}

#[tokio::test]
async fn task_139_partial_decode_and_snapshot() {
    let endpoint = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let paths = ClusterPaths::new(139);
    let mut scheduler = BalanceWitnessScheduler::default();
    let original = scheduler.encode_witness_config().unwrap();
    endpoint
        .save_scheduler_config(&paths, "balance-witness-scheduler", &original)
        .await
        .unwrap();
    let (status, body, _) = call(
        &mut scheduler,
        &endpoint,
        "POST",
        "/config",
        r#"{"batch":-1,"ranges":false}"#,
    )
    .await;
    assert_eq!(status, 500);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        "json: cannot unmarshal bool into Go struct field balanceWitnessSchedulerConfig.ranges of type []keyutil.KeyRange"
    );
    assert_eq!(scheduler.config.transport_batch(), -1);
    assert_eq!(
        serde_json::from_slice::<Value>(&scheduler.encode_witness_config().unwrap()).unwrap()["batch"],
        -1
    );
    // Invalid intermediate HTTP state must not bypass the validated domain API.
    assert!(matches!(
        scheduler
            .update_witness_config(scheduler.config.clone(), &endpoint, &paths)
            .await,
        Err(astersql_pd_schedule_schedulers::SchedulerError::InvalidConfig(_))
    ));
    assert_eq!(
        endpoint
            .load_scheduler_config(&paths, "balance-witness-scheduler")
            .await
            .unwrap(),
        original
    );
    assert_eq!(
        call(
            &mut scheduler,
            &endpoint,
            "POST",
            "/config",
            r#"{"batch":4}"#
        )
        .await
        .0,
        200
    );
    assert_eq!(
        call(
            &mut scheduler,
            &endpoint,
            "POST",
            "/config",
            r#"{"ranges":null}"#
        )
        .await
        .0,
        200
    );
    let persisted = endpoint
        .load_scheduler_config(&paths, "balance-witness-scheduler")
        .await
        .unwrap();
    assert!(serde_json::from_slice::<Value>(&persisted).unwrap()["ranges"].is_null());
    let list = call(&mut scheduler, &endpoint, "GET", "/list", "").await;
    assert_eq!(
        serde_json::from_str::<Value>(&list.1).unwrap()["ranges"],
        serde_json::json!([])
    );
    scheduler.config = BalanceWitnessConfig::default();
    scheduler
        .reload_witness_config(&endpoint, &paths)
        .await
        .unwrap();
    assert!(scheduler.config.transport_ranges().is_none());
    let (status, body, _) = call(
        &mut scheduler,
        &endpoint,
        "POST",
        "/config",
        r#"{"batch":2,"ranges":[{"start-key":"zz"}]}"#,
    )
    .await;
    assert_eq!(status, 500);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        "encoding/hex: invalid byte: U+007A 'z'"
    );
    assert_eq!(scheduler.config.transport_batch(), 2);
    assert_eq!(scheduler.config.ranges(), &[KeyRange::default()]);
    assert_eq!(
        endpoint
            .load_scheduler_config(&paths, "balance-witness-scheduler")
            .await
            .unwrap(),
        persisted
    );
    let partial = scheduler.encode_witness_config().unwrap();
    assert_eq!(
        call(
            &mut scheduler,
            &endpoint,
            "POST",
            "/config",
            r#"{"batch":1,"#
        )
        .await
        .0,
        500
    );
    assert_eq!(scheduler.encode_witness_config().unwrap(), partial);
    println!(
        "task139 partial/snapshot cases=7 passed (partial decode, validated rejection, reset, null save, clone, reload, syntax atomicity)"
    );
}
