// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_storage_kv::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use tokio::{sync::Notify, time::timeout};
use tokio_util::sync::CancellationToken;

// Gate only completion of a real read; never invent transaction/snapshot results.
struct GateStore {
    inner: Arc<dyn MetaStore>,
    gate: AtomicBool,
    entered: Notify,
    release: Notify,
}
impl GateStore {
    fn new(inner: Arc<dyn MetaStore>) -> Self {
        Self {
            inner,
            gate: AtomicBool::new(true),
            entered: Notify::new(),
            release: Notify::new(),
        }
    }
}
#[async_trait::async_trait]
impl MetaStore for GateStore {
    async fn read(&self, request: Read) -> Result<Snapshot> {
        let page = self.inner.read(request).await?;
        if self.gate.swap(false, Ordering::SeqCst) {
            self.entered.notify_one();
            self.release.notified().await;
        }
        Ok(page)
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
        self.inner.keep_alive(id).await
    }
    async fn revoke(&self, id: LeaseId) -> Result<()> {
        self.inner.revoke(id).await
    }
    async fn ttl(&self, lease: LeaseId) -> astersql_pd_storage_kv::Result<i64> {
        self.inner.ttl(lease).await
    }
    async fn compact(&self, revision: Revision) -> astersql_pd_storage_kv::Result<()> {
        self.inner.compact(revision).await
    }
    async fn grant(&self, ttl: i64) -> astersql_pd_storage_kv::Result<Lease> {
        self.inner.grant(ttl).await
    }
}

async fn run(store: Arc<dyn MetaStore>) {
    let keys: &[&[u8]] = &[b"/scan/a", b"/scan/a\0", b"/scan/b", b"/scan/c"];
    let commit = store
        .transact(
            vec![],
            keys.iter().map(|k| Mutation::put(k, b"old")).collect(),
        )
        .await
        .unwrap();
    store
        .transact(vec![], vec![Mutation::put(b"/scan0/outside", b"outside")])
        .await
        .unwrap();
    let revision = store
        .read(Read::prefix(b"/scan/", Consistency::Linearizable))
        .await
        .unwrap()
        .revision;
    assert!(revision > commit.revision);
    for batch in [1, 2, 3, 4, 5, 0] {
        let snapshot = SnapshotLoader::new(b"/scan/".to_vec(), true, batch)
            .load(store.as_ref(), &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(
            snapshot
                .entries
                .iter()
                .map(|e| e.key.as_slice())
                .collect::<Vec<_>>(),
            keys
        );
        assert_eq!(snapshot.revision, revision);
        assert_eq!(snapshot.next_revision, revision + 1);
    }
    let gated = Arc::new(GateStore::new(store.clone()));
    let loader = SnapshotLoader::new(b"/scan/".to_vec(), true, 1);
    let cancel = CancellationToken::new();
    let work = loader.load(gated.as_ref(), &cancel);
    tokio::pin!(work);
    tokio::select! {
        biased;
        result = &mut work => panic!("published before complete snapshot: {result:?}"),
        result = timeout(Duration::from_secs(5), gated.entered.notified()) => result.unwrap(),
    }
    store
        .transact(
            vec![],
            vec![
                Mutation::put(keys[0], b"new"),
                Mutation::put(keys[2], b"new"),
                Mutation::Delete {
                    key: keys[3].to_vec(),
                },
                Mutation::put(b"/scan/d", b"new"),
            ],
        )
        .await
        .unwrap();
    gated.release.notify_one();
    let snapshot = timeout(Duration::from_secs(5), &mut work)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.revision, revision);
    assert_eq!(snapshot.next_revision, revision + 1);
    assert_eq!(
        snapshot
            .entries
            .iter()
            .map(|e| e.key.as_slice())
            .collect::<Vec<_>>(),
        keys
    );
    assert!(snapshot.entries.iter().all(|e| e.value == b"old"));

    // Compaction occurs after the first response, before the second request.
    let gated = GateStore::new(store.clone());
    let loader = SnapshotLoader::new(b"/scan/".to_vec(), true, 1);
    let work = loader.load(&gated, &cancel);
    tokio::pin!(work);
    tokio::select! {
        result = &mut work => panic!("early publish: {result:?}"),
        result = timeout(Duration::from_secs(5), gated.entered.notified()) => result.unwrap(),
    }
    let newer = store
        .transact(vec![], vec![Mutation::put(b"/scan/e", b"new")])
        .await
        .unwrap();
    store.compact(newer.revision).await.unwrap();
    gated.release.notify_one();
    assert!(matches!(work.await, Err(StoreError::Compacted { .. })));

    // Cancellation while a response is gated must discard it, even on the final page.
    for batch in [1, 0] {
        let gated = GateStore::new(store.clone());
        let loader = SnapshotLoader::new(b"/scan/".to_vec(), true, batch);
        let token = CancellationToken::new();
        let work = loader.load(&gated, &token);
        tokio::pin!(work);
        tokio::select! {
            result = &mut work => panic!("early publish: {result:?}"),
            result = timeout(Duration::from_secs(5), gated.entered.notified()) => result.unwrap(),
        }
        token.cancel();
        assert!(matches!(
            timeout(Duration::from_secs(1), &mut work).await.unwrap(),
            Err(StoreError::Cancelled)
        ));
    }
    for (key, prefix, expected) in [
        (b"/absent".as_slice(), true, 0),
        (b"/scan/a".as_slice(), false, 1),
        (b"".as_slice(), true, 6),
    ] {
        let result = SnapshotLoader::new(key.to_vec(), prefix, 1)
            .load(store.as_ref(), &cancel)
            .await
            .unwrap();
        assert_eq!(result.entries.len(), expected);
    }
    let token = CancellationToken::new();
    token.cancel();
    assert!(matches!(
        SnapshotLoader::new(b"/scan/".to_vec(), true, 1)
            .load(store.as_ref(), &token)
            .await,
        Err(StoreError::Cancelled)
    ));
    eprintln!(
        "snapshot contract: limits, pinned revision, atomic errors/cancellation, exact/empty prefix passed"
    );
}
#[tokio::test]
async fn snapshot_loader_values() {
    let store = MemoryStore::default();
    for (key, prefix, batch) in [
        (vec![], false, 1),
        (vec![], true, usize::MAX),
        (vec![], true, i64::MAX as usize),
    ] {
        assert!(matches!(
            SnapshotLoader::new(key, prefix, batch)
                .load(&store, &CancellationToken::new())
                .await,
            Err(StoreError::InvalidArgument)
        ));
    }
    let snapshot = SnapshotLoader::new(vec![], true, 0)
        .load(&store, &CancellationToken::new())
        .await
        .unwrap();
    assert!(snapshot.entries.is_empty());
    assert_eq!(snapshot.next_revision, snapshot.revision + 1);
}
#[tokio::test]
async fn snapshot_loader_memory() {
    run(Arc::new(MemoryStore::default())).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_loader_etcd() {
    let fixture = fixture::Fixture::start();
    run(Arc::new(fixture.store().await)).await;
}
