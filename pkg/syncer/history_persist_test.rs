// Copyright 2026 AsterSQL.
use super::{
    HISTORY_KEY, HistoryBuffer, HistoryIndex as I, HistoryPersistError, PersistentHistory,
};
use astersql_pd_core::{RegionInfo, RegionMeta};
use astersql_pd_storage_kv::{Base, MemoryStore, MetaStore};
use std::sync::Arc;

fn buffer() -> HistoryBuffer {
    HistoryBuffer::with_config(100, 200, 100).unwrap()
}
fn region(id: u64) -> Arc<RegionInfo> {
    Arc::new(RegionInfo::new(
        RegionMeta {
            id,
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    ))
}

async fn checkpoint_contract(store: &dyn MetaStore) {
    let base = Base::new(store);
    let mut h = PersistentHistory::load(buffer(), store).await.unwrap();
    for id in 0..6 {
        h.record(region(id)).await.unwrap();
    }
    assert_eq!(h.history().next_index(), I(6));
    h.persist().await.unwrap();
    assert_eq!(base.load(HISTORY_KEY).await.unwrap(), b"6");
    drop(h);
    let mut h = PersistentHistory::load(buffer(), store).await.unwrap();
    assert_eq!(h.history().first_index(), I(6));
    assert_eq!(h.history().next_index(), I(6));
    assert!(h.history().is_empty());
    assert!(h.history().get(I(5)).is_none());
    assert!(h.history().retained_records_from(I(5)).is_err());
    for id in 0..101 {
        h.record(region(id)).await.unwrap();
    }
    assert_eq!(h.history().next_index(), I(107));
    assert_eq!(h.history().first_index(), I(7));
    assert_eq!(base.load(HISTORY_KEY).await.unwrap(), b"106");
    assert_eq!(
        h.history()
            .records_from(I(7))
            .iter()
            .map(|r| r.meta().id)
            .collect::<Vec<_>>(),
        (1..101).collect::<Vec<_>>()
    );
    drop(h);
    let h = PersistentHistory::load(buffer(), store).await.unwrap();
    assert_eq!(h.history().next_index(), I(106));
    assert!(h.history().is_empty());
}

#[tokio::test]
async fn state_task_32_history_persist_memory_go_snapshot() {
    checkpoint_contract(&MemoryStore::default()).await;
}

#[tokio::test]
async fn state_task_32_history_persist_etcd_go_snapshot() {
    let fixture = super::persist_fixture::Fixture::start();
    let store = fixture.store().await;
    checkpoint_contract(&store).await;
}

#[tokio::test]
async fn state_task_32_history_persist_reset_and_flush_boundary() {
    let store = MemoryStore::default();
    let base = Base::new(&store);
    let mut h = PersistentHistory::load(buffer(), &store).await.unwrap();
    for id in 0..99 {
        h.record(region(id)).await.unwrap();
    }
    assert!(base.load(HISTORY_KEY).await.unwrap().is_empty());
    h.persist().await.unwrap(); // explicit persist must not restart countdown
    h.record(region(99)).await.unwrap();
    assert_eq!(base.load(HISTORY_KEY).await.unwrap(), b"100");
    h.reset_with_index_and_persist(I(900)).await.unwrap();
    assert!(h.history().is_empty());
    assert_eq!(base.load(HISTORY_KEY).await.unwrap(), b"900");
    for id in 0..99 {
        h.record(region(id)).await.unwrap();
    }
    assert_eq!(base.load(HISTORY_KEY).await.unwrap(), b"900");
    h.record(region(99)).await.unwrap();
    assert_eq!(base.load(HISTORY_KEY).await.unwrap(), b"1000");
    h.reset_with_index(I(7));
    assert_eq!(base.load(HISTORY_KEY).await.unwrap(), b"1000");
}

#[tokio::test]
async fn state_task_32_history_persist_decimal_and_corruption() {
    let store = MemoryStore::default();
    let base = Base::new(&store);
    for (value, expected) in [("", 0), ("0003", 3), ("18446744073709551615", u64::MAX)] {
        base.save(HISTORY_KEY, value.as_bytes()).await.unwrap();
        let mut seeded = buffer();
        seeded.record(region(1)).unwrap();
        let h = PersistentHistory::load(seeded, &store).await.unwrap();
        assert_eq!(h.history().next_index(), I(expected));
        assert!(h.history().is_empty());
    }
    for value in [
        b"-1".as_slice(),
        b"+1",
        b" 1",
        b"1\n",
        b"1_0",
        b"0x10",
        b"18446744073709551616",
        b"\xff",
    ] {
        base.save(HISTORY_KEY, value).await.unwrap();
        assert!(
            matches!(
                PersistentHistory::load(buffer(), &store).await,
                Err(HistoryPersistError::CorruptIndex)
            ),
            "value={value:?}"
        );
    }
}

#[tokio::test]
async fn state_task_32_history_persist_etcd_failure_and_restart() {
    let mut fixture = super::persist_fixture::Fixture::start();
    let store = fixture.store().await;
    let mut h = PersistentHistory::load(buffer(), &store).await.unwrap();
    h.reset_with_index_and_persist(I(3)).await.unwrap();
    for id in 0..99 {
        h.record(region(id)).await.unwrap();
    }
    fixture.command("stop", "stopped");
    assert!(matches!(
        h.record(region(99)).await,
        Err(HistoryPersistError::Save { next: I(103), .. })
    ));
    assert_eq!(h.history().next_index(), I(103));
    assert_eq!(h.history().len(), 100);
    // Failed automatic flush still restarts countdown, as in Go.
    h.record(region(100)).await.unwrap();
    assert!(matches!(
        h.reset_with_index_and_persist(I(800)).await,
        Err(HistoryPersistError::Save { next: I(800), .. })
    ));
    assert!(h.history().is_empty());
    assert!(matches!(
        PersistentHistory::load(buffer(), &store).await,
        Err(HistoryPersistError::Load(_))
    ));
    fixture.command("start", "restarted");
    let restored = PersistentHistory::load(buffer(), &store).await.unwrap();
    assert_eq!(restored.history().next_index(), I(3));
    drop(restored);
    h.persist().await.unwrap();
    let restored = PersistentHistory::load(buffer(), &store).await.unwrap();
    assert_eq!(restored.history().next_index(), I(800));
    assert!(restored.history().is_empty());
}

#[tokio::test]
async fn state_task_32_history_persist_next_index_only_and_exhaustion() {
    let store = MemoryStore::default();
    let mut h = PersistentHistory::load(HistoryBuffer::with_config(4, 8, 1).unwrap(), &store)
        .await
        .unwrap();
    for id in 1..=3 {
        h.record(region(id)).await.unwrap();
    }
    h.persist().await.unwrap();
    drop(h);
    let mut h = PersistentHistory::load(buffer(), &store).await.unwrap();
    assert_eq!(h.history().first_index(), I(3));
    assert_eq!(h.history().next_index(), I(3));
    assert!(h.history().get(I(2)).is_none());
    assert!(h.history().is_empty());
    assert_eq!(Base::new(&store).load(HISTORY_KEY).await.unwrap(), b"3");
    h.reset_with_index_and_persist(I(u64::MAX)).await.unwrap();
    assert!(matches!(
        h.record(region(4)).await,
        Err(HistoryPersistError::History(
            super::HistoryError::IndexExhausted
        ))
    ));
    assert_eq!(h.history().next_index(), I(u64::MAX));
    assert!(h.history().is_empty());
}

// Only the backend I/O boundary is paused; all unpaused operations use MemoryStore.
struct PausedStore {
    inner: MemoryStore,
    paused: std::sync::atomic::AtomicBool,
}
#[async_trait::async_trait]
impl MetaStore for PausedStore {
    async fn read(
        &self,
        request: astersql_pd_storage_kv::Read,
    ) -> astersql_pd_storage_kv::Result<astersql_pd_storage_kv::Snapshot> {
        self.inner.read(request).await
    }
    async fn transact(
        &self,
        compares: Vec<astersql_pd_storage_kv::Compare>,
        writes: Vec<astersql_pd_storage_kv::Mutation>,
    ) -> astersql_pd_storage_kv::Result<astersql_pd_storage_kv::Commit> {
        if self.paused.load(std::sync::atomic::Ordering::SeqCst) {
            std::future::pending::<()>().await;
        }
        self.inner.transact(compares, writes).await
    }
    async fn transact_branches(
        &self,
        compares: Vec<astersql_pd_storage_kv::Compare>,
        then_writes: Vec<astersql_pd_storage_kv::Mutation>,
        else_writes: Vec<astersql_pd_storage_kv::Mutation>,
    ) -> astersql_pd_storage_kv::Result<astersql_pd_storage_kv::Commit> {
        self.inner
            .transact_branches(compares, then_writes, else_writes)
            .await
    }
    async fn transact_raw(
        &self,
        compares: Vec<astersql_pd_storage_kv::Compare>,
        then_ops: Vec<astersql_pd_storage_kv::RawTxnOp>,
        else_ops: Vec<astersql_pd_storage_kv::RawTxnOp>,
    ) -> astersql_pd_storage_kv::Result<astersql_pd_storage_kv::RawTxnResponse> {
        self.inner.transact_raw(compares, then_ops, else_ops).await
    }
    async fn watch(
        &self,
        prefix: Vec<u8>,
        start_revision: i64,
    ) -> astersql_pd_storage_kv::Result<Box<dyn astersql_pd_storage_kv::MetaWatch>> {
        self.inner.watch(prefix, start_revision).await
    }
    async fn grant(
        &self,
        ttl_seconds: i64,
    ) -> astersql_pd_storage_kv::Result<astersql_pd_storage_kv::Lease> {
        self.inner.grant(ttl_seconds).await
    }
    async fn keep_alive(
        &self,
        lease: i64,
    ) -> astersql_pd_storage_kv::Result<astersql_pd_storage_kv::Lease> {
        self.inner.keep_alive(lease).await
    }
    async fn revoke(&self, lease: i64) -> astersql_pd_storage_kv::Result<()> {
        self.inner.revoke(lease).await
    }
    async fn ttl(&self, lease: i64) -> astersql_pd_storage_kv::Result<i64> {
        self.inner.ttl(lease).await
    }
    async fn compact(&self, revision: i64) -> astersql_pd_storage_kv::Result<()> {
        self.inner.compact(revision).await
    }
}

#[tokio::test]
async fn state_task_32_history_persist_cancel_write() {
    use std::{
        future::Future,
        sync::atomic::Ordering,
        task::{Context, Poll, Waker},
    };
    let store = PausedStore {
        inner: MemoryStore::default(),
        paused: false.into(),
    };
    let mut h = PersistentHistory::load(buffer(), &store).await.unwrap();
    for id in 0..99 {
        h.record(region(id)).await.unwrap();
    }
    store.paused.store(true, Ordering::SeqCst);
    {
        let mut future = std::pin::pin!(h.record(region(99)));
        assert!(matches!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Pending
        ));
    } // Drops the pending backend future. No detached task exists.
    assert_eq!(h.history().next_index(), I(100));
    assert_eq!(h.history().len(), 100);
    h.record(region(100)).await.unwrap();
    assert!(
        Base::new(&store.inner)
            .load(HISTORY_KEY)
            .await
            .unwrap()
            .is_empty()
    );
    store.paused.store(false, Ordering::SeqCst);
    h.persist().await.unwrap();
    let restored = PersistentHistory::load(buffer(), &store).await.unwrap();
    assert_eq!(restored.history().next_index(), I(101));
    assert!(restored.history().is_empty());
}

#[tokio::test]
async fn state_task_32_history_persist_adaptive_window_reset() {
    let store = MemoryStore::default();
    let mut h = PersistentHistory::load(HistoryBuffer::with_config(2, 8, 1).unwrap(), &store)
        .await
        .unwrap();
    h.reset_with_index_and_persist(I(10)).await.unwrap();
    let guard = h.retain_from(I(10));
    for id in 0..5 {
        h.record(region(id)).await.unwrap();
    }
    assert_eq!(h.history().capacity(), 8);
    assert_eq!(h.history().retained_records_from(I(10)).unwrap().0.len(), 5);
    h.persist().await.unwrap();
    h.reset_with_index_and_persist(I(15)).await.unwrap();
    drop(guard);
    assert_eq!(h.history().capacity(), 2);
    assert!(h.history().is_empty());
    h.observe_required_window(3).unwrap();
    assert_eq!(h.history().capacity(), 8);
    h.maybe_shrink().unwrap();
    for _ in 0..3 {
        h.maybe_shrink().unwrap();
    }
    assert_eq!(h.history().capacity(), 4);
    assert_eq!(Base::new(&store).load(HISTORY_KEY).await.unwrap(), b"15");
}
