// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::StorageEndpoint;
use astersql_pd_storage_domain::{
    Commit, Compare, Lease, MemoryStore, MetaWatch, Mutation, RawTxnOp, RawTxnResponse, Read,
    Snapshot, StoreError,
};
use serde::{Deserialize, Serialize, Serializer, ser::Error};
use serde_json::{Value, json};
use std::sync::{Arc, atomic::Ordering};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    value: String,
}
fn params(s: &str) -> Params {
    Params { value: s.into() }
}
fn setup() -> (Arc<FaultStore>, Arc<StorageEndpoint>, ClusterPaths) {
    let backend = Arc::new(FaultStore::default());
    (
        backend.clone(),
        Arc::new(StorageEndpoint::new(backend)),
        ClusterPaths::new(66),
    )
}
#[tokio::test]
async fn task_066_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_066.json"
    ))
    .unwrap();
    let (_, storage, paths) = setup();
    let cases = fixture["cases"].as_array().unwrap();
    for c in cases {
        let initial: Params = serde_json::from_value(c["value"].clone()).unwrap();
        let updated: Params = serde_json::from_value(c["updated"].clone()).unwrap();
        let mut config = SchedulerConfig::new(
            c["name"].as_str().unwrap().into(),
            storage.clone(),
            paths.clone(),
            initial.clone(),
        );
        config.save().await.unwrap();
        let mut restored = SchedulerConfig::new(
            c["name"].as_str().unwrap().into(),
            storage.clone(),
            paths.clone(),
            params("local"),
        );
        restored.reload().await.unwrap();
        assert_eq!(restored.get(), &initial);
        config.update(updated.clone()).await.unwrap();
        assert_eq!(config.get(), &updated);
        restored.reload().await.unwrap();
        assert_eq!(restored.get(), &updated);
    }
    let mut unknown = SchedulerConfig::new("unknown".into(), storage, paths, params("old"));
    assert!(matches!(
        unknown.reload().await,
        Err(SchedulerError::Decode(_))
    ));
    assert_eq!(unknown.get(), &params("old"));
    println!(
        "{} frozen save/load/update/reinstantiation cases passed; unknown name returns decode error",
        cases.len()
    );
}

#[tokio::test]
async fn task_066_failures_retain_old_value() {
    let (backend, storage, paths) = setup();
    let mut config = SchedulerConfig::new(
        "fault".into(),
        storage.clone(),
        paths.clone(),
        params("old"),
    );
    config.save().await.unwrap();
    backend.fail.store(true, Ordering::SeqCst);
    assert!(matches!(
        config.update(params("new")).await,
        Err(SchedulerError::Storage(_))
    ));
    assert_eq!(
        config.get(),
        &params("old"),
        "failed persistence must not publish candidate"
    );
    assert!(matches!(
        config.reload().await,
        Err(SchedulerError::Storage(_))
    ));
    assert_eq!(config.get(), &params("old"));
    backend.fail.store(false, Ordering::SeqCst);
    assert_eq!(
        decode_config::<Params>(
            &storage
                .load_scheduler_config(&paths, "fault")
                .await
                .unwrap()
        )
        .unwrap(),
        params("old")
    );
    for data in [
        b"".as_slice(),
        b"{",
        b"null",
        b"{\"value\":1}",
        b"{\"unknown\":true}",
    ] {
        assert!(matches!(
            config.update_json(data).await,
            Err(SchedulerError::Decode(_))
        ));
        assert_eq!(config.get(), &params("old"));
        storage
            .save_scheduler_config(&paths, "fault", data)
            .await
            .unwrap();
        assert!(matches!(
            config.reload().await,
            Err(SchedulerError::Decode(_))
        ));
        assert_eq!(config.get(), &params("old"));
    }
    config.update_json(br#"{"value":"retry"}"#).await.unwrap();
    assert_eq!(config.get(), &params("retry"));
    config.reload().await.unwrap();
    assert_eq!(config.get(), &params("retry"));
}

#[tokio::test]
async fn task_066_cancel_before_commit() {
    let (backend, storage, paths) = setup();
    let mut config = SchedulerConfig::new(
        "cancel".into(),
        storage.clone(),
        paths.clone(),
        params("old"),
    );
    config.save().await.unwrap();
    backend.pause.store(true, Ordering::SeqCst);
    let mut pending = Box::pin(config.update(params("new")));
    tokio::select! {
        _ = backend.entered.notified() => {},
        result = &mut pending => panic!("write should pause: {result:?}"),
    }
    assert_eq!(
        decode_config::<Params>(
            &storage
                .load_scheduler_config(&paths, "cancel")
                .await
                .unwrap()
        )
        .unwrap(),
        params("old")
    );
    drop(pending);
    assert_eq!(config.get(), &params("old"));
    backend.pause.store(false, Ordering::SeqCst);
    config.update(params("new")).await.unwrap();
    assert_eq!(config.get(), &params("new"));
}

#[tokio::test]
async fn task_066_encode_failure() {
    #[derive(Clone)]
    struct Fails(bool);
    impl Serialize for Fails {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            if self.0 {
                Err(S::Error::custom("injected encoding failure"))
            } else {
                s.serialize_bool(false)
            }
        }
    }
    let (_, storage, paths) = setup();
    let mut config = SchedulerConfig::new(
        "encode".into(),
        storage.clone(),
        paths.clone(),
        Fails(false),
    );
    config.save().await.unwrap();
    assert!(matches!(
        config.update(Fails(true)).await,
        Err(SchedulerError::Encode(_))
    ));
    assert!(!config.get().0);
    assert_eq!(
        storage
            .load_scheduler_config(&paths, "encode")
            .await
            .unwrap(),
        b"false"
    );
}

#[tokio::test]
async fn task_066_default_disable_restores() {
    let (_, storage, paths) = setup();
    let mut config = DefaultSchedulerConfig::new(
        "default".into(),
        storage.clone(),
        paths.clone(),
        json!({"value":"test"}),
    );
    assert!(!config.is_disabled().await);
    config.set_disabled(true).await.unwrap();
    let mut restored = DefaultSchedulerConfig::new(
        "default".into(),
        storage.clone(),
        paths.clone(),
        json!({"value":"local"}),
    );
    assert!(restored.is_disabled().await);
    assert_eq!(
        decode_config::<Value>(
            &storage
                .load_scheduler_config(&paths, "default")
                .await
                .unwrap()
        )
        .unwrap(),
        json!({"disabled":true,"value":"test"})
    );
    restored.set_disabled(false).await.unwrap();
    assert!(!config.is_disabled().await);
}

/// Only the I/O boundary is faulted; normal operations use the canonical backend.
#[derive(Default)]
struct FaultStore {
    store: MemoryStore,
    fail: std::sync::atomic::AtomicBool,
    pause: std::sync::atomic::AtomicBool,
    entered: tokio::sync::Notify,
    resume: tokio::sync::Notify,
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
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(StoreError::Unavailable);
        }
        if self.pause.load(std::sync::atomic::Ordering::SeqCst) {
            self.entered.notify_one();
            self.resume.notified().await;
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
