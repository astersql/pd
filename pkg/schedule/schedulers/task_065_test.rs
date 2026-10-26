// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_storage_kv::{
    Commit, Compare, Lease, MetaWatch, Mutation, RawTxnOp, RawTxnResponse, Read, Snapshot,
    StoreError,
};

use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_types::*;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use serde::{Serialize, Serializer, ser::Error};
use serde_json::{Value, json};
use std::sync::Arc;

#[test]
fn task_065_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_065.json"
    ))
    .unwrap();
    let cases = fixture["intervals"].as_array().unwrap();
    for c in cases {
        let growth = match c["growth"].as_str().unwrap() {
            "exponential" => IntervalGrowth::Exponential,
            "linear" => IntervalGrowth::Linear,
            "zero" => IntervalGrowth::Zero,
            _ => unreachable!(),
        };
        assert_eq!(
            interval_grow(
                c["input"].as_i64().unwrap(),
                c["max"].as_i64().unwrap(),
                growth
            ),
            c["expected"].as_i64().unwrap(),
            "{c}"
        );
    }
    println!("{} frozen interval cases passed", cases.len());
}

#[test]
fn task_065_codec_and_payload() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_065.json"
    ))
    .unwrap();
    for c in fixture["codec"].as_array().unwrap() {
        let bytes = encode_config(&c["value"]).unwrap();
        assert_eq!(
            String::from_utf8(bytes.clone()).unwrap(),
            c["encoded"].as_str().unwrap()
        );
        assert_eq!(decode_config::<Value>(&bytes).unwrap(), c["value"]);
    }
    for invalid in ["", "{", "true false", "[1,]", "NaN"] {
        assert!(matches!(
            decode_config::<Value>(invalid.as_bytes()),
            Err(SchedulerError::Decode(_))
        ));
    }
    assert!(matches!(
        decode_config::<bool>(b"42"),
        Err(SchedulerError::Decode(_))
    ));
    assert_eq!(
        ConfigJsonDecoder::new(b"true".to_vec())
            .decode::<bool>()
            .unwrap(),
        true
    );
    assert!(matches!(
        ConfigJsonDecoder::new(b"bad".to_vec()).decode::<bool>(),
        Err(SchedulerError::Decode(_))
    ));
    let rounded = to_payload(
        &["n".into()],
        &["[9007199254740993,{\"x\":9007199254740993}]".into()],
    )
    .unwrap();
    assert_eq!(
        rounded["n"],
        json!([9007199254740992.0, {"x":9007199254740992.0}])
    );
    struct Fails;
    impl Serialize for Fails {
        fn serialize<S: Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(S::Error::custom("encoding failure"))
        }
    }
    assert!(matches!(
        encode_config(&Fails),
        Err(SchedulerError::Encode(_))
    ));
    let names = ["a", "b", "a", "a", "null"].map(String::from);
    let configs = ["1", "{", "{\"x\":2}", "bad", "null"].map(String::from);
    let payload = to_payload(&names, &configs).unwrap();
    assert_eq!(
        serde_json::to_value(payload).unwrap(),
        json!({"a":{"x":2.0},"null":null})
    );
    assert!(matches!(
        to_payload(&names, &[]),
        Err(SchedulerError::PayloadLength)
    ));
    assert!(to_payload(&[], &configs).unwrap().is_empty());
}

#[tokio::test]
async fn task_065_base_default_and_disable() {
    let storage = Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default())));
    let paths = ClusterPaths::new(65);
    let mut plain = BaseScheduler::new(SHUFFLE_LEADER_SCHEDULER, None);
    assert_eq!(plain.name(), "shuffle-leader-scheduler");
    assert_eq!(plain.scheduler_type(), &SHUFFLE_LEADER_SCHEDULER);
    assert_eq!(plain.min_interval(), 10_000_000);
    assert_eq!(plain.next_interval(10_000_000), 13_000_000);
    assert_eq!(plain.encode_config().unwrap(), b"null");
    plain.reload_config().unwrap();
    plain.set_disabled(true).await.unwrap();
    assert!(!plain.is_disabled().await);
    plain.set_name("custom".into());
    assert_eq!(plain.name(), "custom");
    plain.set_name(String::new());
    assert_eq!(plain.name(), "shuffle-leader-scheduler");
    for typ in SCHEDULERS {
        let base = BaseScheduler::new(typ.clone(), None);
        assert_eq!(
            base.is_default_in(&default_schedulers(false)),
            [
                BALANCE_LEADER_SCHEDULER,
                BALANCE_REGION_SCHEDULER,
                BALANCE_HOT_REGION_SCHEDULER,
                EVICT_SLOW_STORE_SCHEDULER,
                EVICT_STOPPING_STORE_SCHEDULER
            ]
            .contains(&typ)
        );
        assert_eq!(
            base.is_default_in(&default_schedulers(true)),
            [
                BALANCE_LEADER_SCHEDULER,
                BALANCE_REGION_SCHEDULER,
                BALANCE_HOT_REGION_SCHEDULER,
                EVICT_SLOW_STORE_SCHEDULER
            ]
            .contains(&typ)
        );
    }
    let name = "balance-leader-scheduler";
    let config = DefaultSchedulerConfig::new(
        name.into(),
        storage.clone(),
        paths.clone(),
        json!({"disabled":false,"batch":4}),
    );
    let mut base = BaseScheduler::new(BALANCE_LEADER_SCHEDULER, Some(config));
    assert!(!base.is_disabled().await); // missing storage retains false
    base.set_disabled(true).await.unwrap();
    assert_eq!(
        decode_config::<Value>(&storage.load_scheduler_config(&paths, name).await.unwrap())
            .unwrap(),
        json!({"disabled":true,"batch":4})
    );
    assert!(base.is_disabled().await);
    for bytes in [
        b"{bad".as_slice(),
        b"{}",
        b"{\"disabled\":null}",
        b"{\"disabled\":42}",
    ] {
        storage
            .save_scheduler_config(&paths, name, bytes)
            .await
            .unwrap();
        assert!(base.is_disabled().await);
    }
    storage
        .save_scheduler_config(&paths, name, b"{\"disabled\":false}")
        .await
        .unwrap();
    assert!(!base.is_disabled().await);
    base.set_disabled(false).await.unwrap();
    assert!(!base.is_disabled().await);
}

#[test]
fn task_065_unregistered_and_unsupported() {
    let registry = SchedulerRegistry::default();
    for typ in SCHEDULERS
        .into_iter()
        .chain([CheckerSchedulerType::from("unknown")])
    {
        assert!(matches!(registry.lookup(&typ), Err(SchedulerError::NotRegistered(t)) if t == typ));
        assert!(
            matches!(registry.decode_slice::<Value>(&typ, &[]), Err(SchedulerError::DecoderNotRegistered(t)) if t == typ)
        );
        assert_eq!(
            registry.find_type_by_name(typ.as_str()),
            if typ == SCATTER_RANGE_SCHEDULER {
                Some(SCATTER_RANGE_SCHEDULER)
            } else {
                None
            }
        );
    }
    assert_eq!(
        registry.find_type_by_name("old-scatter-range-1"),
        Some(SCATTER_RANGE_SCHEDULER)
    );
    let mut base = BaseScheduler::new(BALANCE_LEADER_SCHEDULER, None);
    assert!(matches!(
        base.update_config(b"{}"),
        Err(SchedulerError::Unsupported("update config"))
    ));
}

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
async fn task_065_persistence_failure_retains_local_state() {
    let backend = Arc::new(FaultStore::default());
    let storage = Arc::new(StorageEndpoint::new(backend.clone()));
    let paths = ClusterPaths::new(65);
    let mut config = DefaultSchedulerConfig::new(
        "fault".into(),
        storage.clone(),
        paths.clone(),
        json!({"disabled":false}),
    );
    config.set_disabled(false).await.unwrap();
    backend
        .fail
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(matches!(
        config.set_disabled(true).await,
        Err(SchedulerError::Storage(_))
    ));
    assert!(config.is_disabled().await); // Failed read retains the failed write's local state.
    backend
        .fail
        .store(false, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        decode_config::<Value>(
            &storage
                .load_scheduler_config(&paths, "fault")
                .await
                .unwrap()
        )
        .unwrap(),
        json!({"disabled":false})
    );
    assert!(!config.is_disabled().await); // Successful read refreshes from actual persisted data.
}
