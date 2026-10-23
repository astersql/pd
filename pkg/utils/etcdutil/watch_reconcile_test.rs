// Copyright 2026 AsterSQL.
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use astersql_pd_storage_kv::*;
use tokio::{sync::Notify, time::timeout};
use tokio_util::sync::CancellationToken;

use super::*;

// Gate only completion of a real read; never invent transaction/snapshot results.
struct GateStore {
    inner: Arc<dyn MetaStore>,
    gate: AtomicBool,
    fail_read: AtomicUsize,
    disconnect: Arc<AtomicBool>,
    entered: Notify,
    release: Notify,
}
impl GateStore {
    fn new(inner: Arc<dyn MetaStore>) -> Self {
        Self {
            inner,
            gate: AtomicBool::new(false),
            fail_read: AtomicUsize::new(0),
            disconnect: Arc::new(AtomicBool::new(false)),
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
        if self
            .fail_read
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            == Ok(1)
        {
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
        Ok(Box::new(GateWatch {
            inner: self.inner.watch(prefix, start_revision).await?,
            disconnect: self.disconnect.clone(),
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

struct GateWatch {
    inner: Box<dyn MetaWatch>,
    disconnect: Arc<AtomicBool>,
}
#[async_trait::async_trait]
impl MetaWatch for GateWatch {
    async fn next(&mut self, cancel: &CancellationToken) -> Result<WatchBatch> {
        if self.disconnect.swap(false, Ordering::SeqCst) {
            return Err(StoreError::Unavailable);
        }
        self.inner.next(cancel).await
    }
}

fn publish(cache: &mut BTreeMap<Vec<u8>, Vec<u8>>, batch: &WatchBatch) -> usize {
    let mut deletes = 0;
    for event in &batch.events {
        match event {
            Event::Put(entry) => {
                cache.insert(entry.key.clone(), entry.value.clone());
            }
            Event::Delete { key, .. } => {
                assert!(cache.remove(key).is_some(), "duplicate deletion of {key:?}");
                deletes += 1;
            }
        }
    }
    deletes
}

async fn compact(store: &dyn MetaStore) -> Revision {
    // Strictly beyond the observer's start, including etcd's inclusive boundary.
    store
        .transact(vec![], vec![Mutation::put(b"/outside", b"1")])
        .await
        .unwrap();
    let boundary = store
        .transact(vec![], vec![Mutation::put(b"/outside", b"2")])
        .await
        .unwrap()
        .revision;
    store.compact(boundary).await.unwrap();
    boundary
}

async fn run(inner: Arc<dyn MetaStore>) {
    let store = Arc::new(GateStore::new(inner));
    let cancel = CancellationToken::new();
    store
        .transact(
            vec![],
            vec![
                Mutation::put(b"/reconcile/a", b"old"),
                Mutation::put(b"/reconcile/b", b"gone"),
            ],
        )
        .await
        .unwrap();
    let (snapshot, mut watch) =
        ReconciledWatch::load(store.clone(), b"/reconcile/".to_vec(), true, 1, &cancel)
            .await
            .unwrap();
    let mut cache: BTreeMap<_, _> = snapshot
        .entries
        .iter()
        .map(|e| (e.key.clone(), e.value.clone()))
        .collect();
    let original = cache.clone();
    let resume = watch.next_revision();
    store
        .transact(
            vec![],
            vec![
                Mutation::Delete {
                    key: b"/reconcile/b".to_vec(),
                },
                Mutation::put(b"/reconcile/a", b"new"),
                Mutation::put(b"/reconcile/c", b"added"),
                Mutation::put(b"/reconcile/e", b"third-page-key"),
            ],
        )
        .await
        .unwrap();
    compact(store.as_ref()).await;
    // Lose the second page of a real reload: no partial batch may publish.
    store.fail_read.store(2, Ordering::SeqCst);
    assert_eq!(
        timeout(
            Duration::from_secs(5),
            watch.apply_next(&cancel, |_| panic!("partial snapshot published"))
        )
        .await
        .unwrap(),
        Err(StoreError::Unavailable)
    );
    assert_eq!(cache, original);
    assert_eq!(watch.next_revision(), resume);
    // Atomic publication failure leaves the published cache and cursor untouched.
    assert_eq!(
        timeout(
            Duration::from_secs(5),
            watch.apply_next(&cancel, |batch| {
                let mut staging = cache.clone();
                publish(&mut staging, batch);
                Err(StoreError::Backend)
            })
        )
        .await
        .unwrap(),
        Err(StoreError::Backend)
    );
    assert_eq!(cache, original);
    assert_eq!(watch.next_revision(), resume);
    let mut deleted = 0;
    let mut reload_revision = 0;
    timeout(
        Duration::from_secs(5),
        watch.apply_next(&cancel, |batch| {
            reload_revision = batch.revision;
            deleted += publish(&mut cache, batch);
            Ok(())
        }),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(deleted, 1, "reload must reconcile the absent key");
    assert_eq!(
        cache,
        BTreeMap::from([
            (b"/reconcile/a".to_vec(), b"new".to_vec()),
            (b"/reconcile/c".to_vec(), b"added".to_vec()),
            (b"/reconcile/e".to_vec(), b"third-page-key".to_vec())
        ])
    );
    assert_eq!(watch.next_revision(), reload_revision + 1);
    // Repeated compaction cannot delete b twice, even for a non-idempotent sink.
    compact(store.as_ref()).await;
    timeout(
        Duration::from_secs(5),
        watch.apply_next(&cancel, |batch| {
            deleted += publish(&mut cache, batch);
            Ok(())
        }),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(deleted, 1);
    // Live puts/deletes must update the same ledger used by later reloads.
    store
        .transact(vec![], vec![Mutation::put(b"/reconcile/d", b"live")])
        .await
        .unwrap();
    let resume = watch.next_revision();
    // A publication error with the same variant as backend compaction must
    // retain the pending live batch; it must not silently switch to a reload.
    let publication_error = StoreError::Compacted {
        requested: resume,
        compact_revision: None,
    };
    assert_eq!(
        timeout(
            Duration::from_secs(5),
            watch.apply_next(&cancel, |_| Err(publication_error.clone()))
        )
        .await
        .unwrap(),
        Err(publication_error)
    );
    assert_eq!(watch.next_revision(), resume);
    timeout(
        Duration::from_secs(5),
        watch.apply_next(&cancel, |batch| {
            publish(&mut cache, batch);
            Ok(())
        }),
    )
    .await
    .unwrap()
    .unwrap();
    store
        .transact(
            vec![],
            vec![Mutation::Delete {
                key: b"/reconcile/c".to_vec(),
            }],
        )
        .await
        .unwrap();
    timeout(
        Duration::from_secs(5),
        watch.apply_next(&cancel, |batch| {
            deleted += publish(&mut cache, batch);
            Ok(())
        }),
    )
    .await
    .unwrap()
    .unwrap();
    store
        .transact(
            vec![],
            vec![
                Mutation::Delete {
                    key: b"/reconcile/d".to_vec(),
                },
                Mutation::Delete {
                    key: b"/reconcile/a".to_vec(),
                },
                Mutation::Delete {
                    key: b"/reconcile/e".to_vec(),
                },
            ],
        )
        .await
        .unwrap();
    compact(store.as_ref()).await;
    // Disconnect the active IO stream so the next call must recover from a
    // genuinely compacted cursor rather than consume already buffered events.
    store.disconnect.store(true, Ordering::SeqCst);
    assert_eq!(
        watch
            .apply_next(&cancel, |_| panic!("disconnected publication"))
            .await,
        Err(StoreError::Unavailable)
    );
    timeout(
        Duration::from_secs(5),
        watch.apply_next(&cancel, |batch| {
            assert!(
                batch
                    .events
                    .iter()
                    .all(|event| matches!(event, Event::Delete { .. }))
            );
            deleted += publish(&mut cache, batch);
            Ok(())
        }),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(cache.is_empty());
    assert_eq!(deleted, 5);
    cancel.cancel();
    assert_eq!(
        watch
            .apply_next(&cancel, |_| panic!("cancelled publication"))
            .await,
        Err(StoreError::Cancelled)
    );
    assert_eq!(
        watch
            .apply_next(&CancellationToken::new(), |_| panic!("revived observer"))
            .await,
        Err(StoreError::Cancelled)
    );
    // Exact-key reload must not treat a prefixed neighbor as a surviving key.
    let token = CancellationToken::new();
    store
        .transact(
            vec![],
            vec![
                Mutation::put(b"/exact/a", b"old"),
                Mutation::put(b"/exact/a-neighbor", b"outside"),
            ],
        )
        .await
        .unwrap();
    let (_, mut exact) =
        ReconciledWatch::load(store.clone(), b"/exact/a".to_vec(), false, 0, &token)
            .await
            .unwrap();
    store
        .transact(
            vec![],
            vec![Mutation::Delete {
                key: b"/exact/a".to_vec(),
            }],
        )
        .await
        .unwrap();
    let boundary = compact(store.as_ref()).await;
    timeout(
        Duration::from_secs(5),
        exact.apply_next(&token, |batch| {
            assert_eq!(batch.revision, boundary);
            assert_eq!(
                batch.events,
                vec![Event::Delete {
                    key: b"/exact/a".to_vec(),
                    revision: boundary
                }]
            );
            Ok(())
        }),
    )
    .await
    .unwrap()
    .unwrap();
    // Cancellation while a real reload response is held must end without publication.
    store
        .transact(vec![], vec![Mutation::put(b"/cancel/a", b"old")])
        .await
        .unwrap();
    let (_, mut blocked) =
        ReconciledWatch::load(store.clone(), b"/cancel/".to_vec(), true, 1, &token)
            .await
            .unwrap();
    let resume = blocked.next_revision();
    store
        .transact(
            vec![],
            vec![Mutation::Delete {
                key: b"/cancel/a".to_vec(),
            }],
        )
        .await
        .unwrap();
    compact(store.as_ref()).await;
    store.gate.store(true, Ordering::SeqCst);
    let child_token = token.clone();
    let work = tokio::spawn(async move {
        assert_eq!(
            blocked
                .apply_next(&child_token, |_| panic!("cancelled reload published"))
                .await,
            Err(StoreError::Cancelled)
        );
        assert_eq!(blocked.next_revision(), resume);
        assert_eq!(
            blocked
                .apply_next(&CancellationToken::new(), |_| panic!(
                    "revived cancelled reload"
                ))
                .await,
            Err(StoreError::Cancelled)
        );
    });
    timeout(Duration::from_secs(5), store.entered.notified())
        .await
        .unwrap();
    token.cancel();
    timeout(Duration::from_secs(2), work)
        .await
        .unwrap()
        .unwrap();
    eprintln!(
        "watch_reconcile: failed second-page IO and publication unchanged, missing-key deletion, repeated reload, live ledger, empty/exact snapshot, in-flight cancellation/join"
    );
}

#[tokio::test]
async fn watch_reconcile_memory() {
    run(Arc::new(MemoryStore::default())).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn watch_reconcile_etcd() {
    let fixture = fixture::Fixture::start();
    run(Arc::new(fixture.store().await)).await;
}
