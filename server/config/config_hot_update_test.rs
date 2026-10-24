// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::*;
use async_trait::async_trait;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::Semaphore;

use super::config_persist_test::fixture;

// Only gate/fail the real backend's write boundary; all config calculations and
// successful reads/transactions use the canonical backend without replacement.
struct Gate {
    inner: Arc<dyn MetaStore>,
    armed: AtomicBool,
    fail: AtomicBool,
    entered: Semaphore,
    release: Semaphore,
}
impl Gate {
    fn new(inner: Arc<dyn MetaStore>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            armed: AtomicBool::new(false),
            fail: AtomicBool::new(false),
            entered: Semaphore::new(0),
            release: Semaphore::new(0),
        })
    }
}
#[async_trait]
impl MetaStore for Gate {
    async fn read(&self, r: Read) -> Result<Snapshot> {
        self.inner.read(r).await
    }
    async fn transact(&self, c: Vec<Compare>, w: Vec<Mutation>) -> Result<Commit> {
        if self.armed.swap(false, Ordering::SeqCst) {
            self.entered.add_permits(1);
            self.release.acquire().await.unwrap().forget();
            if self.fail.load(Ordering::SeqCst) {
                return Err(StoreError::Unavailable);
            }
        }
        self.inner.transact(c, w).await
    }
    async fn transact_branches(
        &self,
        c: Vec<Compare>,
        t: Vec<Mutation>,
        e: Vec<Mutation>,
    ) -> Result<Commit> {
        self.inner.transact_branches(c, t, e).await
    }
    async fn transact_raw(
        &self,
        c: Vec<Compare>,
        t: Vec<RawTxnOp>,
        e: Vec<RawTxnOp>,
    ) -> Result<RawTxnResponse> {
        self.inner.transact_raw(c, t, e).await
    }
    async fn watch(&self, p: Vec<u8>, r: Revision) -> Result<Box<dyn MetaWatch>> {
        self.inner.watch(p, r).await
    }
    async fn grant(&self, t: i64) -> Result<Lease> {
        self.inner.grant(t).await
    }
    async fn keep_alive(&self, l: LeaseId) -> Result<Lease> {
        self.inner.keep_alive(l).await
    }
    async fn revoke(&self, l: LeaseId) -> Result<()> {
        self.inner.revoke(l).await
    }
    async fn ttl(&self, l: LeaseId) -> Result<i64> {
        self.inner.ttl(l).await
    }
    async fn compact(&self, r: Revision) -> Result<()> {
        self.inner.compact(r).await
    }
}
fn host(store: Arc<dyn MetaStore>) -> Arc<HotConfig> {
    Arc::new(HotConfig::new(
        PersistedConfig::default(),
        StorageEndpoint::new(store),
        ClusterPaths::new(13),
    ))
}
async fn stored(store: Arc<dyn MetaStore>) -> PersistedConfig {
    StorageEndpoint::new(store)
        .load_config(&ClusterPaths::new(13))
        .await
        .unwrap()
        .unwrap()
}
async fn contract(store: Arc<dyn MetaStore>) {
    let gate = Gate::new(store.clone());
    let owner = host(gate.clone());
    owner
        .update_schedule(|_, n| {
            n.max_snapshot_count = 99;
            n.max_pending_peer_count = 88;
            n.leader_schedule_limit = 7;
            n.region_schedule_limit = 13;
            Ok(true)
        })
        .await
        .unwrap();
    let oracle: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule/config/task13/go.out"
    ))
    .unwrap();
    let saved = serde_json::to_value(stored(store.clone()).await.config.schedule).unwrap();
    for (key, value) in oracle["concurrent_active_and_stored"].as_object().unwrap() {
        assert_eq!(&saved[key], value);
    }
    let old = owner.snapshot();
    gate.armed.store(true, Ordering::SeqCst);
    gate.fail.store(true, Ordering::SeqCst);
    let writer = owner.clone();
    let pending = tokio::spawn(async move {
        writer
            .update_schedule(|_, n| {
                n.max_snapshot_count = 100;
                Ok(true)
            })
            .await
    });
    gate.entered.acquire().await.unwrap().forget();
    let candidate = owner.snapshot();
    assert_eq!(candidate.config.schedule.max_snapshot_count, 100);
    assert_eq!(
        stored(store.clone()).await.config.schedule,
        old.config.schedule
    );
    gate.release.add_permits(1);
    assert!(pending.await.unwrap().is_err());
    assert!(
        Arc::ptr_eq(&owner.snapshot(), &old),
        "failed persistence must restore the original generation"
    );
    assert_eq!(candidate.config.schedule.max_snapshot_count, 100); // captured reader stays immutable
    assert_eq!(
        stored(store.clone()).await.config.schedule,
        old.config.schedule
    );
    owner
        .update_schedule(|_, n| {
            n.max_snapshot_count = 101;
            Ok(true)
        })
        .await
        .unwrap();
    assert_eq!(stored(store).await.config.schedule.max_snapshot_count, 101);
}
#[tokio::test]
async fn config_hot_update_memory_publication_and_rollback() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn config_hot_update_etcd_publication_and_rollback() {
    let f = fixture::Fixture::start();
    contract(Arc::new(f.store().await)).await;
}

async fn concurrent_contract(store: Arc<dyn MetaStore>) {
    let owner = host(store.clone());
    let mut tasks = Vec::new();
    for _ in 0..32 {
        let o = owner.clone();
        tasks.push(tokio::spawn(async move {
            o.update_schedule(|c, n| {
                n.max_snapshot_count = c.max_snapshot_count + 1;
                Ok(true)
            })
            .await
            .unwrap()
        }));
        let o = owner.clone();
        tasks.push(tokio::spawn(async move {
            o.mutate_schedule(|n| n.max_pending_peer_count += 1).await
        }));
        let o = owner.clone();
        tasks.push(tokio::spawn(async move { o.persist().await.unwrap() }));
    }
    for task in tasks {
        task.await.unwrap();
    }
    owner.persist().await.unwrap();
    let active = owner.snapshot();
    assert_eq!(active.config.schedule.max_snapshot_count, 32);
    assert_eq!(active.config.schedule.max_pending_peer_count, 32);
    assert_eq!(stored(store).await.config.schedule, active.config.schedule);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn config_hot_update_memory_concurrent_read_modify_persist() {
    concurrent_contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn config_hot_update_etcd_concurrent_read_modify_persist() {
    let f = fixture::Fixture::start();
    concurrent_contract(Arc::new(f.store().await)).await;
}

#[tokio::test]
async fn config_hot_update_noop_error_and_clone_isolation() {
    let store = Arc::new(MemoryStore::default());
    let owner = host(store.clone());
    owner.persist().await.unwrap();
    let old = owner.snapshot();
    owner
        .update_schedule(|_, n| {
            n.max_snapshot_count = 17;
            Ok(false)
        })
        .await
        .unwrap();
    assert!(Arc::ptr_eq(&old, &owner.snapshot()));
    assert!(
        owner
            .update_schedule(|_, n| {
                n.max_snapshot_count = 18;
                Err(ConfigPersistenceError::Invalid("callback rejected".into()))
            })
            .await
            .is_err()
    );
    assert!(Arc::ptr_eq(&old, &owner.snapshot()));
    assert_eq!(stored(store).await.config.schedule, old.config.schedule);
}

#[tokio::test]
async fn config_hot_update_cancel_restores_before_next_writer() {
    let store = Arc::new(MemoryStore::default());
    let gate = Gate::new(store.clone());
    let owner = host(gate.clone());
    owner.persist().await.unwrap();
    let old = owner.snapshot();
    gate.armed.store(true, Ordering::SeqCst);
    let o = owner.clone();
    let pending = tokio::spawn(async move {
        o.update_schedule(|_, n| {
            n.max_snapshot_count = 100;
            Ok(true)
        })
        .await
    });
    gate.entered.acquire().await.unwrap().forget();
    assert_eq!(owner.snapshot().config.schedule.max_snapshot_count, 100);
    pending.abort();
    assert!(pending.await.unwrap_err().is_cancelled());
    assert!(Arc::ptr_eq(&old, &owner.snapshot()));
    owner
        .update_schedule(|c, n| {
            n.max_snapshot_count = c.max_snapshot_count + 1;
            Ok(true)
        })
        .await
        .unwrap();
    assert_eq!(stored(store).await.config.schedule.max_snapshot_count, 1);
}

#[tokio::test]
async fn config_hot_update_queued_writer_and_persist_wait_for_rollback() {
    let store = Arc::new(MemoryStore::default());
    let gate = Gate::new(store.clone());
    let owner = host(gate.clone());
    owner.persist().await.unwrap();
    gate.armed.store(true, Ordering::SeqCst);
    gate.fail.store(true, Ordering::SeqCst);
    let o = owner.clone();
    let first = tokio::spawn(async move {
        o.update_schedule(|_, n| {
            n.max_snapshot_count = 100;
            Ok(true)
        })
        .await
    });
    gate.entered.acquire().await.unwrap().forget();
    let called = Arc::new(AtomicBool::new(false));
    let seen = called.clone();
    let o = owner.clone();
    let mut second = Box::pin(o.update_schedule(move |c, n| {
        seen.store(true, Ordering::SeqCst);
        assert_eq!(c.max_snapshot_count, 0);
        n.max_pending_peer_count = 88;
        Ok(true)
    }));
    let mut persist = Box::pin(owner.persist());
    // Poll each future exactly once while the first I/O is paused. No sleep or
    // scheduling guess: both must be pending and the second callback unexecuted.
    for future in [
        &mut second
            as &mut (
                     dyn std::future::Future<
                Output = std::result::Result<(), ConfigPersistenceError>,
            > + Unpin
                 ),
        &mut persist,
    ] {
        std::future::poll_fn(|cx| {
            assert!(std::pin::Pin::new(&mut *future).poll(cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
    }
    assert!(!called.load(Ordering::SeqCst));
    gate.release.add_permits(1);
    assert!(first.await.unwrap().is_err());
    second.await.unwrap();
    persist.await.unwrap();
    let saved = stored(store).await;
    assert_eq!(saved.config.schedule.max_snapshot_count, 0);
    assert_eq!(saved.config.schedule.max_pending_peer_count, 88);
}
