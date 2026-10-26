// Copyright 2026 AsterSQL.
use super::*;
use serde_json::{Value, json};

#[test]
fn task_085_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_085.json"
    ))
    .unwrap();
    for case in fixture["priority_cases"].as_array().unwrap() {
        let scope = match case["scope"].as_str().unwrap() {
            "read" => HotPriorityScope::Read,
            "write-leader" => HotPriorityScope::WriteLeader,
            _ => HotPriorityScope::WritePeer,
        };
        let origins: Vec<String> = serde_json::from_value(case["origins"].clone()).unwrap();
        let expected: Vec<String> = serde_json::from_value(case["expected"].clone()).unwrap();
        assert_eq!(
            adjust_hot_priorities(
                case["query"].as_bool().unwrap(),
                case["cpu"].as_bool().unwrap(),
                &origins,
                scope
            ),
            expected,
            "{case}"
        );
    }
    println!("task_085: 96 frozen source-derived priority cases passed");
}

#[test]
fn task_085_validation_and_effective_snapshot() {
    let config = HotRegionParams::default();
    assert!(config.validate().is_ok());
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_085.json"
    ))
    .unwrap();
    let expected: HotRegionParams = serde_json::from_value(fixture["defaults"].clone()).unwrap();
    assert_eq!(config, expected);
    assert_eq!(TIFLASH_TOLERANCE_RATIO_CORRECTION, 0.1);
    assert_eq!(config.min_hot_byte_rate, 100.);
    assert_eq!(config.min_hot_key_rate, 10.);
    assert_eq!(config.min_hot_query_rate, 10.);
    assert_eq!(config.min_hot_cpu_rate, 10.);
    assert_eq!(config.max_peer_number, 1000);
    assert_eq!(
        config.store_stat_zombie_duration().as_nanos(),
        30_000_000_000
    );
    assert_eq!(
        config.region_stat_zombie_duration().as_nanos(),
        180_000_000_000
    );
    assert_eq!(config.history_sample_duration.as_nanos(), 300_000_000_000);
    assert_eq!(config.history_sample_interval.as_nanos(), 30_000_000_000);
    let mut state = HotRegionConfig::default();
    assert_eq!(state.valid_config().read_priorities, ["byte", "key"]);
    state.set_support(true, false);
    assert_eq!(state.valid_config().read_priorities, ["query", "byte"]);
    state.set_support(true, true);
    assert_eq!(state.valid_config().read_priorities, ["cpu", "byte"]);
    assert_eq!(state.params().read_priorities, ["cpu", "byte"]);
    assert_eq!(state.valid_config().forbid_rw_type, "");
    assert_eq!(state.params().forbid_rw_type, "none");
    for (patch, message) in [
        (
            json!({"read-priorities":["unknown","byte"]}),
            "invalid scheduling dimensions",
        ),
        (
            json!({"read-priorities":["key","key"]}),
            "priorities shouldn't be repeated",
        ),
        (
            json!({"read-priorities":["key"]}),
            "priorities should have at least 2 dimensions",
        ),
        (
            json!({"write-leader-priorities":["cpu","byte"]}),
            "cpu is not allowed to be set in priorities for write-leader-priorities",
        ),
        (
            json!({"write-peer-priorities":["cpu","byte"]}),
            "cpu is not allowed to be set in priorities for write-peer-priorities",
        ),
        (
            json!({"write-peer-priorities":["query","byte"]}),
            "query is not allowed to be set in priorities for write-peer-priorities",
        ),
        (
            json!({"rank-formula-version":"v3"}),
            "invalid rank-formula-version",
        ),
        (json!({"forbid-rw-type":"all"}), "invalid forbid-rw-type"),
        (
            json!({"pending-weight":0.999}),
            "invalid pending-weight, should be at least 1",
        ),
        (
            json!({"split-thresholds":0.009}),
            "invalid split-thresholds, should be in range [0.01, 1.0]",
        ),
        (
            json!({"split-thresholds":1.001}),
            "invalid split-thresholds, should be in range [0.01, 1.0]",
        ),
    ] {
        let mut value = serde_json::to_value(&config).unwrap();
        for (key, v) in patch.as_object().unwrap() {
            value[key] = v.clone();
        }
        let candidate: HotRegionParams = serde_json::from_value(value).unwrap();
        assert_eq!(candidate.validate().unwrap_err().to_string(), message);
    }
    for version in ["", "v1", "v2"] {
        for forbid in ["", "none", "read", "write"] {
            for threshold in [0.01, 1.0] {
                let p = HotRegionParams {
                    rank_formula_version: version.into(),
                    forbid_rw_type: forbid.into(),
                    split_thresholds: threshold,
                    ..config.clone()
                };
                p.validate().unwrap();
                assert_eq!(p.rank_version(), if version == "v2" { "v2" } else { "v1" });
                assert_eq!(p.is_forbidden("read"), forbid == "read");
                assert_eq!(p.is_forbidden("write"), forbid == "write");
            }
        }
    }
    // Validation deliberately accepts empty and 3/4 distinct read dimensions;
    // effective projection repairs them to a 2-dimensional default.
    for priorities in [
        vec![],
        vec!["byte", "key", "query"],
        vec!["cpu", "byte", "key", "query"],
    ] {
        let p = HotRegionParams {
            read_priorities: priorities.iter().map(|s| s.to_string()).collect(),
            ..config.clone()
        };
        p.validate().unwrap();
        assert_eq!(p.effective(true, true).read_priorities, ["cpu", "byte"]);
    }
    let p = HotRegionParams {
        history_sample_interval: astersql_pd_typeutil::Duration::from_nanos(-1),
        min_hot_byte_rate: -1.,
        ..config
    };
    p.validate().unwrap(); // Go adds no constraints on these values.
    let encoded = serde_json::to_value(&p).unwrap();
    assert_eq!(encoded["enable-for-tiflash"], "true");
    assert_eq!(encoded["strict-picking-store"], "true");
    assert_eq!(encoded["history-sample-interval"], "-1ns");
    assert_eq!(encoded["history-sample-duration"], "5m0s");
    assert_eq!(
        serde_json::from_value::<HotRegionParams>(encoded).unwrap(),
        p
    );
    println!("task_085: 11 rejection, 24 boundary, 3 variable-dimension cases passed");
}

use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use std::sync::Arc;
use std::sync::atomic::Ordering;
/// Only the I/O boundary is faulted; normal operations use the canonical backend.
#[derive(Default)]
struct FaultStore {
    store: MemoryStore,
    fail: std::sync::atomic::AtomicBool,
    stall: std::sync::atomic::AtomicBool,
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

use astersql_pd_storage_kv::{
    Commit, Compare, Lease, MetaWatch, Mutation, RawTxnOp, RawTxnResponse, Read, Snapshot,
    StoreError,
};

#[tokio::test]
async fn task_085_update_reload_and_persist_failure() {
    let backend = Arc::new(FaultStore::default());
    let storage = StorageEndpoint::new(backend.clone());
    let paths = ClusterPaths::new(85);
    let mut state = HotRegionConfig::default();
    state.set_support(true, true);
    let original = state.encode().unwrap();
    assert!(state.reload(&storage, &paths).await.is_err());
    assert_eq!(state.encode().unwrap(), original);
    assert!(matches!(state.update_json(br#"{"pending-weight":2,"rank-formula-version":"v1","forbid-rw-type":"write","enable-for-tiflash":"false","strict-picking-store":"false","history-sample-interval":"1.5s"}"#, &storage, &paths).await.unwrap(), HotConfigUpdate::Updated {persistence_error:None}));
    assert_eq!(state.params().pending_weight, 2.);
    assert_eq!(
        state.params().history_sample_interval.as_nanos(),
        1_500_000_000
    );
    assert!(!state.params().enable_for_tiflash);
    assert!(!state.params().strict_picking_store);
    assert_eq!(state.params().rank_version(), "v1");
    assert!(state.params().is_forbidden("write"));
    let saved = state.encode().unwrap();
    assert_eq!(
        storage
            .load_scheduler_config(&paths, HOT_REGION_CONFIG_NAME)
            .await
            .unwrap(),
        saved
    );
    // Failed persistence retains the live value; unchanged updates avoid I/O.
    backend.fail.store(true, Ordering::SeqCst);
    assert!(matches!(
        state
            .update_json(br#"{"pending-weight":3}"#, &storage, &paths)
            .await
            .unwrap(),
        HotConfigUpdate::Updated {
            persistence_error: Some(_)
        }
    ));
    assert_eq!(state.params().pending_weight, 3.);
    assert!(matches!(
        state
            .update_json(br#"{"pending-weight":3}"#, &storage, &paths)
            .await
            .unwrap(),
        HotConfigUpdate::Unchanged
    ));
    let live = state.encode().unwrap();
    assert!(state.reload(&storage, &paths).await.is_err());
    assert_eq!(state.encode().unwrap(), live);
    backend.fail.store(false, Ordering::SeqCst);
    let mut restored = HotRegionConfig::default();
    restored.reload(&storage, &paths).await.unwrap();
    assert_eq!(restored.encode().unwrap(), saved);
    state.reload(&storage, &paths).await.unwrap();
    assert_eq!(state.encode().unwrap(), saved);
    assert_eq!(state.valid_config().read_priorities, ["cpu", "byte"]); // Support survives reload.
    assert_eq!(restored.valid_config().read_priorities, ["byte", "key"]); // Support isn't persisted.
    for input in [
        br#"{"pending-weight":0}"#.as_slice(),
        br#"{"read-priorities":["cpu"]}"#,
        br#"{"history-sample-interval":"bad"}"#,
        br#"{"enable-for-tiflash":false}"#,
        br#"{"enable-for-tiflash":"bad"}"#,
        br#"{"max-peer-number":1.5}"#,
        br#"{"pending-weight":1e400}"#,
        br#"{"pending-weight":2} trailing"#,
        b"{",
        b"[]",
        b"{}",
        br#"{"unknown":true}"#,
    ] {
        assert!(
            state.update_json(input, &storage, &paths).await.is_err(),
            "{:?}",
            input
        );
        assert_eq!(state.encode().unwrap(), saved);
        assert_eq!(
            storage
                .load_scheduler_config(&paths, HOT_REGION_CONFIG_NAME)
                .await
                .unwrap(),
            saved
        );
    }
    // Unknown and null scalar fields are ignored if a known unchanged field exists.
    assert!(matches!(
        state
            .update_json(br#"{"unknown":4,"pending-weight":null}"#, &storage, &paths)
            .await
            .unwrap(),
        HotConfigUpdate::Unchanged
    ));
    assert!(matches!(
        state
            .update_json(
                br#"{"unknown":4,"pending-weight":4,"read-priorities":null}"#,
                &storage,
                &paths
            )
            .await
            .unwrap(),
        HotConfigUpdate::Updated {
            persistence_error: None
        }
    ));
    assert!(state.params().read_priorities.is_empty());
    assert_eq!(state.valid_config().read_priorities, ["cpu", "byte"]);
    // Public reload overlays the active config; absent fields retain the active values.
    storage
        .save_scheduler_config(
            &paths,
            HOT_REGION_CONFIG_NAME,
            br#"{"pending-weight":5,"disabled":true}"#,
        )
        .await
        .unwrap();
    state.reload(&storage, &paths).await.unwrap();
    assert_eq!(state.params().pending_weight, 5.);
    assert_eq!(
        state.params().history_sample_interval.as_nanos(),
        1_500_000_000
    );
    assert!(!state.disabled()); // ReloadConfig leaves the base disabled flag alone.
    assert!(
        state
            .update_json(br#"{"PENDING-WEIGHT":5}"#, &storage, &paths)
            .await
            .is_err()
    );
    state
        .update_json(br#"{"PENDING-WEIGHT":6}"#, &storage, &paths)
        .await
        .unwrap();
    assert_eq!(state.params().pending_weight, 6.);
    let before = state.encode().unwrap();
    for bytes in [
        b"{".as_slice(),
        br#"{"split-thresholds":0}"#,
        br#"{"rank-formula-version":"v9"}"#,
        br#"{"write-peer-priorities":["query","byte"]}"#,
    ] {
        storage
            .save_scheduler_config(&paths, HOT_REGION_CONFIG_NAME, bytes)
            .await
            .unwrap();
        assert!(state.reload(&storage, &paths).await.is_err());
        assert_eq!(state.encode().unwrap(), before);
    }
    // Typed entry rejects non-finite floats before publication or persistence.
    let mut bad = state.params().clone();
    bad.min_hot_cpu_rate = f64::NAN;
    assert!(state.update(bad, &storage, &paths).await.is_err());
    assert_eq!(state.encode().unwrap(), before);
    println!(
        "task_085: actual MemoryStore update/rebuild, 12 bad patches, 4 bad reloads and read/write faults passed"
    );
}

#[tokio::test]
async fn task_085_cancelled_io_keeps_published_config() {
    use std::future::Future;
    use std::task::{Context, Poll, Waker};
    let backend = Arc::new(FaultStore::default());
    let storage = StorageEndpoint::new(backend.clone());
    let paths = ClusterPaths::new(850);
    let mut state = HotRegionConfig::default();
    state
        .update_json(br#"{"pending-weight":2}"#, &storage, &paths)
        .await
        .unwrap();
    backend.stall.store(true, Ordering::SeqCst);
    let mut update = Box::pin(state.update_json(br#"{"pending-weight":3}"#, &storage, &paths));
    assert!(matches!(
        update
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Pending
    ));
    drop(update);
    assert_eq!(state.params().pending_weight, 3.);
    backend.stall.store(false, Ordering::SeqCst);
    state.reload(&storage, &paths).await.unwrap();
    assert_eq!(state.params().pending_weight, 2.);
}
