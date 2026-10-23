// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_storage_kv::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicI64, Ordering},
    },
    time::Duration,
};
use tokio::time::{sleep, timeout};

// Discard real backend responses at the IO boundary, never fabricate a commit.
struct LossyStore {
    inner: Arc<dyn MetaStore>,
    lose_write: AtomicBool,
    lose_revoke: AtomicBool,
    block_revoke: AtomicBool,
    lease: AtomicI64,
}
#[async_trait::async_trait]
impl MetaStore for LossyStore {
    async fn read(&self, request: Read) -> Result<Snapshot> {
        self.inner.read(request).await
    }
    async fn transact(&self, compares: Vec<Compare>, writes: Vec<Mutation>) -> Result<Commit> {
        let result = self.inner.transact(compares, writes).await?;
        if self.lose_write.swap(false, Ordering::SeqCst) {
            return Err(StoreError::Unavailable);
        }
        Ok(result)
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

    async fn grant(&self, ttl: i64) -> Result<Lease> {
        let lease = self.inner.grant(ttl).await?;
        self.lease.store(lease.id, Ordering::SeqCst);
        Ok(lease)
    }
    async fn keep_alive(&self, lease: LeaseId) -> Result<Lease> {
        self.inner.keep_alive(lease).await
    }
    async fn revoke(&self, lease: LeaseId) -> Result<()> {
        // A failed dispatch is a transport boundary, not a fake store result.
        if self.block_revoke.swap(false, Ordering::SeqCst) {
            return Err(StoreError::Unavailable);
        }
        self.inner.revoke(lease).await?;
        if self.lose_revoke.swap(false, Ordering::SeqCst) {
            return Err(StoreError::Unavailable);
        }
        Ok(())
    }
    async fn ttl(&self, lease: LeaseId) -> Result<i64> {
        self.inner.ttl(lease).await
    }
    async fn compact(&self, revision: Revision) -> Result<()> {
        self.inner.compact(revision).await
    }
}
async fn entries(store: &dyn MetaStore, key: &[u8]) -> Vec<Entry> {
    store
        .read(Read::range(key, b"", Consistency::Linearizable))
        .await
        .unwrap()
        .entries
}
async fn expired(store: &dyn MetaStore, key: &[u8]) {
    timeout(Duration::from_secs(8), async {
        loop {
            if entries(store, key).await.is_empty() {
                break;
            }
            sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("TTL key did not expire");
}
async fn run(store: Arc<dyn MetaStore>) {
    let io = LossyStore {
        inner: store.clone(),
        lose_write: AtomicBool::new(false),
        lose_revoke: AtomicBool::new(false),
        block_revoke: AtomicBool::new(false),
        lease: AtomicI64::new(0),
    };
    // Empty key produces a real backend validation error after a real grant.
    let error = put_with_ttl(&io, b"", b"bad", 30).await.unwrap_err();
    assert_eq!(error.cause, StoreError::InvalidArgument);
    let id = error.lease.unwrap();
    assert_eq!(
        store.ttl(id).await,
        Err(StoreError::LeaseNotFound),
        "failed Put must revoke its granted lease"
    );
    assert_eq!(error.cleanup_error, None);
    let error = put_with_ttl(&io, b"/ttl/invalid", b"v", 0)
        .await
        .unwrap_err();
    assert_eq!(error.cause, StoreError::InvalidArgument);
    assert_eq!(error.lease, None);
    assert!(entries(&*store, b"/ttl/invalid").await.is_empty());

    // The backend really commits; discard its response, then compensate once.
    let before = store
        .read(Read::prefix(b"/ttl/lost", Consistency::Linearizable))
        .await
        .unwrap()
        .revision;
    io.lose_write.store(true, Ordering::SeqCst);
    let error = put_with_ttl(&io, b"/ttl/lost", b"v", 30).await.unwrap_err();
    assert_eq!(error.cause, StoreError::Unavailable);
    assert_eq!(error.cleanup_error, None);
    assert!(entries(&*store, b"/ttl/lost").await.is_empty());
    assert_eq!(
        store.ttl(error.lease.unwrap()).await,
        Err(StoreError::LeaseNotFound)
    );
    let after = store
        .read(Read::prefix(b"/ttl/lost", Consistency::Linearizable))
        .await
        .unwrap()
        .revision;
    assert_eq!(
        after,
        before + 2,
        "exactly one real Put and one compensating Delete; no blind write retry"
    );
    let last_grant = io.lease.load(Ordering::SeqCst);
    assert_eq!(error.lease, Some(last_grant));

    // Even cleanup's response can be lost; preserve both errors and lease ID.
    io.lose_write.store(true, Ordering::SeqCst);
    io.lose_revoke.store(true, Ordering::SeqCst);
    let error = put_with_ttl(&io, b"/ttl/cleanup", b"v", 30)
        .await
        .unwrap_err();
    assert_eq!(error.cause, StoreError::Unavailable);
    assert_eq!(error.cleanup_error, Some(StoreError::Unavailable));
    assert!(entries(&*store, b"/ttl/cleanup").await.is_empty());
    assert_eq!(
        store.ttl(error.lease.unwrap()).await,
        Err(StoreError::LeaseNotFound)
    );

    // Failed revoke dispatch leaves a real lease; recovery uses the reported ID.
    io.lose_write.store(true, Ordering::SeqCst);
    io.block_revoke.store(true, Ordering::SeqCst);
    let error = put_with_ttl(&io, b"/ttl/recover", b"v", 30)
        .await
        .unwrap_err();
    assert_eq!(error.cause, StoreError::Unavailable);
    assert_eq!(error.cleanup_error, Some(StoreError::Unavailable));
    let id = error.lease.unwrap();
    assert!(store.ttl(id).await.unwrap() > 0);
    assert_eq!(entries(&*store, b"/ttl/recover").await[0].lease, Some(id));
    store.revoke(id).await.unwrap();
    assert!(entries(&*store, b"/ttl/recover").await.is_empty());
    assert_eq!(store.ttl(id).await, Err(StoreError::LeaseNotFound));

    let lease = put_with_ttl(&*store, b"/ttl/expire", &[0, 255, 42], 1)
        .await
        .unwrap();
    let initial = entries(&*store, b"/ttl/expire").await;
    assert_eq!(initial[0].lease, Some(lease.id));
    assert_eq!(initial[0].value, [0, 255, 42]);
    let mut watch = store
        .watch(b"/ttl/expire".to_vec(), initial[0].mod_revision + 1)
        .await
        .unwrap();
    expired(&*store, b"/ttl/expire").await;
    let batch = timeout(
        Duration::from_secs(4),
        watch.next(&tokio_util::sync::CancellationToken::new()),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        matches!(&batch.events[..], [Event::Delete { key, revision }] if key == b"/ttl/expire" && *revision > initial[0].mod_revision)
    );
    assert_eq!(store.ttl(lease.id).await, Err(StoreError::LeaseNotFound));

    let lease = put_with_ttl(&*store, b"/ttl/renew", b"v", 3).await.unwrap();
    sleep(Duration::from_secs(2)).await;
    assert_eq!(store.keep_alive(lease.id).await.unwrap().id, lease.id);
    sleep(Duration::from_secs(2)).await;
    assert_eq!(
        entries(&*store, b"/ttl/renew").await[0].lease,
        Some(lease.id)
    );
    store.revoke(lease.id).await.unwrap();
    assert!(entries(&*store, b"/ttl/renew").await.is_empty());
    assert_eq!(
        store.keep_alive(lease.id).await.unwrap_err(),
        StoreError::LeaseNotFound
    );
    eprintln!(
        "ttl_put: failed write cleanup, lost commit/revoke responses, raw bytes, expiry Delete, renewal beyond original TTL, explicit revoke; terminal keys absent and leases revoked/expired"
    );
}
#[tokio::test]
async fn ttl_put_memory() {
    run(Arc::new(MemoryStore::default())).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ttl_put_etcd() {
    let fixture = fixture::Fixture::start();
    run(Arc::new(fixture.store().await)).await;
}
