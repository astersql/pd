// Copyright 2026 AsterSQL.
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use astersql_pd_storage_kv::*;
use tokio::{
    sync::Notify,
    time::{Instant, timeout},
};
use tokio_util::sync::CancellationToken;

use super::*;

// Hold or discard only a real backend IO response; all storage results stay real.
struct GateStore {
    inner: Arc<dyn MetaStore>,
    gate: AtomicBool,
    fail: AtomicBool,
    entered: Notify,
    release: Notify,
}
#[async_trait::async_trait]
impl MetaStore for GateStore {
    async fn read(&self, request: Read) -> Result<Snapshot> {
        let page = self.inner.read(request).await?;
        if self.gate.swap(false, Ordering::SeqCst) {
            self.entered.notify_one();
            self.release.notified().await;
        }
        if self.fail.swap(false, Ordering::SeqCst) {
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

async fn bounded<T>(future: impl std::future::Future<Output = T>) -> T {
    timeout(Duration::from_secs(8), future)
        .await
        .expect("worker or notification stalled")
}

async fn run(store: Arc<dyn MetaStore>) {
    store
        .transact(vec![], vec![Mutation::put(b"/control/a", b"initial")])
        .await
        .unwrap();
    let cancel = CancellationToken::new();
    let (handle, mut worker) = ControlledWatch::new(store.clone(), b"/control/".to_vec(), true, 1);
    let mut attempts = 0;
    bounded(worker.initialize(&cancel, |snapshot| {
        attempts += 1;
        assert_eq!(snapshot.entries[0].value, b"initial");
        if attempts == 1 {
            Err(StoreError::Backend)
        } else {
            Ok(())
        }
    }))
    .await
    .expect("initial publication must retry before reporting completion");
    assert_eq!(attempts, 2);
    assert_eq!(bounded(handle.wait_load(&cancel)).await, Ok(()));
    assert_eq!(
        worker
            .initialize(&cancel, |_| panic!("published initialization twice"))
            .await,
        Ok(())
    );

    // WaitLoad is one-shot, even when ForceLoad is serviced afterwards.
    let waiting = CancellationToken::new();
    let child = waiting.clone();
    let waiter_handle = handle.clone();
    let entered = Arc::new(Notify::new());
    let signal = entered.clone();
    let waiter = tokio::spawn(async move {
        signal.notify_one();
        waiter_handle.wait_load(&child).await
    });
    bounded(entered.notified()).await;
    assert!(!waiter.is_finished());

    let clock = Instant::now() + Duration::from_secs(2);
    handle.force_load_at(clock);
    handle.force_load_at(clock); // rate-limited
    handle.force_load_at(clock + Duration::from_secs(1)); // coalesced full slot
    assert_eq!(
        bounded(worker.apply_next(
            &cancel,
            |_| Err(StoreError::Backend),
            |_| panic!("force became a live event")
        ))
        .await,
        Err(StoreError::Backend)
    );
    // Failed force must retain the old watch and its resume revision.
    let revision = store
        .transact(vec![], vec![Mutation::put(b"/control/a", b"live")])
        .await
        .unwrap()
        .revision;
    assert_eq!(
        bounded(worker.apply_next(
            &cancel,
            |_| panic!("duplicate forced request"),
            |batch| {
                assert_eq!(batch.revision, revision);
                match &batch.events[0] {
                    Event::Put(entry) => {
                        assert_eq!(entry.value, b"live");
                        assert_eq!(entry.mod_revision, revision);
                    }
                    _ => panic!("expected live Put"),
                }
                Ok(())
            }
        ))
        .await,
        Ok(revision + 1)
    );

    // A request arriving while the host is awaiting a live event wakes that wait.
    let child = cancel.clone();
    let task = tokio::spawn(async move {
        let result = worker
            .apply_next(
                &child,
                |snapshot| {
                    assert_eq!(snapshot.entries[0].value, b"live");
                    Ok(())
                },
                |_| panic!("no new live transaction"),
            )
            .await;
        (result, worker)
    });
    handle.force_load_at(clock + Duration::from_secs(2));
    let (result, mut worker) = bounded(task).await.unwrap();
    assert_eq!(result, Ok(revision + 1));
    assert!(
        !waiter.is_finished(),
        "forced load incorrectly completed WaitLoad twice"
    );
    waiting.cancel();
    assert_eq!(bounded(waiter).await.unwrap(), Err(StoreError::Cancelled));
    cancel.cancel();
    assert_eq!(
        worker.apply_next(&cancel, |_| panic!(), |_| panic!()).await,
        Err(StoreError::Cancelled)
    );
    assert_eq!(
        worker
            .apply_next(&CancellationToken::new(), |_| panic!(), |_| panic!())
            .await,
        Err(StoreError::Cancelled)
    );

    // Exhaustion reports the final error to both the caller and initial waiter.
    let token = CancellationToken::new();
    let (handle, mut worker) = ControlledWatch::new(store.clone(), b"/absent".to_vec(), false, 0);
    let mut attempts = 0;
    assert_eq!(
        bounded(worker.initialize(&token, |snapshot| {
            assert!(snapshot.entries.is_empty());
            attempts += 1;
            Err(if attempts == 3 {
                StoreError::Unavailable
            } else {
                StoreError::Backend
            })
        }))
        .await,
        Err(StoreError::Unavailable)
    );
    assert_eq!(attempts, 3);
    assert_eq!(
        bounded(handle.wait_load(&token)).await,
        Err(StoreError::Unavailable)
    );

    // Notification barrier proves cancellation occurs after the failed first attempt.
    let (handle, mut worker) = ControlledWatch::new(store.clone(), b"/absent".to_vec(), false, 0);
    let reached = Arc::new(Notify::new());
    let signal = reached.clone();
    let token = CancellationToken::new();
    let child = token.clone();
    let work = tokio::spawn(async move {
        worker
            .initialize(&child, |_| {
                signal.notify_one();
                Err(StoreError::Backend)
            })
            .await
    });
    bounded(reached.notified()).await;
    token.cancel();
    assert_eq!(bounded(work).await.unwrap(), Err(StoreError::Cancelled));
    assert_eq!(
        bounded(handle.wait_load(&CancellationToken::new())).await,
        Err(StoreError::Cancelled)
    );

    // Cancelled waiter must not consume the pending successful empty snapshot result.
    let (handle, mut worker) = ControlledWatch::new(store.clone(), b"/absent".to_vec(), false, 0);
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert_eq!(
        handle.wait_load(&cancelled).await,
        Err(StoreError::Cancelled)
    );
    worker
        .initialize(&CancellationToken::new(), |snapshot| {
            assert!(snapshot.entries.is_empty());
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(handle.wait_load(&CancellationToken::new()).await, Ok(()));
    // A lost real read response retries without exposing a partial snapshot.
    let gate = Arc::new(GateStore {
        inner: store,
        gate: AtomicBool::new(false),
        fail: AtomicBool::new(true),
        entered: Notify::new(),
        release: Notify::new(),
    });
    let token = CancellationToken::new();
    let (control, mut observer) =
        ControlledWatch::new(gate.clone(), b"/control/".to_vec(), true, 1);
    let mut publications = 0;
    bounded(observer.initialize(&token, |snapshot| {
        publications += 1;
        assert_eq!(snapshot.entries[0].value, b"live");
        Ok(())
    }))
    .await
    .unwrap();
    assert_eq!(publications, 1);
    assert_eq!(control.wait_load(&token).await, Ok(()));
    // Cancel while a forced full read is actually in flight, then join host task.
    gate.gate.store(true, Ordering::SeqCst);
    control.force_load_at(Instant::now() + Duration::from_secs(2));
    let child = token.clone();
    let task = tokio::spawn(async move {
        assert_eq!(
            observer
                .apply_next(&child, |_| panic!("cancelled IO published"), |_| panic!())
                .await,
            Err(StoreError::Cancelled)
        );
        assert_eq!(
            observer
                .apply_next(&CancellationToken::new(), |_| panic!(), |_| panic!())
                .await,
            Err(StoreError::Cancelled)
        );
    });
    bounded(gate.entered.notified()).await;
    token.cancel();
    bounded(task).await.unwrap();
    eprintln!(
        "watch_control: initial retry, final error, empty snapshot, one-shot WaitLoad, forced replacement error/success, live resume, rate/coalescing, cancellation and host join"
    );
}

#[tokio::test]
async fn watch_control_memory() {
    run(Arc::new(MemoryStore::default())).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn watch_control_etcd() {
    let fixture = fixture::Fixture::start();
    run(Arc::new(fixture.store().await)).await;
}

#[tokio::test(start_paused = true)]
async fn watch_control_initial_force_interval() {
    let cancel = CancellationToken::new();
    let (handle, mut worker) = ControlledWatch::new(
        Arc::new(MemoryStore::default()),
        b"/empty".to_vec(),
        false,
        0,
    );
    worker.initialize(&cancel, |_| Ok(())).await.unwrap();
    handle.force_load(); // suppressed during the first second after construction
    let stopped = cancel.clone();
    let reached = Arc::new(Notify::new());
    let signal = reached.clone();
    let task = tokio::spawn(async move {
        signal.notify_one();
        worker.apply_next(&stopped, |_| Ok(()), |_| panic!()).await
    });
    reached.notified().await;
    tokio::task::yield_now().await;
    assert!(
        !task.is_finished(),
        "construction-time force must be throttled"
    );
    tokio::time::advance(Duration::from_secs(1)).await;
    handle.force_load();
    assert!(task.await.unwrap().is_ok());
}
