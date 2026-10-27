// Copyright 2026 AsterSQL.
use crate::schedule_admin_balance_leader::serve;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_schedulers::BalanceLeaderScheduler;
use astersql_pd_storage_endpoint::StorageEndpoint;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use serde_json::Value;
use std::sync::Arc;

async fn call(
    s: &mut BalanceLeaderScheduler,
    e: &StorageEndpoint,
    method: &str,
    path: &str,
    body: &str,
) -> (u16, String, String) {
    let response = serve(
        s,
        e,
        &ClusterPaths::new(137),
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
async fn task_137_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_137.json"
    ))
    .unwrap();
    let endpoint = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let mut scheduler = BalanceLeaderScheduler::default();
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
        }
        assert_eq!(
            scheduler.config.batch(),
            case["batch"].as_u64().unwrap() as usize
        );
    }
    println!(
        "task137 HTTP cases={} passed",
        fixture["cases"].as_array().unwrap().len()
    );
}

use astersql_pd_core::{StoreInfo, StoreMeta};
use astersql_pd_schedule_schedulers::{BalanceLeaderConfig, LeaderBalanceContext};
fn store(id: u64, count: i64) -> Arc<StoreInfo> {
    let mut s = StoreInfo::new(StoreMeta {
        id,
        ..Default::default()
    });
    s.set_leader_count(count);
    s.set_last_heartbeat_unix_nanos(1_000_000_000_000);
    Arc::new(s)
}
#[tokio::test]
async fn task_137_updated_scheduling_cases() {
    use astersql_pd_core::{BasicCluster as CoreCluster, KeyRange, Peer, RegionInfo, RegionMeta};
    use astersql_pd_keypath::ClusterPaths;
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_filter::AffinityFilter;
    use astersql_pd_schedule_operator::{OpInfluence, OpStep};
    use astersql_pd_schedule_placement::RuleManager;
    use astersql_pd_statistics::StatisticsSnapshot;
    use astersql_pd_storage_endpoint::StorageEndpoint;
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_137.json"
    ))
    .unwrap();
    let cases = fixture["decision_cases"].as_array().unwrap();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let counts = case["counts"]
            .as_array()
            .unwrap_or(fixture["defaults"]["counts"].as_array().unwrap());
        let sizes = case["sizes"]
            .as_array()
            .unwrap_or(fixture["defaults"]["sizes"].as_array().unwrap());
        let mut core = CoreCluster::new();
        for id in 1..=3 {
            let mut st = (*store(id, counts[id as usize - 1].as_i64().unwrap())).clone();
            st.set_leader_size(sizes[id as usize - 1].as_i64().unwrap());
            core.stores.put_store(Arc::new(st));
        }
        for id in 1..=8 {
            let peers: Vec<_> = (1..=3)
                .map(|store_id| Peer {
                    id: id * 10 + store_id,
                    store_id,
                    ..Default::default()
                })
                .collect();
            let mut region = RegionInfo::new(
                RegionMeta {
                    id,
                    key_range: KeyRange {
                        start_key: vec![b'a' + id as u8 - 1],
                        end_key: vec![b'a' + id as u8],
                    },
                    peers: peers.clone(),
                    ..Default::default()
                },
                Some(peers[0].clone()),
                vec![],
                vec![],
            );
            region.stats_mut().approximate_size = 10;
            let region = Arc::new(region);
            let (o, v, b) = core.regions.set_region(region.clone());
            core.regions.update_sub_tree(region, o.as_deref(), &v, b);
        }
        let mut config = ProviderSnapshot::<()>::default();
        config.schedule.tolerant_size_ratio = 1.0;
        config.schedule.leader_schedule_policy = "count".into();
        config.replication.enable_placement_rules = false;
        let storage = Arc::new(MemoryStore::default());
        let mut rules = RuleManager::new(
            Arc::new(StorageEndpoint::new(storage.clone())),
            ClusterPaths::new(71),
        );
        rules
            .initialize(3, &[], "", false, &config, &[])
            .await
            .unwrap();
        let labels = astersql_pd_schedule_labeler::RegionLabeler::load(
            StorageEndpoint::new(storage),
            ClusterPaths::new(71),
            Default::default(),
        )
        .await
        .unwrap();
        let key_ranges = astersql_pd_schedule_keyrange::Manager::new();
        let influence = OpInfluence::new();
        let before = influence.snapshot();
        let statistics = StatisticsSnapshot::default();
        let mut next = 0usize;
        let mut index = |n: usize| {
            let result = (next / 20) % n;
            next += 1;
            result
        };
        let mut permutations = 0usize;
        let mut shuffle = |order: &mut [usize]| {
            let n = order.len();
            order.rotate_left(permutations % n);
            permutations += 1;
        };

        let endpoint = Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default())));
        let paths = ClusterPaths::new(71);
        let mut scheduler = BalanceLeaderScheduler::default();
        let ranges = case["ranges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| KeyRange {
                start_key: r[0].as_str().unwrap().as_bytes().to_vec(),
                end_key: r[1].as_str().unwrap().as_bytes().to_vec(),
            })
            .collect();
        let mut candidate =
            BalanceLeaderConfig::new(ranges, case["batch"].as_u64().unwrap() as usize).unwrap();
        if case["empty"] == true {
            candidate.set_ranges(vec![]);
        }
        let original = scheduler.encode_leader_config().unwrap();
        let request = serde_json::to_string(&candidate).unwrap();
        let response = serve(
            &mut scheduler,
            &endpoint,
            &paths,
            Request::builder()
                .method("POST")
                .uri("/config")
                .body(Body::from(request))
                .unwrap(),
        )
        .await;
        assert_eq!(response.status(), 200);
        assert_ne!(scheduler.encode_leader_config().unwrap(), original);
        let encoded = scheduler.encode_leader_config().unwrap();
        let actual: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(actual["batch"], case["batch"]);
        for (actual, expected) in actual["ranges"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["ranges"].as_array().unwrap())
        {
            let hex = |s: &str| {
                s.as_bytes()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            };
            assert_eq!(actual["start-key"], hex(expected[0].as_str().unwrap()));
            assert_eq!(actual["end-key"], hex(expected[1].as_str().unwrap()));
        }
        let request = serde_json::to_string(&candidate).unwrap();
        let response = serve(
            &mut scheduler,
            &endpoint,
            &paths,
            Request::builder()
                .method("POST")
                .uri("/config")
                .body(Body::from(request))
                .unwrap(),
        )
        .await;
        assert_eq!(response.status(), 200);
        let mut restored = BalanceLeaderScheduler::default();
        restored
            .reload_leader_config(&endpoint, &paths)
            .await
            .unwrap();
        assert_eq!(restored.encode_leader_config().unwrap(), encoded);
        let mut ctx = LeaderBalanceContext {
            cluster: &core,
            config: &config,
            rules: &mut rules,
            affinity: AffinityFilter {
                manager: None,
                labels: &labels,
                now: 1_000_000_000_000,
            },
            statistics: &statistics,
            hot_degree: 3,
            key_ranges: &key_ranges,
            influence: &influence,
            now_unix_nanos: 1_000_000_000_000,
            random_index: &mut index,
            shuffle: &mut shuffle,
        };
        let mut original_scheduler = BalanceLeaderScheduler::default();
        assert_eq!(
            original_scheduler
                .schedule(&mut ctx, false)
                .await
                .operators
                .len(),
            4
        );
        for active in [&mut scheduler, &mut restored] {
            let result = active.schedule(&mut ctx, false).await;
            assert_eq!(
                result.operators.len(),
                case["expected"].as_u64().unwrap() as usize,
                "{name}"
            );
            assert!(result.errors.is_empty());
            let mut ids = std::collections::HashSet::new();
            for op in result.operators {
                let OpStep::TransferLeader(step) = op.step(0).unwrap() else {
                    panic!("wrong command")
                };
                assert_eq!(step.from_store, 1);
                assert!([2, 3].contains(&step.to_store));
                assert!(ids.insert(op.region_id()));
                let start = vec![b'a' + op.region_id() as u8 - 1];
                let end = vec![b'a' + op.region_id() as u8];
                assert!(
                    active.config.ranges().is_empty()
                        || active
                            .config
                            .ranges()
                            .iter()
                            .any(|r| r.start_key <= start
                                && (r.end_key.is_empty() || end <= r.end_key))
                );
            }
        }
        assert_eq!(influence.snapshot(), before);
    }
    println!("task137 HTTP-to-real-decision cases=7 passed");
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
async fn task_137_persistence_failure() {
    use std::sync::atomic::Ordering;
    let backend = Arc::new(FaultStore::default());
    let endpoint = StorageEndpoint::new(backend.clone());
    let paths = ClusterPaths::new(137);
    let mut scheduler = BalanceLeaderScheduler::default();
    let old = scheduler.encode_leader_config().unwrap();
    endpoint
        .save_scheduler_config(&paths, &scheduler.name, &old)
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
            .load_scheduler_config(&paths, &scheduler.name)
            .await
            .unwrap(),
        old
    );
    scheduler
        .reload_leader_config(&endpoint, &paths)
        .await
        .unwrap();
    assert_eq!(scheduler.config.batch(), 4);
    let (status, _, _) = call(
        &mut scheduler,
        &endpoint,
        "POST",
        "/config",
        r#"{"batch":100}"#,
    )
    .await;
    assert_eq!(status, 200);
    scheduler.config.set_batch(1).unwrap();
    scheduler
        .reload_leader_config(&endpoint, &paths)
        .await
        .unwrap();
    assert_eq!(scheduler.config.batch(), 100);
    println!("task137 fault cases=4 passed (publish, unchanged, old reload, recovered save)");
}

#[tokio::test]
async fn task_137_body_read_failure() {
    let endpoint = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let mut scheduler = BalanceLeaderScheduler::default();
    for (partial, expected) in [(r#"{"batch":2}"#, 200), (r#"{"batch":"#, 500)] {
        let chunks = futures_util::stream::iter(vec![
            Ok(axum::body::Bytes::from(partial)),
            Err(std::io::Error::other("injected read error")),
        ]);
        let response = serve(
            &mut scheduler,
            &endpoint,
            &ClusterPaths::new(137),
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
    println!("task137 body I/O cases=2 passed");
}
