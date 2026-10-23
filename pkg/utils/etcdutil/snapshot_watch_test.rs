// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_storage_kv::*;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{sync::Notify, time::timeout};
use tokio_util::sync::CancellationToken;

// Drop a real IO response once, modelling a disconnected stream after receipt.
// All reads, writes, revisions and replay results come from the actual backend.
struct LossyStore {
    inner: Arc<dyn MetaStore>,
    lose: Arc<AtomicBool>,
    active: Arc<AtomicUsize>,
    observe_wait: Arc<AtomicBool>,
    entered: Arc<Notify>,
    starts: Mutex<Vec<Revision>>,
}
struct LossyWatch {
    inner: Box<dyn MetaWatch>,
    lose: Arc<AtomicBool>,
    active: Arc<AtomicUsize>,
    observe_wait: Arc<AtomicBool>,
    entered: Arc<Notify>,
}
impl Drop for LossyWatch {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::SeqCst);
    }
}
#[async_trait::async_trait]
impl MetaWatch for LossyWatch {
    async fn next(&mut self, cancel: &CancellationToken) -> Result<WatchBatch> {
        if self.observe_wait.swap(false, Ordering::SeqCst) {
            self.entered.notify_one();
        }
        let batch = self.inner.next(cancel).await?;
        if self.lose.swap(false, Ordering::SeqCst) {
            return Err(StoreError::Unavailable);
        }
        Ok(batch)
    }
}
#[async_trait::async_trait]
impl MetaStore for LossyStore {
    async fn read(&self, request: Read) -> Result<Snapshot> {
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
        self.starts.lock().unwrap().push(start_revision);
        let inner = self.inner.watch(prefix, start_revision).await?;
        self.active.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(LossyWatch {
            inner,
            lose: self.lose.clone(),
            active: self.active.clone(),
            observe_wait: self.observe_wait.clone(),
            entered: self.entered.clone(),
        }))
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

async fn run(inner: Arc<dyn MetaStore>) {
    let store = Arc::new(LossyStore {
        inner,
        lose: Arc::new(AtomicBool::new(false)),
        active: Arc::new(AtomicUsize::new(0)),
        observe_wait: Arc::new(AtomicBool::new(false)),
        entered: Arc::new(Notify::new()),
        starts: Mutex::new(vec![]),
    });
    let token = CancellationToken::new();
    store
        .transact(vec![], vec![Mutation::put(b"/watch/a", b"old")])
        .await
        .unwrap();
    let (snapshot, mut watch) =
        SnapshotWatch::load(store.clone(), b"/watch/".to_vec(), true, 1, &token)
            .await
            .unwrap();
    assert_eq!(snapshot.entries.len(), 1);
    assert_eq!(snapshot.entries[0].value, b"old");
    let start = snapshot.next_revision;
    assert_eq!(watch.next_revision(), snapshot.revision + 1);
    // Write after the snapshot, before any stream is opened. One transaction
    // preserves a multi-event batch at one revision on both actual backends.
    let committed = store
        .transact(
            vec![],
            vec![
                Mutation::put(b"/watch/a", b"new"),
                Mutation::put(b"/watch/b", b"new"),
            ],
        )
        .await
        .unwrap();
    let mut failed = None;
    let error = timeout(
        Duration::from_secs(5),
        watch.apply_next(&token, |batch| {
            failed = Some(batch.clone());
            Err(StoreError::Backend)
        }),
    )
    .await
    .unwrap();
    assert_eq!(error, Err(StoreError::Backend));
    assert_eq!(
        watch.next_revision(),
        start,
        "callback failure must not acknowledge a batch"
    );
    let failed = failed.unwrap();
    assert_eq!(failed.revision, committed.revision);
    assert_eq!(failed.events.len(), 2);
    assert!(
        failed
            .events
            .iter()
            .all(|e| matches!(e, Event::Put(entry) if entry.value == b"new"))
    );
    // A newer write during callback retry must not replace or merge the failed batch.
    let deleted = store
        .transact(
            vec![],
            vec![Mutation::Delete {
                key: b"/watch/b".to_vec(),
            }],
        )
        .await
        .unwrap();
    timeout(
        Duration::from_secs(5),
        watch.apply_next(&token, |batch| {
            assert_eq!(batch.events, failed.events);
            assert_eq!(batch.revision, failed.revision);
            Ok(())
        }),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(watch.next_revision(), committed.revision + 1);
    store.lose.store(true, Ordering::SeqCst);
    assert_eq!(
        timeout(
            Duration::from_secs(5),
            watch.apply_next(&token, |_| panic!("lost response published"))
        )
        .await
        .unwrap(),
        Err(StoreError::Unavailable)
    );
    assert_eq!(watch.next_revision(), committed.revision + 1);
    assert_eq!(
        store.active.load(Ordering::SeqCst),
        0,
        "disconnected stream closed"
    );
    timeout(
        Duration::from_secs(5),
        watch.apply_next(&token, |batch| {
            assert_eq!(
                batch.events,
                vec![Event::Delete {
                    key: b"/watch/b".to_vec(),
                    revision: deleted.revision
                }]
            );
            Ok(())
        }),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        *store.starts.lock().unwrap(),
        vec![start, committed.revision + 1]
    );
    assert_eq!(watch.next_revision(), deleted.revision + 1);

    // Host-owned task cancellation must close resources and be joined.
    let active = store.active.clone();
    let child_token = token.clone();
    store.observe_wait.store(true, Ordering::SeqCst);
    let work = tokio::spawn(async move {
        assert_eq!(
            watch
                .apply_next(&child_token, |_| panic!("unexpected extra event"))
                .await,
            Err(StoreError::Cancelled)
        );
        assert_eq!(
            watch
                .apply_next(&CancellationToken::new(), |_| panic!(
                    "revived cancelled observer"
                ))
                .await,
            Err(StoreError::Cancelled)
        );
    });
    timeout(Duration::from_secs(2), store.entered.notified())
        .await
        .unwrap();
    token.cancel();
    timeout(Duration::from_secs(2), work)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(active.load(Ordering::SeqCst), 0);

    // Exact-key selection must exclude a neighboring key with the same prefix.
    let token = CancellationToken::new();
    let (_, mut exact) = SnapshotWatch::load(store.clone(), b"/watch/a".to_vec(), false, 0, &token)
        .await
        .unwrap();
    store
        .transact(
            vec![],
            vec![Mutation::put(b"/watch/a-neighbor", b"outside")],
        )
        .await
        .unwrap();
    store
        .transact(vec![], vec![Mutation::put(b"/watch/a", b"exact")])
        .await
        .unwrap();
    timeout(Duration::from_secs(5), exact.apply_next(&token, |batch| {
        assert_eq!(batch.events.len(), 1);
        assert!(matches!(&batch.events[0], Event::Put(e) if e.key == b"/watch/a" && e.value == b"exact"));
        Ok(())
    })).await.unwrap().unwrap();
    drop(exact);
    assert_eq!(active.load(Ordering::SeqCst), 0);
    // A compacted handoff is an explicit error, never an implicit cursor jump.
    let (snapshot, mut compacted) =
        SnapshotWatch::load(store.clone(), b"/watch/".to_vec(), true, 1, &token)
            .await
            .unwrap();
    let latest = store
        .transact(vec![], vec![Mutation::put(b"/watch/c", b"new")])
        .await
        .unwrap();
    // Compact strictly beyond the requested revision: etcd may still serve
    // events at the compaction boundary itself.
    let boundary = store
        .transact(vec![], vec![Mutation::put(b"/watch/d", b"new")])
        .await
        .unwrap();
    assert!(boundary.revision > latest.revision);
    store.compact(boundary.revision).await.unwrap();
    assert!(matches!(
        timeout(
            Duration::from_secs(5),
            compacted.apply_next(&token, |_| panic!("compacted batch published"))
        )
        .await
        .unwrap(),
        Err(StoreError::Compacted { .. })
    ));
    assert_eq!(compacted.next_revision(), snapshot.next_revision);
    assert_eq!(active.load(Ordering::SeqCst), 0);
    eprintln!(
        "snapshot_watch: handoff, callback retry, lost IO replay, exact key, cancellation/join, compaction passed"
    );
}
#[tokio::test]
async fn snapshot_watch_memory() {
    run(Arc::new(MemoryStore::default())).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_watch_etcd() {
    let mut fixture = fixture::Fixture::start();
    run(Arc::new(fixture.store().await)).await;
    // Real TCP disconnection: stop embedded etcd, then restart its same WAL.
    let store = Arc::new(fixture.store().await);
    let cancel = CancellationToken::new();
    let (_, mut watch) =
        SnapshotWatch::load(store.clone(), b"/restart/".to_vec(), true, 1, &cancel)
            .await
            .unwrap();
    store
        .transact(vec![], vec![Mutation::put(b"/restart/a", b"before")])
        .await
        .unwrap();
    watch.apply_next(&cancel, |_| Ok(())).await.unwrap();
    let resume = watch.next_revision();
    fixture.command("stop", "stopped");
    assert_eq!(
        timeout(
            Duration::from_secs(5),
            watch.apply_next(&cancel, |_| panic!("published during outage"))
        )
        .await
        .unwrap(),
        Err(StoreError::Unavailable)
    );
    assert_eq!(watch.next_revision(), resume);
    fixture.command("start", "restarted");
    // Write once through a newly connected client; the existing observer must
    // reconnect and replay from its acknowledged cursor, not from latest.
    let writer = fixture.store().await;
    let committed = writer
        .transact(vec![], vec![Mutation::put(b"/restart/a", b"after")])
        .await
        .unwrap();
    timeout(Duration::from_secs(10), async {
        loop {
            match watch.apply_next(&cancel, |batch| {
                assert_eq!(batch.events.len(), 1);
                assert!(matches!(&batch.events[0], Event::Put(e) if e.value == b"after" && e.mod_revision == committed.revision));
                Ok(())
            }).await {
                Ok(_) => break,
                Err(StoreError::Unavailable) => tokio::time::sleep(Duration::from_millis(50)).await,
                error => panic!("unexpected recovery result: {error:?}"),
            }
        }
    }).await.unwrap();
    assert_eq!(watch.next_revision(), committed.revision + 1);
    cancel.cancel();
    assert_eq!(
        watch.apply_next(&cancel, |_| panic!("cancelled")).await,
        Err(StoreError::Cancelled)
    );
    eprintln!(
        "snapshot_watch: real etcd stop -> Unavailable/no advance -> same-WAL restart -> new write replayed -> Cancelled"
    );
}
