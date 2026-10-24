// Copyright 2026 AsterSQL.
use crate::{HistoryBuffer, HistoryIndex, MetadataWatch};
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
    after_read: Mutex<Option<(Vec<Mutation>, bool)>>,
    fail_read: AtomicBool,
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
        let page = self.inner.read(request).await?;
        let action = self.after_read.lock().unwrap().take();
        if let Some((writes, compact)) = action {
            let commit = self.inner.transact(vec![], writes).await?;
            if compact {
                self.inner.compact(commit.revision).await?;
            }
        }
        if self.fail_read.swap(false, Ordering::SeqCst) {
            return Err(StoreError::Unavailable);
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
fn controlled(inner: Arc<dyn MetaStore>) -> Arc<LossyStore> {
    Arc::new(LossyStore {
        inner,
        lose: Arc::new(AtomicBool::new(false)),
        active: Arc::new(AtomicUsize::new(0)),
        observe_wait: Arc::new(AtomicBool::new(false)),
        entered: Arc::new(Notify::new()),
        starts: Mutex::new(vec![]),
        after_read: Mutex::new(None),
        fail_read: AtomicBool::new(false),
    })
}
fn values(watch: &MetadataWatch) -> Vec<(Vec<u8>, Vec<u8>)> {
    watch
        .entries()
        .iter()
        .map(|(k, e)| (k.clone(), e.value.clone()))
        .collect()
}
async fn advance(watch: &mut MetadataWatch, token: &CancellationToken) -> Revision {
    timeout(Duration::from_secs(8), watch.advance(token))
        .await
        .unwrap()
        .unwrap()
}
async fn run(inner: Arc<dyn MetaStore>) {
    let store = controlled(inner);
    let token = CancellationToken::new();
    let mut history = HistoryBuffer::new(10).unwrap();
    history.reset_with_index(HistoryIndex(700));
    let initial = store
        .transact(
            vec![],
            vec![
                Mutation::put(b"/meta/a", b"old"),
                Mutation::put(b"/meta/b", b"old"),
                Mutation::put(b"/meta/c", b"old"),
            ],
        )
        .await
        .unwrap();
    // A real transaction between pages: later read headers must not repin the snapshot.
    *store.after_read.lock().unwrap() = Some((
        vec![
            Mutation::put(b"/meta/a", b"new"),
            Mutation::put(b"/meta/b", b"new"),
        ],
        false,
    ));
    let mut watch = MetadataWatch::load(store.clone(), b"/meta/".to_vec(), 1, &token)
        .await
        .unwrap();
    assert_eq!(
        watch.entries().len(),
        3,
        "complete initial metadata snapshot"
    );
    assert!(watch.entries().values().all(|e| e.value == b"old"));
    assert_eq!(watch.next_revision(), initial.revision + 1);
    assert_eq!(history.next_index(), HistoryIndex(700));
    let updated = advance(&mut watch, &token).await;
    assert_eq!(updated, initial.revision + 2);
    assert_eq!(watch.entries()[b"/meta/b".as_slice()].value, b"new");
    assert_eq!(*store.starts.lock().unwrap(), vec![initial.revision + 1]);

    let before = values(&watch);
    let commit = store
        .transact(
            vec![],
            vec![
                Mutation::Delete {
                    key: b"/meta/a".to_vec(),
                },
                Mutation::put(b"/meta/d", b"live"),
            ],
        )
        .await
        .unwrap();
    store.lose.store(true, Ordering::SeqCst);
    assert_eq!(
        timeout(Duration::from_secs(8), watch.advance(&token))
            .await
            .unwrap(),
        Err(StoreError::Unavailable)
    );
    assert_eq!(values(&watch), before);
    assert_eq!(watch.next_revision(), updated);
    assert_eq!(store.active.load(Ordering::SeqCst), 0);
    assert_eq!(advance(&mut watch, &token).await, commit.revision + 1);
    assert!(!watch.entries().contains_key(b"/meta/a".as_slice()));
    assert_eq!(watch.entries()[b"/meta/d".as_slice()].value, b"live");
    assert_eq!(store.starts.lock().unwrap().last(), Some(&updated));

    // An unopened cursor is compacted. Failed reload publishes nothing; retry must reload.
    drop(watch);
    let mut watch = MetadataWatch::load(store.clone(), b"/meta/".to_vec(), 1, &token)
        .await
        .unwrap();
    let old = values(&watch);
    let old_next = watch.next_revision();
    let deleted = store
        .transact(
            vec![],
            vec![
                Mutation::Delete {
                    key: b"/meta/b".to_vec(),
                },
                Mutation::Delete {
                    key: b"/meta/c".to_vec(),
                },
                Mutation::put(b"/meta/d", b"recovered"),
            ],
        )
        .await
        .unwrap();
    let boundary = store
        .transact(vec![], vec![Mutation::put(b"/outside", b"advance")])
        .await
        .unwrap();
    store.compact(boundary.revision).await.unwrap();
    store.fail_read.store(true, Ordering::SeqCst);
    assert_eq!(
        timeout(Duration::from_secs(8), watch.advance(&token))
            .await
            .unwrap(),
        Err(StoreError::Unavailable)
    );
    assert_eq!(values(&watch), old);
    assert_eq!(watch.next_revision(), old_next);
    assert_eq!(advance(&mut watch, &token).await, boundary.revision + 1);
    assert_eq!(
        values(&watch),
        vec![(b"/meta/d".to_vec(), b"recovered".to_vec())]
    );
    assert_eq!(
        watch.entries()[b"/meta/d".as_slice()].mod_revision,
        deleted.revision
    );
    let live = store
        .transact(vec![], vec![Mutation::put(b"/meta/e", b"after")])
        .await
        .unwrap();
    assert_eq!(advance(&mut watch, &token).await, live.revision + 1);
    assert_eq!(history.next_index(), HistoryIndex(700));

    // Cancel an actually entered watch, await the host worker, then verify terminality.
    store.observe_wait.store(true, Ordering::SeqCst);
    let worker_token = token.clone();
    let worker = tokio::spawn(async move {
        let result = watch.advance(&worker_token).await;
        (watch, result)
    });
    timeout(Duration::from_secs(8), store.entered.notified())
        .await
        .unwrap();
    token.cancel();
    let (mut watch, result) = timeout(Duration::from_secs(8), worker)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result, Err(StoreError::Cancelled));
    assert_eq!(
        watch.advance(&CancellationToken::new()).await,
        Err(StoreError::Cancelled)
    );
    assert_eq!(store.active.load(Ordering::SeqCst), 0);
    assert_eq!(history.next_index(), HistoryIndex(700));
}
#[tokio::test]
async fn state_task_39_metadata_watch_memory() {
    run(Arc::new(MemoryStore::default())).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn state_task_39_metadata_watch_etcd() {
    let fixture = crate::persist_fixture::Fixture::start();
    run(Arc::new(fixture.store().await)).await;
}

async fn empty_and_compacted_page(inner: Arc<dyn MetaStore>) {
    let store = controlled(inner);
    let token = CancellationToken::new();
    let mut empty = MetadataWatch::load(store.clone(), b"/empty/".to_vec(), 1, &token)
        .await
        .unwrap();
    assert!(empty.entries().is_empty());
    let commit = store
        .transact(vec![], vec![Mutation::put(b"/empty/a", b"one")])
        .await
        .unwrap();
    assert_eq!(advance(&mut empty, &token).await, commit.revision + 1);
    assert_eq!(empty.entries().len(), 1);
    let seed = store
        .transact(
            vec![],
            vec![
                Mutation::put(b"/pages/a", b"old"),
                Mutation::put(b"/pages/b", b"old"),
                Mutation::put(b"/pages/c", b"old"),
            ],
        )
        .await
        .unwrap();
    *store.after_read.lock().unwrap() = Some((vec![Mutation::put(b"/pages/a", b"new")], true));
    let result = MetadataWatch::load(store.clone(), b"/pages/".to_vec(), 1, &token).await;
    assert!(
        matches!(result, Err(StoreError::Compacted{ requested, .. }) if requested == seed.revision)
    );
    let retry = MetadataWatch::load(store.clone(), b"/pages/".to_vec(), 1, &token)
        .await
        .unwrap();
    assert_eq!(retry.entries().len(), 3);
    assert_eq!(retry.entries()[b"/pages/a".as_slice()].value, b"new");
    token.cancel();
    assert!(matches!(
        MetadataWatch::load(store, b"/pages/".to_vec(), 1, &token).await,
        Err(StoreError::Cancelled)
    ));
}
#[tokio::test]
async fn state_task_39_metadata_watch_memory_empty_and_pages() {
    empty_and_compacted_page(Arc::new(MemoryStore::default())).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn state_task_39_metadata_watch_etcd_empty_and_pages() {
    let fixture = crate::persist_fixture::Fixture::start();
    empty_and_compacted_page(Arc::new(fixture.store().await)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn state_task_39_metadata_watch_etcd_restart() {
    let mut fixture = crate::persist_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let token = CancellationToken::new();
    let mut watch = MetadataWatch::load(store.clone(), b"/restart/".to_vec(), 1, &token)
        .await
        .unwrap();
    let first = store
        .transact(vec![], vec![Mutation::put(b"/restart/a", b"one")])
        .await
        .unwrap();
    assert_eq!(advance(&mut watch, &token).await, first.revision + 1);
    fixture.command("stop", "stopped");
    let before = values(&watch);
    assert_eq!(
        timeout(Duration::from_secs(8), watch.advance(&token))
            .await
            .unwrap(),
        Err(StoreError::Unavailable)
    );
    assert_eq!(values(&watch), before);
    assert_eq!(watch.next_revision(), first.revision + 1);
    fixture.command("start", "restarted");
    let restored = fixture.store().await;
    let committed = restored
        .transact(
            vec![],
            vec![
                Mutation::Delete {
                    key: b"/restart/a".to_vec(),
                },
                Mutation::put(b"/restart/b", b"two"),
            ],
        )
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        match tokio::time::timeout_at(deadline, watch.advance(&token))
            .await
            .unwrap()
        {
            Ok(revision) => {
                assert_eq!(revision, committed.revision + 1);
                break;
            }
            Err(StoreError::Unavailable) => tokio::time::sleep(Duration::from_millis(100)).await,
            Err(error) => panic!("unexpected restart error: {error}"),
        }
    }
    assert_eq!(
        values(&watch),
        vec![(b"/restart/b".to_vec(), b"two".to_vec())]
    );
    token.cancel();
    assert_eq!(watch.advance(&token).await, Err(StoreError::Cancelled));
}
