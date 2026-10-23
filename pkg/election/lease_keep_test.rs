// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_storage_kv::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicI64, AtomicU8, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{runtime::Handle, sync::Notify, time::timeout};
use tokio_util::sync::CancellationToken;

struct ActiveRequest<'a>(&'a AtomicUsize);
impl Drop for ActiveRequest<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
// Gate only external IO; every transaction and valid lease response uses a real backend.
struct GateStore {
    inner: Arc<dyn MetaStore>,
    granted_ttl: AtomicI64,
    active: AtomicUsize,
    mode: AtomicU8, // 0 healthy, 1 delayed response, 2 unavailable, 3 invalid TTL
    entered: Notify,
    release: Notify,
    revoke_entered: Notify,
    revoke_release: Notify,
    block_revoke: std::sync::atomic::AtomicBool,
}
#[async_trait::async_trait]
impl MetaStore for GateStore {
    async fn read(&self, request: Read) -> astersql_pd_storage_kv::Result<Snapshot> {
        self.inner.read(request).await
    }
    async fn transact(
        &self,
        compares: Vec<Compare>,
        writes: Vec<Mutation>,
    ) -> astersql_pd_storage_kv::Result<Commit> {
        self.inner.transact(compares, writes).await
    }
    async fn transact_branches(
        &self,
        compares: Vec<Compare>,
        then_writes: Vec<Mutation>,
        else_writes: Vec<Mutation>,
    ) -> astersql_pd_storage_kv::Result<Commit> {
        self.inner
            .transact_branches(compares, then_writes, else_writes)
            .await
    }
    async fn transact_raw(
        &self,
        compares: Vec<Compare>,
        then_ops: Vec<RawTxnOp>,
        else_ops: Vec<RawTxnOp>,
    ) -> astersql_pd_storage_kv::Result<RawTxnResponse> {
        self.inner.transact_raw(compares, then_ops, else_ops).await
    }
    async fn watch(
        &self,
        prefix: Vec<u8>,
        start_revision: Revision,
    ) -> astersql_pd_storage_kv::Result<Box<dyn MetaWatch>> {
        self.inner.watch(prefix, start_revision).await
    }
    async fn keep_alive(&self, id: LeaseId) -> astersql_pd_storage_kv::Result<Lease> {
        self.active.fetch_add(1, Ordering::SeqCst);
        let _active = ActiveRequest(&self.active);
        let mode = self.mode.load(Ordering::SeqCst);
        if mode == 2 {
            self.entered.notify_one();
            return Err(StoreError::Unavailable);
        }
        let mut lease = self.inner.keep_alive(id).await?;
        // Signal only after the real backend has successfully renewed the lease.
        self.entered.notify_one();
        if mode == 1 {
            self.release.notified().await;
        }
        if mode == 3 {
            lease.ttl_seconds = 0;
        }
        Ok(lease)
    }
    async fn revoke(&self, id: LeaseId) -> astersql_pd_storage_kv::Result<()> {
        if self.block_revoke.load(Ordering::SeqCst) {
            self.revoke_entered.notify_one();
            self.revoke_release.notified().await;
        }
        self.inner.revoke(id).await
    }
    async fn ttl(&self, lease: LeaseId) -> astersql_pd_storage_kv::Result<i64> {
        self.inner.ttl(lease).await
    }
    async fn compact(&self, revision: Revision) -> astersql_pd_storage_kv::Result<()> {
        self.inner.compact(revision).await
    }
    async fn grant(&self, ttl: i64) -> astersql_pd_storage_kv::Result<Lease> {
        let lease = self.inner.grant(ttl).await?;
        self.granted_ttl.store(lease.ttl_seconds, Ordering::SeqCst);
        Ok(lease)
    }
}
async fn campaign(store: Arc<dyn MetaStore>, cancel: CancellationToken) -> Arc<Leadership> {
    Arc::new(
        Leadership::campaign_with_key(
            store,
            cancel,
            63,
            b"/keep".to_vec(),
            b"leader".to_vec(),
            1,
            vec![],
        )
        .await
        .unwrap(),
    )
}
async fn joined(keep: LeaseKeep) -> LeaseKeepExit {
    timeout(Duration::from_secs(3), keep.join())
        .await
        .expect("loop must exit and join")
        .unwrap()
}
async fn behavior(backend: Arc<dyn MetaStore>) {
    let io = Arc::new(GateStore {
        inner: backend,
        mode: AtomicU8::new(0),
        granted_ttl: AtomicI64::new(0),
        active: AtomicUsize::new(0),
        entered: Notify::new(),
        release: Notify::new(),
        revoke_entered: Notify::new(),
        revoke_release: Notify::new(),
        block_revoke: false.into(),
    });
    let parent = CancellationToken::new();
    let term = campaign(io.clone(), parent.clone()).await;
    let stop = CancellationToken::new();
    let keep = term.keep(&Handle::current(), stop.clone());
    timeout(Duration::from_secs(2), io.entered.notified())
        .await
        .expect("immediate renewal reaches real backend");
    // Continue across the requested grant TTL: a live loop must really renew.
    tokio::time::sleep(Duration::from_millis(1400)).await;
    assert!(!keep.is_finished());
    assert!(!term.is_expired());
    term.validate().await.unwrap();
    stop.cancel();
    assert_eq!(joined(keep).await, LeaseKeepExit::Cancelled);
    assert!(
        !parent.is_cancelled(),
        "worker cancellation must not cancel host"
    );
    term.validate().await.unwrap(); // stopping keep alone does not revoke
    term.resign().await.unwrap();
    while timeout(Duration::from_millis(1), io.entered.notified())
        .await
        .is_ok()
    {}

    // Cancellation while a successful backend response is held at the IO boundary.
    let term = campaign(io.clone(), parent.clone()).await;
    io.mode.store(1, Ordering::SeqCst);
    let keep = term.keep(&Handle::current(), CancellationToken::new());
    timeout(Duration::from_secs(2), io.entered.notified())
        .await
        .unwrap();
    keep.cancel();
    assert_eq!(joined(keep).await, LeaseKeepExit::Cancelled);
    assert_eq!(
        io.active.load(Ordering::SeqCst),
        0,
        "join drains pending IO"
    );
    io.release.notify_waiters();
    term.resign().await.unwrap();
    assert!(term.is_expired());

    // Dropping the owned handle cancels and drains even without an explicit join.
    let term = campaign(io.clone(), parent.clone()).await;
    let keep = term.keep(&Handle::current(), CancellationToken::new());
    timeout(Duration::from_secs(2), io.entered.notified())
        .await
        .unwrap();
    drop(keep);
    timeout(Duration::from_secs(2), async {
        while Arc::strong_count(&term) != 1 || io.active.load(Ordering::SeqCst) != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("dropped handle must release loop and pending requests");
    term.resign().await.unwrap();

    // Reset detaches locally before a slow revoke; late Keep cannot restart it.
    io.mode.store(1, Ordering::SeqCst);
    let term = campaign(io.clone(), parent.clone()).await;
    let keep = term.keep(&Handle::current(), CancellationToken::new());
    timeout(Duration::from_secs(2), io.entered.notified())
        .await
        .unwrap();
    io.block_revoke.store(true, Ordering::SeqCst);
    let old = term.clone();
    let reset = tokio::spawn(async move { old.resign().await });
    timeout(Duration::from_secs(2), io.revoke_entered.notified())
        .await
        .unwrap();
    assert!(term.is_expired());
    assert_eq!(joined(keep).await, LeaseKeepExit::Cancelled);
    assert_eq!(io.active.load(Ordering::SeqCst), 0);
    let late = term.keep(&Handle::current(), CancellationToken::new());
    assert_eq!(joined(late).await, LeaseKeepExit::Cancelled);
    io.release.notify_waiters();
    // A separately published replacement is unaffected by old cleanup.
    io.inner
        .transact(
            vec![],
            vec![Mutation::Delete {
                key: b"/keep".to_vec(),
            }],
        )
        .await
        .unwrap();
    let replacement = campaign(io.clone(), parent.clone()).await;
    io.mode.store(0, Ordering::SeqCst);
    let replacement_keep = replacement.keep(&Handle::current(), CancellationToken::new());
    io.block_revoke.store(false, Ordering::SeqCst);
    io.revoke_release.notify_one();
    reset.await.unwrap().unwrap();
    assert!(term.is_expired());
    replacement.validate().await.unwrap();
    assert!(!replacement.is_expired());
    replacement_keep.cancel();
    assert_eq!(joined(replacement_keep).await, LeaseKeepExit::Cancelled);
    replacement.resign().await.unwrap();

    // Errors and nonpositive TTL never reset the no-success timer.
    for mode in [1, 2, 3] {
        io.mode.store(mode, Ordering::SeqCst);
        let term = campaign(io.clone(), parent.clone()).await;
        let actual_ttl = io.granted_ttl.load(Ordering::SeqCst);
        let started = tokio::time::Instant::now();
        let keep = term.keep(&Handle::current(), CancellationToken::new());
        assert_eq!(joined(keep).await, LeaseKeepExit::TimedOut);
        assert!(started.elapsed() >= Duration::from_secs(1));
        assert_eq!(io.active.load(Ordering::SeqCst), 0);
        tokio::time::sleep(Duration::from_millis(10)).await;
        if mode == 2 {
            tokio::time::sleep_until(
                started + Duration::from_secs(actual_ttl as u64) + Duration::from_millis(30),
            )
            .await;
            assert!(term.is_expired());
            timeout(Duration::from_secs(3), async {
                while term.validate().await.is_ok() {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("backend eventually removes naturally expired key");
            assert!(matches!(
                term.validate().await,
                Err(ElectionError::NotLeader)
            ));
        }
        term.resign().await.unwrap();
    }
    io.mode.store(0, Ordering::SeqCst);
    let term = campaign(io.clone(), parent.clone()).await;
    // Ownership loss is terminal and never causes a new grant.
    io.inner
        .transact(
            vec![],
            vec![Mutation::Delete {
                key: b"/keep".to_vec(),
            }],
        )
        .await
        .unwrap();
    let keep = term.keep(&Handle::current(), CancellationToken::new());
    assert_eq!(joined(keep).await, LeaseKeepExit::NotLeader);
    term.resign().await.unwrap();
    let term = campaign(io.clone(), parent.clone()).await;
    let keep = term.keep(&Handle::current(), CancellationToken::new());
    parent.cancel();
    assert_eq!(joined(keep).await, LeaseKeepExit::Cancelled);
    assert_eq!(
        joined(term.keep(&Handle::current(), CancellationToken::new())).await,
        LeaseKeepExit::Cancelled
    );
    term.resign().await.unwrap();
}
#[tokio::test]
async fn lease_keep_memory() {
    behavior(Arc::new(MemoryStore::default())).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lease_keep_etcd() {
    let mut fixture = super::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    behavior(store.clone()).await;
    let term = campaign(store, CancellationToken::new()).await;
    fixture.command("stop", "stopped");
    let keep = term.keep(&Handle::current(), CancellationToken::new());
    assert_eq!(joined(keep).await, LeaseKeepExit::TimedOut);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert!(
        term.is_expired(),
        "disconnected loop must leave conservative deadline unchanged"
    );
    fixture.command("start", "restarted");
    term.resign().await.unwrap();
}
