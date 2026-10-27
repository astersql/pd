// Copyright 2026 AsterSQL.
//! Acceptance composes existing policies; it does not implement scheduling algorithms.
use astersql_pd_core::{BasicCluster, StoreInfo, StoreMeta};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_schedulers::*;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::*;
use serde_json::Value;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_156.json"
    ))
    .unwrap()
}
fn cases() -> Value {
    // Frozen before task156; expectations were derived from Go, never from this harness.
    serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_106.json"
    ))
    .unwrap()
}
fn registry() -> SchedulerRegistry {
    let mut r = SchedulerRegistry::default();
    r.register_builtins().unwrap();
    r
}
fn context(backend: Arc<dyn MetaStore>) -> FactoryContext {
    FactoryContext::new(
        Arc::new(StorageEndpoint::new(backend)),
        ClusterPaths::new(156),
        JobTime(0),
    )
}
fn create(r: &SchedulerRegistry, case: &Value, ctx: &FactoryContext) -> RegisteredScheduler {
    let args: Vec<_> = case["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap())
        .collect();
    r.create_args(&case["type"].as_str().unwrap().into(), &args, ctx)
        .unwrap()
}

#[tokio::test]
async fn task_156_behavior_cases() {
    let r = registry();
    let backend = Arc::new(MemoryStore::default());
    let ctx = context(backend.clone());
    let restored_ctx = context(backend);
    let all = cases();
    let expected_types: std::collections::BTreeSet<_> = all["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["type"].as_str().unwrap())
        .collect();
    assert_eq!(
        r.registered_types()
            .map(|t| t.as_str())
            .collect::<std::collections::BTreeSet<_>>(),
        expected_types
    );
    let mut checked = 0;
    for case in all["cases"].as_array().unwrap() {
        let s = create(&r, case, &ctx);
        assert_eq!(s.name(), case["name"].as_str().unwrap());
        assert_eq!(s.scheduler_type().as_str(), case["type"].as_str().unwrap());
        let bytes = s.encode_config().unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        for (key, expected) in case["expected"].as_object().unwrap() {
            assert_eq!(&value[key], expected, "{} {key}", s.name());
        }
        if case["expected_null"] == true {
            assert_eq!(value, Value::Null);
        }
        if let Some(job) = case["job_expected"].as_object() {
            assert_eq!(value.as_array().unwrap().len(), 1);
            for (key, expected) in job {
                assert_eq!(&value[0][key], expected);
            }
        }
        s.save_config(&ctx).await.unwrap();
        // Recreate registry and endpoint while retaining the actual backend state.
        let restored = registry().restore(&s.name(), &restored_ctx).await.unwrap();
        assert_eq!(restored.scheduler_type(), s.scheduler_type());
        assert_eq!(restored.encode_config().unwrap(), bytes);
        ctx.storage
            .save_scheduler_config(
                &ctx.paths,
                &s.name(),
                fixture()["corrupt_config"].as_str().unwrap().as_bytes(),
            )
            .await
            .unwrap();
        let corrupt_restore = registry().restore(&s.name(), &restored_ctx).await;
        if fixture()["decoder_ignored_types"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == &case["type"])
        {
            assert_eq!(corrupt_restore.unwrap().encode_config().unwrap(), b"null");
        } else {
            assert!(corrupt_restore.is_err(), "corrupt config {}", s.name());
        }
        s.save_config(&ctx).await.unwrap();
        assert_eq!(
            registry()
                .restore(&s.name(), &ctx)
                .await
                .unwrap()
                .encode_config()
                .unwrap(),
            bytes
        );
        checked += 1;
    }
    assert_eq!(checked, fixture()["factories"].as_u64().unwrap());
    println!("TASK156 config: {checked} factories, defaults, roundtrip, corrupt restore, repair");
}

#[tokio::test]
async fn task_156_pause_resume_all_factories() {
    let f = fixture();
    let at = f["pause"]["at"].as_i64().unwrap();
    let ttl = f["pause"]["ttl"].as_i64().unwrap();
    let expiry = f["pause"]["expires"].as_i64().unwrap();
    for case in cases()["cases"].as_array().unwrap() {
        // Separate real state per policy avoids cross-policy pause references.
        let cluster = Arc::new(BasicCluster::new());
        for id in 1..=3 {
            cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
                id,
                ..Default::default()
            })));
        }
        let ctx = context(Arc::new(MemoryStore::default()));
        let s = create(&registry(), case, &ctx);
        let name = s.name();
        let mut controller = SchedulerController::new(Some(cluster), ctx, serde_json::json!({}));
        controller.add(s, &[], false).await.unwrap();
        assert!(!controller.is_paused(&name, at).unwrap());
        controller.pause_or_resume(&name, ttl, at).unwrap();
        assert_eq!(controller.delay(&name, at).unwrap(), (at, expiry));
        assert!(controller.is_paused(&name, expiry - 1).unwrap());
        assert!(!controller.is_paused(&name, expiry).unwrap());
        controller.pause_or_resume("all", ttl, at).unwrap();
        assert!(controller.is_paused(&name, at).unwrap());
        controller.pause_or_resume(&name, 0, at).unwrap();
        assert!(!controller.is_paused(&name, at).unwrap());
        controller.remove(&name, false).await.unwrap();
        assert!(controller.names().is_empty());
        assert!(controller.pause_or_resume(&name, ttl, at).is_err());
    }
    println!("TASK156 lifecycle: 18 add/pause/expiry/resume/remove cases");
}

#[derive(Default)]
struct FaultStore {
    store: MemoryStore,
    fail: AtomicBool,
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
async fn task_156_io_faults_and_invalid_configuration() {
    let r = registry();
    let backend = Arc::new(FaultStore::default());
    let ctx = context(backend.clone());
    for case in cases()["cases"].as_array().unwrap() {
        let s = create(&r, case, &ctx);
        let name = s.name();
        let bytes = s.encode_config().unwrap();
        s.save_config(&ctx).await.unwrap();
        backend.fail.store(true, Ordering::SeqCst);
        assert!(s.save_config(&ctx).await.is_err(), "failed write {name}");
        assert!(r.restore(&name, &ctx).await.is_err(), "failed read {name}");
        backend.fail.store(false, Ordering::SeqCst);
        assert_eq!(
            r.restore(&name, &ctx)
                .await
                .unwrap()
                .encode_config()
                .unwrap(),
            bytes,
            "old committed value survives {name}"
        );
        s.save_config(&ctx).await.unwrap();
        assert_eq!(
            r.restore(&name, &ctx)
                .await
                .unwrap()
                .encode_config()
                .unwrap(),
            bytes
        );
    }
    let all = cases();
    let invalid = all["invalid"].as_array().unwrap();
    assert_eq!(
        invalid.len() as u64,
        fixture()["expected_invalid_args"].as_u64().unwrap()
    );
    for c in invalid {
        let args: Vec<_> = c[1]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(
            r.create_args(&c[0].as_str().unwrap().into(), &args, &ctx)
                .is_err()
        );
    }
    assert!(r.restore("unknown-scheduler", &ctx).await.is_err());
    println!(
        "TASK156 faults: 18 read + 18 write failures, old value + retry; 8 invalid args, unknown name"
    );
}

#[tokio::test]
async fn task_156_storage_adapter_shares_neutral_backend() {
    use astersql_pd_proto::pdpb::RequestHeader;
    use astersql_pd_storage_domain as domain;
    use astersql_pd_storage_endpoint::protobuf;
    // The legacy facade and clean domain expose the exact same canonical KV types.
    assert_eq!(
        std::any::TypeId::of::<MemoryStore>(),
        std::any::TypeId::of::<domain::MemoryStore>()
    );
    let backend = Arc::new(MemoryStore::default());
    let adapter = Arc::new(StorageEndpoint::new(backend.clone()));
    let neutral: &domain::StorageEndpoint = adapter.as_ref().as_ref();
    let paths = ClusterPaths::new(156);
    let payload = br#"{"batch":4}"#;
    neutral
        .save_scheduler_config(&paths, "boundary", payload)
        .await
        .unwrap();
    assert_eq!(
        adapter
            .load_scheduler_config(&paths, "boundary")
            .await
            .unwrap(),
        payload
    );
    let host = FactoryContext::new(adapter.clone(), paths, JobTime(0));
    assert_eq!(
        host.storage
            .load_scheduler_config(&host.paths, "boundary")
            .await
            .unwrap(),
        payload
    );

    // Existing frozen Go wire fixture; expectations are not encoded by the tested Rust codec.
    let fixture: Value =
        serde_json::from_str(include_str!("../../pkg/proto/wire-fixtures.json")).unwrap();
    let hex = fixture["request_header"].as_str().unwrap();
    let wire: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    let header = RequestHeader {
        cluster_id: 42,
        sender_id: 7,
        caller_id: "oracle".into(),
        caller_component: "pd".into(),
    };
    protobuf::save(&adapter, b"/boundary/header", &header)
        .await
        .unwrap();
    assert_eq!(
        neutral.load(b"/boundary/header").await.unwrap(),
        domain::LoadState::Value(wire)
    );
    assert_eq!(
        protobuf::load::<RequestHeader>(&adapter, b"/boundary/header")
            .await
            .unwrap(),
        Some(header)
    );
    Base::new(backend.as_ref())
        .save(b"/boundary/header", &[0x80])
        .await
        .unwrap();
    let error = protobuf::load::<RequestHeader>(&adapter, b"/boundary/header")
        .await
        .unwrap_err();
    assert!(error.to_string().starts_with("proto unmarshal:"));
    let domain::EndpointError::ProtoUnmarshal(source) = error else {
        panic!("codec category lost")
    };
    assert!(source.downcast_ref::<prost::DecodeError>().is_some());
    println!(
        "TASK156 storage boundary: shared KV identity, config handle, frozen Go wire, typed codec failure"
    );
}
