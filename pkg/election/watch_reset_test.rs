// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_storage_kv::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{sync::Notify, time::timeout};
use tokio_util::sync::CancellationToken;

// Gate only lease-revoke IO; all transactions, snapshots and watches are real.
struct GateStore {
    inner: Arc<dyn MetaStore>,
    block_revoke: AtomicBool,
    revoke_entered: Notify,
    revoke_release: Notify,
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
        self.inner.keep_alive(id).await
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
        self.inner.grant(ttl).await
    }
}

async fn update(observer: &mut LeaderObserver, cancel: &CancellationToken) -> LeaderUpdate {
    timeout(Duration::from_secs(5), observer.next(cancel))
        .await
        .expect("observation deadline")
        .unwrap()
}
async fn put(store: &dyn MetaStore, key: &[u8], value: &[u8]) -> Revision {
    store
        .transact(vec![], vec![Mutation::put(key, value)])
        .await
        .unwrap()
        .revision
}
async fn observe_behavior(store: Arc<dyn MetaStore>) {
    let key = b"/reset-watch/leader";
    let cancel = CancellationToken::new();
    let initial = put(store.as_ref(), key, b"initial").await;
    put(store.as_ref(), b"/reset-watch/leader-neighbor", b"neighbor").await;
    let mut observer = LeaderObserver::new(store.clone(), key.to_vec()).unwrap();
    let LeaderUpdate::Snapshot(snapshot) = update(&mut observer, &cancel).await else {
        panic!("initial snapshot required")
    };
    assert_eq!(snapshot.entries.len(), 1);
    assert_eq!(snapshot.entries[0].mod_revision, initial);
    // Deterministic gap: write after snapshot delivery, before opening watch.
    let gap = put(store.as_ref(), key, b"gap").await;
    assert!(
        matches!(update(&mut observer, &cancel).await, LeaderUpdate::Event(Event::Put(e)) if e.mod_revision == gap && e.value == b"gap")
    );
    // Reopen at a deterministic snapshot/watch boundary (no live stream yet).
    drop(observer);
    let mut observer = LeaderObserver::new(store.clone(), key.to_vec()).unwrap();
    assert!(matches!(
        update(&mut observer, &cancel).await,
        LeaderUpdate::Snapshot(_)
    ));
    // Compact over unread replacement and deletion; recovery must reveal absence.
    let changed = put(store.as_ref(), key, b"unread").await;
    let deleted = store
        .transact(vec![], vec![Mutation::Delete { key: key.to_vec() }])
        .await
        .unwrap()
        .revision;
    store.compact(deleted).await.unwrap();
    let LeaderUpdate::Snapshot(snapshot) = update(&mut observer, &cancel).await else {
        panic!("compaction requires a new snapshot")
    };
    assert!(snapshot.entries.is_empty());
    assert!(snapshot.revision >= deleted && deleted > changed);
    // Compact again with a new leader: replacement snapshot carries exact current value.
    let new_revision = put(store.as_ref(), key, b"new term").await;
    // Etcd can replay the boundary revision itself; compact strictly past it.
    let boundary = put(store.as_ref(), b"/reset-watch/leader-neighbor", b"boundary").await;
    store.compact(boundary).await.unwrap();
    let LeaderUpdate::Snapshot(snapshot) = update(&mut observer, &cancel).await else {
        panic!("second recovery snapshot required")
    };
    assert_eq!(snapshot.entries.len(), 1);
    assert_eq!(snapshot.entries[0].value, b"new term");
    assert_eq!(snapshot.entries[0].mod_revision, new_revision);
    let gap = put(store.as_ref(), key, b"after recovery").await;
    assert!(
        matches!(update(&mut observer, &cancel).await, LeaderUpdate::Event(Event::Put(e)) if e.mod_revision == gap)
    );
    assert!(
        timeout(Duration::from_millis(80), observer.next(&cancel))
            .await
            .is_err()
    );
    let deleted = store
        .transact(vec![], vec![Mutation::Delete { key: key.to_vec() }])
        .await
        .unwrap()
        .revision;
    assert!(
        matches!(update(&mut observer, &cancel).await, LeaderUpdate::Event(Event::Delete { revision, .. }) if revision == deleted)
    );
    assert!(matches!(
        observer.next(&cancel).await,
        Err(ElectionError::NotLeader)
    ));
    cancel.cancel();
    assert!(matches!(
        observer.next(&cancel).await,
        Err(ElectionError::Cancelled)
    ));
    assert!(matches!(
        observer.next(&CancellationToken::new()).await,
        Err(ElectionError::Cancelled)
    ));
}
async fn campaign(store: Arc<dyn MetaStore>) -> Arc<Leadership> {
    Arc::new(
        Leadership::campaign_with_key(
            store,
            CancellationToken::new(),
            66,
            b"/reset-race/leader".to_vec(),
            b"same value".to_vec(),
            30,
            vec![],
        )
        .await
        .unwrap(),
    )
}
async fn reset_behavior(backend: Arc<dyn MetaStore>) {
    let io = Arc::new(GateStore {
        inner: backend.clone(),
        block_revoke: true.into(),
        revoke_entered: Notify::new(),
        revoke_release: Notify::new(),
    });
    let slot = Arc::new(LeadershipSlot::default());
    let old = campaign(io.clone()).await;
    slot.publish(old.clone());
    let reset_slot = slot.clone();
    let reset_old = old.clone();
    let reset = tokio::spawn(async move { reset_slot.reset(&reset_old).await });
    timeout(Duration::from_secs(3), io.revoke_entered.notified())
        .await
        .unwrap();
    assert!(
        slot.current().is_none(),
        "detach before blocking lease cleanup"
    );
    assert!(old.is_expired());
    // Lease cleanup is held at IO. Remove old key through the real store and campaign.
    backend
        .transact(
            vec![],
            vec![Mutation::Delete {
                key: b"/reset-race/leader".to_vec(),
            }],
        )
        .await
        .unwrap();
    let new = campaign(io.clone()).await;
    assert_ne!(new.lease_id(), old.lease_id());
    slot.publish(new.clone());
    let keep = new.keep(&tokio::runtime::Handle::current(), CancellationToken::new());
    io.revoke_release.notify_one();
    timeout(Duration::from_secs(3), reset)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        slot.current().is_some_and(|term| Arc::ptr_eq(&term, &new)),
        "old reset must preserve new publication"
    );
    assert!(old.is_expired());
    assert!(!new.is_expired());
    assert!(!keep.is_finished());
    new.validate().await.unwrap();
    io.block_revoke.store(false, Ordering::SeqCst);
    slot.reset(&old).await.unwrap(); // late repeated cleanup cannot detach replacement
    assert!(slot.current().is_some_and(|term| Arc::ptr_eq(&term, &new)));
    new.renew().await.unwrap();
    slot.reset(&new).await.unwrap();
    assert!(slot.current().is_none());
    assert!(new.is_expired());
    assert_eq!(
        timeout(Duration::from_secs(3), keep.join())
            .await
            .unwrap()
            .unwrap(),
        LeaseKeepExit::Cancelled
    );
    assert!(
        backend
            .read(Read::range(
                b"/reset-race/leader",
                b"",
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
}
#[tokio::test]
async fn watch_reset_memory() {
    observe_behavior(Arc::new(MemoryStore::default())).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn watch_reset_etcd() {
    let fixture = super::etcd_fixture::Fixture::start();
    observe_behavior(Arc::new(fixture.store().await)).await;
}
#[tokio::test]
async fn watch_reset_race_memory() {
    reset_behavior(Arc::new(MemoryStore::default())).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn watch_reset_race_etcd() {
    let fixture = super::etcd_fixture::Fixture::start();
    reset_behavior(Arc::new(fixture.store().await)).await;
}
