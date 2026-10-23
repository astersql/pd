// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_storage_kv::*;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

// Observe real lease grants at the IO boundary; all transactions use the real backend.
struct RecordingStore {
    inner: Arc<dyn MetaStore>,
    grants: Mutex<Vec<LeaseId>>,
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
        self.inner.keep_alive(lease).await
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
        self.grants.lock().unwrap().push(lease.id);
        Ok(lease)
    }
}
async fn entry(store: &dyn MetaStore, key: &[u8]) -> Vec<Entry> {
    store
        .read(Read::range(key, b"", Consistency::Linearizable))
        .await
        .unwrap()
        .entries
}
async fn behavior(backend: Arc<dyn MetaStore>) {
    let observed = Arc::new(RecordingStore {
        inner: backend,
        grants: Mutex::new(vec![]),
    });
    let store: Arc<dyn MetaStore> = observed.clone();
    let campaign = |key: &'static [u8], value: &'static [u8], compares| {
        Leadership::campaign_with_key(
            store.clone(),
            CancellationToken::new(),
            42,
            key.to_vec(),
            value.to_vec(),
            30,
            compares,
        )
    };
    // Two paths on the same backend and cluster remain independent.
    let a = campaign(b"/election/a", b"opaque\0\xff", vec![])
        .await
        .unwrap();
    let b = campaign(b"/election/ab", b"second", vec![]).await.unwrap();
    for (key, value, term) in [
        (b"/election/a".as_slice(), b"opaque\0\xff".as_slice(), &a),
        (b"/election/ab", b"second", &b),
    ] {
        let saved = entry(store.as_ref(), key).await;
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].value, value);
        assert_eq!(saved[0].lease, Some(term.lease_id()));
        assert_eq!(saved[0].mod_revision, term.revision());
        term.validate().await.unwrap();
        assert!(matches!(
            term.transact(vec![], vec![Mutation::put(key, b"bad")])
                .await,
            Err(ElectionError::InvalidArgument)
        ));
    }
    assert!(entry(store.as_ref(), b"/pd/42/leader").await.is_empty());
    // Contention and rejected conditions must revoke the actual granted lease.
    assert!(matches!(
        campaign(b"/election/a", b"loser", vec![]).await,
        Err(ElectionError::Contended)
    ));
    let failed_lease = *observed.grants.lock().unwrap().last().unwrap();
    assert!(matches!(
        store.ttl(failed_lease).await,
        Err(StoreError::LeaseNotFound)
    ));
    store
        .transact(vec![], vec![Mutation::put(b"/gate", b"closed")])
        .await
        .unwrap();
    let condition = || vec![Compare::Value(b"/gate".to_vec(), b"open".to_vec())];
    assert!(matches!(
        campaign(b"/election/c", b"third", condition()).await,
        Err(ElectionError::Contended)
    ));
    let failed_lease = *observed.grants.lock().unwrap().last().unwrap();
    assert!(matches!(
        store.ttl(failed_lease).await,
        Err(StoreError::LeaseNotFound)
    ));
    assert!(entry(store.as_ref(), b"/election/c").await.is_empty());
    store
        .transact(vec![], vec![Mutation::put(b"/gate", b"open")])
        .await
        .unwrap();
    let c = campaign(b"/election/c", b"third", condition())
        .await
        .unwrap();
    // Transaction validation error also cleans the granted lease.
    assert!(matches!(
        campaign(b"/election/error", b"x", vec![Compare::Exists(vec![])]).await,
        Err(ElectionError::Store(StoreError::InvalidArgument))
    ));
    let failed_lease = *observed.grants.lock().unwrap().last().unwrap();
    assert!(matches!(
        store.ttl(failed_lease).await,
        Err(StoreError::LeaseNotFound)
    ));
    // Two concurrent candidates at a fresh key have exactly one winner.
    let (left, right) = tokio::join!(
        campaign(b"/race", b"left", vec![]),
        campaign(b"/race", b"right", vec![])
    );
    let winner = match (left, right) {
        (Ok(term), Err(ElectionError::Contended)) | (Err(ElectionError::Contended), Ok(term)) => {
            term
        }
        _ => panic!("expected exactly one successful candidate"),
    };
    winner.validate().await.unwrap();
    let default = Leadership::campaign(
        store.clone(),
        CancellationToken::new(),
        42,
        b"default".to_vec(),
        30,
    )
    .await
    .unwrap();
    assert_eq!(
        entry(store.as_ref(), b"/pd/42/leader").await[0].lease,
        Some(default.lease_id())
    );
    a.resign().await.unwrap();
    b.validate().await.unwrap();
    c.validate().await.unwrap();
    default.validate().await.unwrap();
    assert!(entry(store.as_ref(), b"/election/a").await.is_empty());
    for term in [b, c, winner, default] {
        term.resign().await.unwrap();
    }
    for lease in observed.grants.lock().unwrap().clone() {
        assert!(matches!(
            store.ttl(lease).await,
            Err(StoreError::LeaseNotFound)
        ));
    }
}
#[tokio::test]
async fn campaign_key_memory() {
    behavior(Arc::new(MemoryStore::default())).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn campaign_key_etcd() {
    let fixture = super::etcd_fixture::Fixture::start();
    behavior(Arc::new(fixture.store().await)).await;
}
