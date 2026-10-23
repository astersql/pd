// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_storage_kv::*;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct TestClock(Arc<Mutex<Instant>>);
impl TestClock {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(Instant::now())))
    }
    fn advance(&self, duration: Duration) {
        *self.0.lock().unwrap() += duration;
    }
}
impl LeaseClock for TestClock {
    fn now(&self) -> Instant {
        *self.0.lock().unwrap()
    }
}
// Only delay/fail responses at the IO boundary; real backends execute every transaction.
struct RecordingStore {
    inner: Arc<dyn MetaStore>,
    clock: TestClock,
    delay: Mutex<Duration>,
    last_ttl: Mutex<i64>,
    unavailable: AtomicBool,
}
#[async_trait::async_trait]
impl MetaStore for RecordingStore {
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
    async fn keep_alive(&self, lease: LeaseId) -> astersql_pd_storage_kv::Result<Lease> {
        if self.unavailable.load(Ordering::SeqCst) {
            return Err(StoreError::Unavailable);
        }
        let lease = self.inner.keep_alive(lease).await?;
        *self.last_ttl.lock().unwrap() = lease.ttl_seconds;
        self.clock.advance(*self.delay.lock().unwrap());
        Ok(lease)
    }
    async fn revoke(&self, lease: LeaseId) -> astersql_pd_storage_kv::Result<()> {
        self.inner.revoke(lease).await
    }
    async fn ttl(&self, lease: LeaseId) -> astersql_pd_storage_kv::Result<i64> {
        self.inner.ttl(lease).await
    }
    async fn compact(&self, revision: Revision) -> astersql_pd_storage_kv::Result<()> {
        self.inner.compact(revision).await
    }
    async fn grant(&self, ttl_seconds: i64) -> astersql_pd_storage_kv::Result<Lease> {
        let lease = self.inner.grant(ttl_seconds).await?;
        *self.last_ttl.lock().unwrap() = lease.ttl_seconds;
        self.clock.advance(*self.delay.lock().unwrap());
        Ok(lease)
    }
}

#[test]
fn lease_expiry_boundaries() {
    let clock = Arc::new(TestClock::new());
    let expiry = LeaseExpiry::new(clock.clone());
    assert!(expiry.is_expired());
    let start = expiry.request_start();
    expiry.granted(start, 2);
    assert!(!expiry.is_expired());
    clock.advance(Duration::from_nanos(1_999_999_999));
    assert!(!expiry.is_expired(), "fractional seconds must not truncate");
    clock.advance(Duration::from_nanos(1));
    assert!(!expiry.is_expired(), "Go After is strict at equality");
    clock.advance(Duration::from_nanos(1));
    assert!(expiry.is_expired());
    expiry.granted(clock.now(), 5);
    let newer = expiry.request_start();
    clock.advance(Duration::from_secs(1));
    expiry.renewed(newer, 10);
    expiry.renewed(start, 1); // An older out-of-order response cannot shorten the maximum.
    expiry.renewed(clock.now(), 0);
    expiry.renewed(clock.now(), -1);
    clock.advance(Duration::from_secs(8));
    assert!(!expiry.is_expired());
    clock.advance(Duration::from_secs(2));
    assert!(expiry.is_expired());
    expiry.close();
    expiry.renewed(clock.now(), 100);
    assert!(
        expiry.is_expired(),
        "late response cannot revive a closed lease"
    );
    expiry.granted(clock.now(), 3);
    assert!(!expiry.is_expired(), "a fresh grant can reopen it");
}

async fn behavior(backend: Arc<dyn MetaStore>) {
    let clock = TestClock::new();
    let io = Arc::new(RecordingStore {
        inner: backend,
        clock: clock.clone(),
        delay: Mutex::new(Duration::from_secs(2)),
        last_ttl: Mutex::new(0),
        unavailable: AtomicBool::new(false),
    });
    let term = Leadership::campaign_with_key_and_clock(
        io.clone(),
        CancellationToken::new(),
        42,
        b"/expiry".to_vec(),
        b"leader".to_vec(),
        30,
        vec![],
        Arc::new(clock.clone()),
    )
    .await
    .unwrap();
    let granted_ttl = *io.last_ttl.lock().unwrap();
    assert!(!term.is_expired());
    clock.advance(Duration::from_secs((granted_ttl - 2) as u64));
    assert!(!term.is_expired()); // exact request-start + response TTL
    clock.advance(Duration::from_nanos(1));
    assert!(
        term.is_expired(),
        "grant response delay must consume the safety window"
    );
    // Local expiry does not pretend the backend has deleted the still-live key.
    term.validate().await.unwrap();
    assert!(io.ttl(term.lease_id()).await.unwrap() > 0);
    term.renew().await.unwrap();
    let renewed_ttl = *io.last_ttl.lock().unwrap();
    assert!(!term.is_expired());
    io.unavailable.store(true, Ordering::SeqCst);
    assert!(matches!(
        term.renew().await,
        Err(ElectionError::Store(StoreError::Unavailable))
    ));
    clock.advance(Duration::from_secs((renewed_ttl - 2) as u64));
    assert!(!term.is_expired());
    clock.advance(Duration::from_nanos(1));
    assert!(
        term.is_expired(),
        "failed renewal cannot extend the deadline"
    );
    io.unavailable.store(false, Ordering::SeqCst);
    *io.delay.lock().unwrap() = Duration::from_secs(100);
    term.renew().await.unwrap();
    assert!(
        term.is_expired(),
        "even successful renew can arrive after its conservative deadline"
    );
    term.resign().await.unwrap();
    assert!(term.is_expired());
    assert!(matches!(
        io.ttl(term.lease_id()).await,
        Err(StoreError::LeaseNotFound)
    ));
    // Use the backend response TTL, including etcd's rounding of a one-second request.
    *io.delay.lock().unwrap() = Duration::ZERO;
    let rounded = Leadership::campaign_with_key_and_clock(
        io.clone(),
        CancellationToken::new(),
        42,
        b"/expiry".to_vec(),
        b"rounded".to_vec(),
        1,
        vec![],
        Arc::new(clock.clone()),
    )
    .await
    .unwrap();
    let actual_ttl = *io.last_ttl.lock().unwrap();
    clock.advance(Duration::from_secs(actual_ttl as u64));
    assert!(
        !rounded.is_expired(),
        "response TTL must be preserved exactly"
    );
    clock.advance(Duration::from_nanos(1));
    assert!(rounded.is_expired());
    rounded.resign().await.unwrap();
    *io.delay.lock().unwrap() = Duration::from_secs(100);
    // A grant response may arrive after its whole TTL, including backend rounding.
    let slow = Leadership::campaign_with_key_and_clock(
        io.clone(),
        CancellationToken::new(),
        42,
        b"/expiry".to_vec(),
        b"new".to_vec(),
        1,
        vec![],
        Arc::new(clock),
    )
    .await
    .unwrap();
    assert!(slow.is_expired());
    slow.resign().await.unwrap();
}
#[tokio::test]
async fn lease_expiry_memory() {
    behavior(Arc::new(MemoryStore::default())).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lease_expiry_etcd() {
    let mut fixture = super::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    behavior(store.clone()).await;
    let clock = Arc::new(TestClock::new());
    let term = Leadership::campaign_with_key_and_clock(
        store,
        CancellationToken::new(),
        42,
        b"/disconnect".to_vec(),
        b"leader".to_vec(),
        30,
        vec![],
        clock.clone(),
    )
    .await
    .unwrap();
    assert!(!term.is_expired());
    fixture.command("stop", "stopped");
    assert!(term.renew().await.is_err());
    clock.advance(Duration::from_secs(31));
    assert!(
        term.is_expired(),
        "backend disconnect must not delay local expiry"
    );
    assert!(term.resign().await.is_err());
    assert!(
        term.is_expired(),
        "close invalidates locally even if revoke fails"
    );
    fixture.command("start", "restarted");
    term.resign().await.unwrap();
    assert!(term.is_expired());
}
