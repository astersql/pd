// Copyright 2026 AsterSQL.
use crate::schedule_admin_balance_range::serve;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_schedulers::{BalanceRangeJobs, JobTime};
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use serde_json::Value;
use std::sync::Arc;
const NOW: JobTime = JobTime(1_000_000_000_000);
const VALID: &str =
    r#"{"engine":"tikv","rule":"leader-scatter","alias":"work","start-key":"","end-key":""}"#;
fn request(method: &str, path: &str, body: impl Into<Body>) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .body(body.into())
        .unwrap()
}
async fn body(response: axum::response::Response) -> Vec<u8> {
    to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec()
}
#[tokio::test]
async fn task_154_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_154.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let storage = Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default())));
        let paths = ClusterPaths::new(154);
        let mut jobs = BalanceRangeJobs::new(storage.clone(), paths.clone());
        let r = serve(
            &mut jobs,
            NOW,
            request(
                case["method"].as_str().unwrap(),
                case["path"].as_str().unwrap(),
                case["input"].as_str().unwrap().to_owned(),
            ),
        )
        .await;
        assert_eq!(
            r.status().as_u16(),
            case["status"].as_u64().unwrap() as u16,
            "{case}"
        );
        assert!(r.headers().get("allow").is_none());
        assert_eq!(
            r.headers()
                .get("x-content-type-options")
                .map(|h| h.to_str().unwrap()),
            if case["status"] == 404 {
                Some("nosniff")
            } else {
                None
            },
            "{case}"
        );
        assert_eq!(
            r.headers()
                .get("tidb-error-code")
                .map(|h| h.to_str().unwrap()),
            case["code"].as_str(),
            "{case}"
        );
        if case["status"] == 405 {
            assert!(r.headers().get("content-type").is_none());
        } else {
            assert_eq!(
                r.headers()["content-type"],
                if case["status"] == 404 {
                    "text/plain; charset=utf-8"
                } else {
                    "application/json; charset=UTF-8"
                }
            );
        }
        let bytes = body(r).await;
        if case["body"].is_object() {
            assert_eq!(
                serde_json::from_slice::<Value>(&bytes).unwrap(),
                case["body"],
                "{case}"
            );
        } else {
            assert_eq!(bytes, case["body"].as_str().unwrap().as_bytes(), "{case}");
        }
        assert_eq!(
            jobs.jobs().len(),
            case["jobs"].as_u64().unwrap() as usize,
            "{case}"
        );
        let durable = storage
            .load_scheduler_config(&paths, "balance-range-scheduler")
            .await
            .unwrap();
        if jobs.jobs().is_empty() {
            assert!(durable.is_empty(), "{case}");
        } else {
            assert_eq!(durable, jobs.encode_config().unwrap());
            let j = &jobs.jobs()[0];
            assert_eq!(j.job_id, 1);
            assert_eq!(j.create, NOW);
            assert_eq!(
                j.status,
                astersql_pd_schedule_schedulers::JobStatus::Pending
            );
            let input: Value = serde_json::from_str(case["input"].as_str().unwrap()).unwrap();
            let expected = match input["timeout"].as_str() {
                Some("1.5s") => 1_500_000_000,
                Some("1h2m3.000000004s") => 3_723_000_000_004,
                _ => 1_800_000_000_000,
            };
            assert_eq!(j.timeout, expected);
            assert_eq!(
                serde_json::to_value(j).unwrap()["ranges"],
                case.get("ranges")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!([{"start-key":"61","end-key":"7a"}]))
            );
            assert_eq!(j.engine, input["engine"]);
            assert_eq!(j.rule.to_string(), input["rule"]);
        }
    }
    println!(
        "task154: {} source-derived HTTP cases passed",
        fixture["cases"].as_array().unwrap().len()
    );
}

use astersql_pd_storage_kv::{
    Commit, Compare, Lease, MetaWatch, Mutation, RawTxnOp, RawTxnResponse, Read, Snapshot,
    StoreError,
};
use std::sync::atomic::Ordering;
/// Only the I/O boundary is faulted; normal operations use the canonical backend.
#[derive(Default)]
struct FaultStore {
    store: MemoryStore,
    stall: std::sync::atomic::AtomicBool,
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
        if self.stall.load(std::sync::atomic::Ordering::SeqCst) {
            std::future::pending::<()>().await;
        }
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
async fn task_154_job_lifecycle_and_list() {
    let storage = Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default())));
    let paths = ClusterPaths::new(154);
    let mut jobs = BalanceRangeJobs::new(storage.clone(), paths.clone());
    let r = serve(&mut jobs, NOW, request("PUT", "/job", VALID)).await;
    assert_eq!(r.status(), 200);
    assert_eq!(body(r).await, b"null\n");
    let r = serve(&mut jobs, NOW, request("GET", "/list", "ignored")).await;
    assert_eq!(r.status(), 200);
    let expected = serde_json::json!([{"job-id":1,"rule":"leader-scatter","engine":"tikv","timeout":1800000000000i64,"ranges":[{"start-key":"","end-key":""}],"alias":"work","create":"1970-01-01T00:16:40Z","status":"pending"}]);
    assert_eq!(
        serde_json::from_slice::<Value>(&body(r).await).unwrap(),
        expected
    );
    let r = serve(&mut jobs, NOW, request("PUT", "/job", VALID)).await;
    assert_eq!(r.status(), 400);
    assert_eq!(body(r).await, b"\"job already exists\"\n");
    assert_eq!(jobs.jobs().len(), 1);
    // First decoded query value wins, including duplicate parameters and leading zeros.
    let r = serve(
        &mut jobs,
        JobTime(NOW.0 + 1),
        request("DELETE", "/job?job-id=%30%30%31&job-id=99", "{"),
    )
    .await;
    assert_eq!(r.status(), 200);
    assert_eq!(body(r).await, b"null\n");
    let r = serve(&mut jobs, NOW, request("GET", "/list", "")).await;
    let mut cancelled = expected.clone();
    cancelled[0]["status"] = serde_json::json!("cancelled");
    cancelled[0]["start"] = serde_json::json!("1970-01-01T00:16:40.000000001Z");
    cancelled[0]["finish"] = cancelled[0]["start"].clone();
    assert_eq!(
        serde_json::from_slice::<Value>(&body(r).await).unwrap(),
        cancelled
    );
    let r = serve(&mut jobs, NOW, request("DELETE", "/job?job-id=1", "")).await;
    assert_eq!(r.status(), 500);
    assert_eq!(
        serde_json::from_slice::<Value>(&body(r).await).unwrap(),
        serde_json::json!({})
    );
    let r = serve(&mut jobs, NOW, request("PUT", "/job", VALID)).await;
    assert_eq!(r.status(), 200);
    assert_eq!(jobs.jobs()[1].job_id, 2);
    jobs.begin(1, NOW).await.unwrap();
    let r = serve(
        &mut jobs,
        JobTime(NOW.0 + 2),
        request("DELETE", "/job?job-id=2", ""),
    )
    .await;
    assert_eq!(r.status(), 200);
    assert_eq!(jobs.jobs()[1].start, Some(NOW));
    let mut restored = BalanceRangeJobs::new(storage.clone(), paths.clone());
    restored.reload().await.unwrap();
    assert_eq!(restored.jobs(), jobs.jobs());
    assert_eq!(
        storage
            .load_scheduler_config(&paths, "balance-range-scheduler")
            .await
            .unwrap(),
        jobs.encode_config().unwrap()
    );
    println!(
        "task154 lifecycle: add/list/duplicate/cancel/completed-error/alias-reuse/running-cancel/reload checked"
    );
}

#[tokio::test]
async fn task_154_persistence_and_body_faults() {
    let backend = Arc::new(FaultStore::default());
    let storage = Arc::new(StorageEndpoint::new(backend.clone()));
    let paths = ClusterPaths::new(154);
    let mut jobs = BalanceRangeJobs::new(storage.clone(), paths.clone());
    assert_eq!(
        serve(&mut jobs, NOW, request("PUT", "/job", VALID))
            .await
            .status(),
        200
    );
    let original = jobs.jobs().to_vec();
    let durable = storage
        .load_scheduler_config(&paths, "balance-range-scheduler")
        .await
        .unwrap();
    backend.fail.store(true, Ordering::SeqCst);
    let r = serve(
        &mut jobs,
        NOW,
        request("PUT", "/job", VALID.replace("work", "second")),
    )
    .await;
    assert_eq!(r.status(), 400);
    assert_eq!(
        body(r).await,
        b"\"metadata backend unavailable; mutation outcome may be unknown\"\n"
    );
    let r = serve(&mut jobs, NOW, request("DELETE", "/job?job-id=1", "")).await;
    assert_eq!(r.status(), 500);
    assert_eq!(body(r).await, b"{}\n");
    assert_eq!(jobs.jobs(), original);
    for (method, path, status) in [
        ("POST", "/config", 400),
        ("GET", "/list", 200),
        ("GET", "/missing", 404),
        ("GET", "/job", 405),
        ("DELETE", "/job?job-id=bad", 400),
    ] {
        let stream = futures_util::stream::poll_fn(
            |_| -> std::task::Poll<Option<Result<axum::body::Bytes, std::io::Error>>> {
                panic!("non-reading route polled external body")
            },
        );
        assert_eq!(
            serve(
                &mut jobs,
                NOW,
                request(method, path, Body::from_stream(stream))
            )
            .await
            .status(),
            status
        );
    }
    backend.fail.store(false, Ordering::SeqCst);
    assert_eq!(
        storage
            .load_scheduler_config(&paths, "balance-range-scheduler")
            .await
            .unwrap(),
        durable
    );
    for prefix in ["", VALID] {
        let stream = futures_util::stream::iter(vec![
            Ok(axum::body::Bytes::copy_from_slice(prefix.as_bytes())),
            Err(std::io::Error::other("injected body read failure")),
        ]);
        let r = serve(
            &mut jobs,
            NOW,
            request("PUT", "/job", Body::from_stream(stream)),
        )
        .await;
        assert_eq!(r.status(), 500);
        assert_eq!(r.headers()["tidb-error-code"], "internal");
        let value: Value = serde_json::from_slice(&body(r).await).unwrap();
        assert_eq!(value["msg"], "injected body read failure");
        assert_eq!(value["code"], "internal");
        assert_eq!(value["data"], serde_json::json!({}));
        assert!(!value["stack"].as_array().unwrap().is_empty());
        assert_eq!(jobs.jobs(), original);
    }
    backend.stall.store(true, Ordering::SeqCst);
    for (method, path, input) in [
        ("PUT", "/job", VALID.replace("work", "second")),
        ("DELETE", "/job?job-id=1", String::new()),
    ] {
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(5),
                serve(&mut jobs, NOW, request(method, path, input))
            )
            .await
            .is_err()
        );
        assert_eq!(jobs.jobs(), original);
    }
    backend.stall.store(false, Ordering::SeqCst);
    assert_eq!(
        storage
            .load_scheduler_config(&paths, "balance-range-scheduler")
            .await
            .unwrap(),
        durable
    );
    assert_eq!(
        serve(
            &mut jobs,
            NOW,
            request("PUT", "/job", VALID.replace("work", "second"))
        )
        .await
        .status(),
        200
    );
    let mut restored = BalanceRangeJobs::new(storage, paths);
    restored.reload().await.unwrap();
    assert_eq!(restored.jobs(), jobs.jobs());
    assert_eq!(jobs.jobs()[1].job_id, 2);
    println!(
        "task154 faults: add/delete I/O rollback, 5 unread routes, 2 read failures, 2 cancelled saves, retry/reload checked"
    );
}

#[tokio::test]
async fn task_154_http_job_drives_real_schedule() {
    use astersql_pd_core::{
        BasicCluster, KeyRange, Peer, RegionInfo, RegionMeta, RegionOption, StoreInfo, StoreMeta,
    };
    use astersql_pd_core_constant::PriorityLevel;
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_filter::AffinityFilter;
    use astersql_pd_schedule_operator::{OpInfluence, OpKind, OpStep};
    use astersql_pd_schedule_placement::RuleManager;
    use astersql_pd_schedule_schedulers::{BalanceRangeScheduler, JobStatus, RegionBalanceContext};
    use astersql_pd_statistics::StatisticsSnapshot;
    for v2 in [false, true] {
        let mut cluster = BasicCluster::new();
        for id in 1..=3 {
            let mut store = StoreInfo::new(StoreMeta {
                id,
                ..Default::default()
            });
            store.set_last_heartbeat_unix_nanos(NOW.0 as i64);
            cluster.stores.put_store(Arc::new(store));
        }
        for id in 1..=6 {
            let peers: Vec<_> = (1..=3)
                .map(|store_id| Peer {
                    id: id * 10 + store_id,
                    store_id,
                    ..Default::default()
                })
                .collect();
            let region = Arc::new(RegionInfo::new(
                RegionMeta {
                    id,
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
            let (old, overlaps, changed) = cluster.regions.set_region(region.clone());
            cluster
                .regions
                .update_sub_tree(region, old.as_deref(), &overlaps, changed);
        }
        let mut config = ProviderSnapshot::<()>::default();
        config.replication.max_replicas = 3;
        config.replication.enable_placement_rules = true;
        config.schedule.region_schedule_limit = 4;
        config.schedule.max_store_down_time = "1h".parse().unwrap();
        let backend = Arc::new(FaultStore::default());
        let storage = Arc::new(StorageEndpoint::new(backend.clone()));
        let paths = ClusterPaths::new(154);
        let labels = astersql_pd_schedule_labeler::RegionLabeler::load(
            StorageEndpoint::new(backend.clone()),
            paths.clone(),
            0,
        )
        .await
        .unwrap();
        let mut rules = RuleManager::new(storage.clone(), paths.clone());
        rules
            .initialize(3, &[], "", false, &config, &[])
            .await
            .unwrap();
        let ranges = astersql_pd_schedule_keyrange::Manager::new();
        let influence = OpInfluence::new();
        let stats = StatisticsSnapshot::default();
        let mut random = |_: usize| 0;
        let mut shuffle = |_: &mut [usize]| {};
        let mut allocate = || Ok(1000);
        let mut ctx = RegionBalanceContext {
            cluster: &cluster,
            config: &config,
            rules: &mut rules,
            affinity: AffinityFilter {
                manager: None,
                labels: &labels,
                now: 1_000_000_000_000,
            },
            statistics: &stats,
            hot_degree: 1,
            key_ranges: &ranges,
            influence: &influence,
            now_unix_nanos: NOW.0,
            random_index: &mut random,
            shuffle: &mut shuffle,
            allocate: &mut allocate,
            supports_conf_change_v2: v2,
            allow_empty_regions: true,
        };
        let mut jobs = BalanceRangeJobs::new(storage.clone(), paths.clone());
        let mut scheduler = BalanceRangeScheduler::default();
        assert!(!scheduler.check_job(&mut jobs, &ctx, 0).await.unwrap());
        assert!(scheduler.schedule(&mut ctx).await.operators.is_empty());
        assert_eq!(
            serve(&mut jobs, NOW, request("PUT", "/job", VALID))
                .await
                .status(),
            200
        );
        assert!(!scheduler.check_job(&mut jobs, &ctx, 4).await.unwrap());
        assert_eq!(jobs.jobs()[0].status, JobStatus::Pending);
        assert!(scheduler.check_job(&mut jobs, &ctx, 0).await.unwrap());
        assert_eq!(jobs.jobs()[0].status, JobStatus::Running);
        assert!(!ranges.is_empty());
        assert_eq!(scheduler.scores[&1], 6.0);
        assert_eq!(scheduler.expected[&1], 2.0);
        let result = scheduler.schedule(&mut ctx).await;
        assert!(result.errors.is_empty());
        assert_eq!(result.operators.len(), 1);
        let op = &result.operators[0];
        assert_eq!(op.region_id(), 1);
        assert_eq!(op.len(), 1);
        assert_eq!(op.kind(), OpKind(OpKind::RANGE.0 | OpKind::LEADER.0));
        assert_eq!(op.priority_level(), PriorityLevel::MEDIUM);
        assert_eq!(
            op.additional_info().get("sourceScore").as_deref(),
            Some("6.00")
        );
        assert_eq!(
            op.additional_info().get("targetScore").as_deref(),
            Some("0.00")
        );
        let OpStep::TransferLeader(step) = op.step(0).unwrap() else {
            panic!("expected real transfer leader")
        };
        assert_eq!(step.from_store, 1);
        assert_eq!(step.to_store, 3);
        let region = cluster.regions.get_region(op.region_id()).unwrap();
        assert_eq!(step.conf_ver_changed(&region), 0);
        let delta = OpInfluence::new();
        step.influence(&delta, &region);
        assert_eq!(delta.get_store_influence(1).leader_count, -1);
        assert_eq!(delta.get_store_influence(3).leader_count, 1);
        assert_eq!(delta.get_store_influence(1).region_count, 0);
        let command = step.get_cmd(&region, v2).transfer_leader.unwrap();
        assert_eq!(command.peer.unwrap().store_id, 3);
        assert!(command.peers.is_empty());
        assert!(op.start());
        assert!(op.check(&region).is_some());
        let applied = region.clone_with([RegionOption::Leader(region.get_store_peer(3).cloned())]);
        assert!(op.check(&applied).is_none());
        assert!(op.check_success());
        // A failed HTTP cancellation retains the running job and active scheduler state.
        backend.fail.store(true, Ordering::SeqCst);
        assert_eq!(
            serve(&mut jobs, NOW, request("DELETE", "/job?job-id=1", ""))
                .await
                .status(),
            500
        );
        assert_eq!(jobs.jobs()[0].status, JobStatus::Running);
        assert!(!ranges.is_empty());
        backend.fail.store(false, Ordering::SeqCst);
        assert_eq!(
            serve(&mut jobs, NOW, request("DELETE", "/job?job-id=1", ""))
                .await
                .status(),
            200
        );
        assert!(!scheduler.check_job(&mut jobs, &ctx, 0).await.unwrap());
        assert!(scheduler.schedule(&mut ctx).await.operators.is_empty());
        assert!(ranges.is_empty());
        assert!(scheduler.report.scores.values().all(|v| *v == (0.0, 0.0)));
        let mut restored = BalanceRangeJobs::new(storage, paths);
        restored.reload().await.unwrap();
        assert_eq!(restored.jobs()[0].status, JobStatus::Cancelled);
    }
    println!(
        "task154: 2 real schedule cases (conf-change v1/v2), HTTP add -> Range|Leader 1->3 -> failed/successful cancel -> cleanup"
    );
}
