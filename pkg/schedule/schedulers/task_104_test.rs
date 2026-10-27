// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::MemoryStore;
use astersql_pd_storage_domain::StorageEndpoint;
use std::sync::{Arc, atomic::Ordering};
fn job(alias: &str) -> BalanceRangeJob {
    BalanceRangeJob::from_args(
        &["leader-scatter", "tikv", "1s", alias, "a", "b"],
        JobTime(0),
    )
    .unwrap()
}
fn setup() -> (Arc<FaultStore>, Arc<StorageEndpoint>, BalanceRangeJobs) {
    let backend = Arc::new(FaultStore::default());
    let storage = Arc::new(StorageEndpoint::new(backend.clone()));
    let q = BalanceRangeJobs::new(storage.clone(), ClusterPaths::new(104));
    (backend, storage, q)
}
#[test]
fn task_104_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_104.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    for case in cases {
        let got = BalanceRangeJob::from_http(case["input"].as_object().unwrap(), JobTime(17));
        assert_eq!(
            got.is_ok(),
            case["ok"].as_bool().unwrap(),
            "{}: {got:?}",
            case["name"]
        );
        if let Ok(j) = got {
            assert_eq!(j.timeout, case["timeout"].as_i64().unwrap());
            assert_eq!(j.create, JobTime(17));
            assert_eq!(j.alias, "a");
            assert_eq!(j.engine, case["input"]["engine"]);
            assert_eq!(j.rule.to_string(), case["input"]["rule"]);
            assert_eq!(j.status, JobStatus::Pending);
            let bytes = serde_json::to_value(&j).unwrap();
            assert_eq!(bytes["ranges"][0]["start-key"], case["start"]);
            assert_eq!(bytes["ranges"][0]["end-key"], case["end"]);
            assert_eq!(
                j.ranges.len(),
                case["ranges"].as_u64().unwrap_or(1) as usize
            );
        }
    }
    for args in [
        vec![],
        vec!["leader-scatter", "tikv", "1s", "a"],
        vec!["bad", "tikv", "1s", "a", ""],
        vec!["peer-scatter", "other", "1s", "a", ""],
        vec!["peer-scatter", "tikv", "0s", "a", ""],
        vec!["peer-scatter", "tikv", "1s", "%q0", ""],
    ] {
        assert!(BalanceRangeJob::from_args(&args, JobTime(0)).is_err());
    }
    let j = BalanceRangeJob::from_args(&["peer%2Dscatter", "tiflash", "1m", "", "x"], JobTime(0))
        .unwrap();
    assert_eq!(j.alias, "");
    assert_eq!(j.timeout, 60_000_000_000);
    assert_eq!(j.ranges[0].start_key, b""); // unmatched arg ignored, full range default
    println!(
        "task_104: {}/{} HTTP fixture cases, 7 slice cases",
        cases.len(),
        cases.len()
    );
}
#[test]
fn task_104_status_codec_and_time() {
    for (status, text) in [
        (JobStatus::Pending, "pending"),
        (JobStatus::Running, "running"),
        (JobStatus::Finished, "finished"),
        (JobStatus::Cancelled, "cancelled"),
        (JobStatus::Unknown(9), "unknown"),
    ] {
        assert_eq!(
            serde_json::to_string(&status).unwrap(),
            format!("\"{text}\"")
        );
        let got: JobStatus = serde_json::from_str(&format!("\"{text}\"")).unwrap();
        assert_eq!(
            got,
            if text == "unknown" {
                JobStatus::Pending
            } else {
                status
            }
        );
    }
    for raw in [
        "null",
        "42",
        "{}",
        "[]",
        "true",
        "\"unknown\"",
        r#""\u0072unning""#,
    ] {
        let mut status = JobStatus::Finished;
        status.unmarshal_json(raw.as_bytes());
        assert_eq!(status, JobStatus::Finished);
        assert_eq!(
            serde_json::from_str::<JobStatus>(raw).unwrap(),
            JobStatus::Pending
        );
    }
    let default: BalanceRangeJob = serde_json::from_str("{}").unwrap();
    assert_eq!(default.rule, astersql_pd_core::Rule::LeaderScatter);
    assert_eq!(default.timeout, 0);
    let escaped: BalanceRangeJob =
        serde_json::from_str(r#"{"rule":"\u006ceader-scatter"}"#).unwrap();
    assert_eq!(escaped.rule, astersql_pd_core::Rule::Unknown);
    let mut j = job("a");
    assert!(!j.expired(JobTime(i128::MAX), RANGE_JOB_RETENTION));
    assert!(j.should_finish(JobTime(0)).is_err());
    j.start = Some(JobTime(0));
    assert!(!j.should_finish(JobTime(1_000_000_000)).unwrap());
    assert!(j.should_finish(JobTime(1_000_000_001)).unwrap());
    j.finish = Some(JobTime(0));
    assert!(!j.expired(JobTime(RANGE_JOB_RETENTION), RANGE_JOB_RETENTION));
    assert!(j.expired(JobTime(RANGE_JOB_RETENTION + 1), RANGE_JOB_RETENTION));
    assert!(!j.expired(JobTime(-1), RANGE_JOB_RETENTION));
    for (time, text) in [
        (JobTime::default(), "0001-01-01T00:00:00Z"),
        (JobTime(1), "1970-01-01T00:00:00.000000001Z"),
        (JobTime(10), "1970-01-01T00:00:00.00000001Z"),
        (JobTime(1_000_000_000), "1970-01-01T00:00:01Z"),
    ] {
        assert_eq!(serde_json::to_string(&time).unwrap(), format!("\"{text}\""));
        assert_eq!(
            serde_json::from_str::<JobTime>(&format!("\"{text}\"")).unwrap(),
            time
        );
    }
    let encoded = serde_json::to_value(&job("a")).unwrap();
    assert!(encoded.get("start").is_none());
    assert!(encoded.get("finish").is_none());
    assert_eq!(encoded["timeout"], 1_000_000_000_i64);
    assert_eq!(
        encoded["ranges"][0],
        serde_json::json!({"start-key":"61","end-key":"62"})
    );
    assert_eq!(
        serde_json::from_value::<BalanceRangeJob>(encoded).unwrap(),
        job("a")
    );
}
#[tokio::test]
async fn task_104_queue_lifecycle() {
    let (_, storage, mut q) = setup();
    assert!(q.peek().is_none());
    assert_eq!(q.add(job("a")).await.unwrap(), 1);
    assert!(matches!(
        q.add(job("a")).await,
        Err(RangeJobError::Duplicate)
    ));
    assert!(matches!(
        q.finish(0, JobTime(1)).await,
        Err(RangeJobError::State("running"))
    ));
    q.begin(0, JobTime(10)).await.unwrap();
    assert_eq!(q.jobs()[0].status, JobStatus::Running);
    assert!(matches!(
        q.begin(0, JobTime(11)).await,
        Err(RangeJobError::State("pending"))
    ));
    q.finish(0, JobTime(20)).await.unwrap();
    assert!(matches!(
        q.delete(1, JobTime(30)).await,
        Err(RangeJobError::Complete(1))
    ));
    assert_eq!(q.add(job("a")).await.unwrap(), 2);
    assert_eq!(q.peek().unwrap().0, 1);
    q.delete(2, JobTime(30)).await.unwrap();
    assert_eq!(q.jobs()[1].start, Some(JobTime(30)));
    assert_eq!(q.jobs()[1].finish, Some(JobTime(30)));
    assert!(matches!(
        q.delete(99, JobTime(30)).await,
        Err(RangeJobError::NotFound(99))
    ));
    assert_eq!(q.add(job("b")).await.unwrap(), 3);
    q.begin(2, JobTime(40)).await.unwrap();
    q.delete(3, JobTime(50)).await.unwrap();
    assert_eq!(q.jobs()[2].start, Some(JobTime(40)));
    let mut rebuilt = BalanceRangeJobs::new(storage.clone(), ClusterPaths::new(104));
    rebuilt.reload().await.unwrap();
    assert_eq!(rebuilt.jobs(), q.jobs());
    assert_eq!(q.gc(JobTime(RANGE_JOB_RETENTION + 20)).await.unwrap(), 0);
    assert_eq!(q.gc(JobTime(RANGE_JOB_RETENTION + 21)).await.unwrap(), 1);
    assert_eq!(q.gc(JobTime(RANGE_JOB_RETENTION + 51)).await.unwrap(), 2);
    assert_eq!(q.add(job("reset")).await.unwrap(), 1); // ID reuses after all GC
    let factory = BalanceRangeJobs::from_args(
        storage,
        ClusterPaths::new(1),
        &["peer-scatter", "tikv", "1s", "", ""],
        JobTime(0),
    )
    .unwrap();
    assert_eq!(factory.jobs()[0].job_id, 0);
    assert!(matches!(
        q.begin(99, JobTime(0)).await,
        Err(RangeJobError::Index)
    ));
}
#[tokio::test]
async fn task_104_persistence_faults_and_gc_prefix() {
    let (backend, storage, mut q) = setup();
    q.add(job("a")).await.unwrap();
    let saved = q.encode_config().unwrap();
    backend.fail.store(true, Ordering::SeqCst);
    assert!(matches!(
        q.add(job("b")).await,
        Err(RangeJobError::Persistence(_))
    ));
    assert!(q.delete(1, JobTime(1)).await.is_err());
    assert!(q.begin(0, JobTime(1)).await.is_err());
    assert!(q.reload().await.is_err());
    assert_eq!(q.encode_config().unwrap(), saved);
    backend.fail.store(false, Ordering::SeqCst);
    q.begin(0, JobTime(10)).await.unwrap();
    let running = q.encode_config().unwrap();
    backend.fail.store(true, Ordering::SeqCst);
    assert!(q.finish(0, JobTime(20)).await.is_err());
    assert_eq!(q.encode_config().unwrap(), running);
    backend.fail.store(false, Ordering::SeqCst);
    backend.stall.store(true, Ordering::SeqCst);
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(10),
            q.finish(0, JobTime(20))
        )
        .await
        .is_err()
    );
    assert_eq!(q.encode_config().unwrap(), running);
    backend.stall.store(false, Ordering::SeqCst);
    q.finish(0, JobTime(20)).await.unwrap();
    let complete = q.encode_config().unwrap();
    backend.fail.store(true, Ordering::SeqCst);
    assert!(q.gc(JobTime(RANGE_JOB_RETENTION + 21)).await.is_err());
    assert_eq!(q.encode_config().unwrap(), complete);
    backend.fail.store(false, Ordering::SeqCst);
    let mut rebuilt = BalanceRangeJobs::new(storage.clone(), ClusterPaths::new(104));
    rebuilt.reload().await.unwrap();
    assert_eq!(rebuilt.encode_config().unwrap(), complete);
    // An expired later job is retained behind the first incomplete job.
    q.add(job("b")).await.unwrap();
    q.add(job("c")).await.unwrap();
    q.delete(3, JobTime(0)).await.unwrap();
    assert_eq!(q.gc(JobTime(RANGE_JOB_RETENTION + 30)).await.unwrap(), 1);
    assert_eq!(q.jobs().len(), 2);
    assert_eq!(q.peek().unwrap().1.job_id, 2);
    let before = q.encode_config().unwrap();
    storage
        .save_scheduler_config(&ClusterPaths::new(104), "balance-range-scheduler", b"{")
        .await
        .unwrap();
    assert!(q.reload().await.is_err());
    assert_eq!(q.encode_config().unwrap(), before);
    storage
        .save_scheduler_config(&ClusterPaths::new(104), "balance-range-scheduler", &before)
        .await
        .unwrap();
    q.reload().await.unwrap();
}

/// Only the I/O boundary is faulted; normal operations use the canonical backend.
#[derive(Default)]
struct FaultStore {
    store: MemoryStore,
    fail: std::sync::atomic::AtomicBool,
    stall: std::sync::atomic::AtomicBool,
}
#[async_trait::async_trait]
impl astersql_pd_storage_domain::MetaStore for FaultStore {
    async fn read(&self, r: Read) -> astersql_pd_storage_domain::Result<Snapshot> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(StoreError::Unavailable);
        }
        self.store.read(r).await
    }
    async fn transact(
        &self,
        c: Vec<Compare>,
        w: Vec<Mutation>,
    ) -> astersql_pd_storage_domain::Result<Commit> {
        if self.stall.load(Ordering::SeqCst) {
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
    ) -> astersql_pd_storage_domain::Result<Commit> {
        self.store.transact_branches(c, t, e).await
    }
    async fn transact_raw(
        &self,
        c: Vec<Compare>,
        t: Vec<RawTxnOp>,
        e: Vec<RawTxnOp>,
    ) -> astersql_pd_storage_domain::Result<RawTxnResponse> {
        self.store.transact_raw(c, t, e).await
    }
    async fn watch(
        &self,
        p: Vec<u8>,
        r: i64,
    ) -> astersql_pd_storage_domain::Result<Box<dyn MetaWatch>> {
        self.store.watch(p, r).await
    }
    async fn grant(&self, t: i64) -> astersql_pd_storage_domain::Result<Lease> {
        self.store.grant(t).await
    }
    async fn keep_alive(&self, l: i64) -> astersql_pd_storage_domain::Result<Lease> {
        self.store.keep_alive(l).await
    }
    async fn revoke(&self, l: i64) -> astersql_pd_storage_domain::Result<()> {
        self.store.revoke(l).await
    }
    async fn ttl(&self, l: i64) -> astersql_pd_storage_domain::Result<i64> {
        self.store.ttl(l).await
    }
    async fn compact(&self, r: i64) -> astersql_pd_storage_domain::Result<()> {
        self.store.compact(r).await
    }
}

use astersql_pd_storage_domain::{
    Commit, Compare, Lease, MetaWatch, Mutation, RawTxnOp, RawTxnResponse, Read, Snapshot,
    StoreError,
};
