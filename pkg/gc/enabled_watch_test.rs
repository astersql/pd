// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_storage_domain::{
    Base, Consistency, Keyspace, KeyspaceIdentity, KeyspaceStorage, MetaStore, Read,
};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
async fn storage(store: Arc<dyn MetaStore>) -> KeyspaceStorage {
    astersql_pd_storage_endpoint::StorageEndpoint::new(store)
        .keyspace_storage(astersql_pd_keypath::ClusterPaths::new(42))
}
async fn save(s: &KeyspaceStorage, id: u32, state: i32) -> i64 {
    let mut tx = Base::new(s.store.as_ref()).begin_txn();
    s.save_keyspace_meta(
        &mut tx,
        &s.paths,
        &Keyspace {
            identity: Some(KeyspaceIdentity::Id(id)),
            name: format!("ks{id}"),
            state,
            config: std::collections::HashMap::from([(
                "gc_management_type".into(),
                "keyspace_level".into(),
            )]),
            ..Default::default()
        },
    )
    .unwrap();
    tx.commit::<String>().await.unwrap();
    s.store
        .read(Read::prefix(b"", Consistency::Linearizable))
        .await
        .unwrap()
        .revision
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_049_behavior() {
    let f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let storage = storage(store).await;
    let rev = save(&storage, 1, 0).await;
    let index = Arc::new(EnabledKeyspaceIndex::new(EnabledSnapshotLoader {
        storage: storage.clone(),
        concurrency: 2,
    }));
    let cancel = CancellationToken::new();
    let task = {
        let index = index.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move { index.run(&cancel).await })
    };
    let initial = tokio::time::timeout(
        Duration::from_secs(3),
        index.snapshot_at_least(rev, &cancel),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(initial.entries.len(), 1);
    let target = save(&storage, 2, 0).await;
    let updated = tokio::time::timeout(
        Duration::from_secs(4),
        index.snapshot_at_least(target, &cancel),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        updated.entries.contains_key(&2),
        "watch must publish newly enabled metadata at the requested progress barrier"
    );
    assert!(updated.revision >= target);
    cancel.cancel();
    task.await.unwrap();
}
async fn at(index: &EnabledKeyspaceIndex, revision: i64) -> EnabledSnapshot {
    tokio::time::timeout(
        Duration::from_secs(12),
        index.snapshot_at_least(revision, &CancellationToken::new()),
    )
    .await
    .unwrap()
    .unwrap()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_049_deletes_empty_progress_and_corrupt_reload() {
    let mut f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let storage = storage(store.clone()).await;
    let revision = save(&storage, 1, 0).await;
    let index = Arc::new(EnabledKeyspaceIndex::new(EnabledSnapshotLoader {
        storage: storage.clone(),
        concurrency: 2,
    }));
    let cancel = CancellationToken::new();
    let task = {
        let i = index.clone();
        let c = cancel.clone();
        tokio::spawn(async move { i.run(&c).await })
    };
    assert_eq!(at(&index, revision).await.entries.len(), 1);
    assert!(index.can_skip_gc_state_warmup(2));
    assert!(!index.can_skip_gc_state_warmup(1));
    // No prefix events exist for this revision: only ordered progress can prove it.
    Base::new(store.as_ref())
        .save(b"/unrelated-key", b"one")
        .await
        .unwrap();
    let barrier = store
        .read(Read::prefix(b"", Consistency::Linearizable))
        .await
        .unwrap()
        .revision;
    let mut owned = at(&index, barrier).await;
    owned.entries.clear();
    assert_eq!(at(&index, barrier).await.entries.len(), 1);
    let disabled = save(&storage, 1, 1).await;
    assert!(at(&index, disabled).await.entries.is_empty());
    let enabled = save(&storage, 2, 0).await;
    assert!(at(&index, enabled).await.entries.contains_key(&2));
    Base::new(store.as_ref())
        .remove(storage.paths.keyspace_meta_path(2).as_bytes())
        .await
        .unwrap();
    let removed = store
        .read(Read::prefix(b"", Consistency::Linearizable))
        .await
        .unwrap()
        .revision;
    assert!(at(&index, removed).await.entries.is_empty());
    let corrupt = storage.paths.keyspace_meta_path(3);
    Base::new(store.as_ref())
        .save(corrupt.as_bytes(), b"malformed")
        .await
        .unwrap();
    let bad = store
        .read(Read::prefix(b"", Consistency::Linearizable))
        .await
        .unwrap()
        .revision;
    let waiter = CancellationToken::new();
    let timer = waiter.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(1500)).await;
        timer.cancel();
    });
    assert!(index.snapshot_at_least(bad, &waiter).await.is_err());
    assert!(index.applied_revision() < bad);
    let fixed = save(&storage, 3, 0).await;
    assert!(at(&index, fixed).await.entries.contains_key(&3));
    f.command("stop", "stopped");
    f.command("start", "restarted");
    let recovered = astersql_pd_storage_endpoint::StorageEndpoint::new(fresh(&f).await)
        .keyspace_storage(storage.paths.clone());
    let resumed = save(&recovered, 4, 0).await;
    assert!(at(&index, resumed).await.entries.contains_key(&4));
    cancel.cancel();
    task.await.unwrap();
    assert!(
        index
            .snapshot_at_least(0, &CancellationToken::new())
            .await
            .is_err()
    );
}
async fn fresh(f: &etcd_fixture::Fixture) -> Arc<dyn MetaStore> {
    Arc::new(f.store().await)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_049_compacted_creation_reloads_without_repeating_initial_hooks() {
    let f = etcd_fixture::Fixture::start();
    let storage = storage(fresh(&f).await).await;
    let initial = save(&storage, 1, 0).await;
    let index = Arc::new(EnabledKeyspaceIndex::new(EnabledSnapshotLoader {
        storage: storage.clone(),
        concurrency: 2,
    }));
    let cancel = CancellationToken::new();
    let published = Arc::new(tokio::sync::Notify::new());
    let wake = published.clone();
    let (release, wait) = std::sync::mpsc::channel();
    let wait = std::sync::Mutex::new(wait);
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = count.clone();
    let hooks = EnabledIndexHooks {
        on_initial_snapshot: Some(Arc::new(move |mut s| {
            assert_eq!(s.entries.len(), 1);
            s.entries.clear();
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            wake.notify_one();
            tokio::task::block_in_place(|| {
                wait.lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap()
            });
        })),
        ..Default::default()
    };
    let task = {
        let i = index.clone();
        let c = cancel.clone();
        tokio::spawn(async move { i.run_with_hooks(&c, hooks).await })
    };
    published.notified().await;
    assert_eq!(index.applied_revision(), initial);
    let revision = save(&storage, 2, 0).await;
    storage.store.compact(revision).await.unwrap();
    release.send(()).unwrap();
    let recovered = at(&index, revision).await;
    assert_eq!(recovered.entries.len(), 2);
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
    cancel.cancel();
    task.await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_049_cancel_before_ready_wakes_independent_waiter() {
    let f = etcd_fixture::Fixture::start();
    let gate = response_gate_test::ResponseGate::start(&f.endpoint).await;
    let reader: Arc<dyn MetaStore> = Arc::new(
        astersql_pd_storage_kv::EtcdMetaStore::connect(
            &[gate.endpoint.clone()],
            Duration::from_secs(2),
        )
        .await
        .unwrap(),
    );
    let storage = storage(reader).await;
    gate.arm_on_key(storage.paths.keyspace_meta_prefix().as_bytes());
    let index = Arc::new(EnabledKeyspaceIndex::new(EnabledSnapshotLoader {
        storage,
        concurrency: 2,
    }));
    assert!(!index.can_skip_gc_state_warmup(99));
    let cancel = CancellationToken::new();
    let task = {
        let i = index.clone();
        let c = cancel.clone();
        tokio::spawn(async move { i.run(&c).await })
    };
    tokio::time::timeout(Duration::from_secs(3), gate.held.notified())
        .await
        .unwrap();
    cancel.cancel();
    task.await.unwrap();
    assert!(
        tokio::time::timeout(
            Duration::from_secs(1),
            index.snapshot_at_least(0, &CancellationToken::new())
        )
        .await
        .unwrap()
        .is_err()
    );
    gate.close().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_049_busy_watch_still_publishes_ordered_progress() {
    let f = etcd_fixture::Fixture::start();
    let storage = storage(fresh(&f).await).await;
    let revision = save(&storage, 1, 0).await;
    let index = Arc::new(EnabledKeyspaceIndex::new(EnabledSnapshotLoader {
        storage: storage.clone(),
        concurrency: 2,
    }));
    let cancel = CancellationToken::new();
    let task = {
        let i = index.clone();
        let c = cancel.clone();
        tokio::spawn(async move { i.run(&c).await })
    };
    at(&index, revision).await;
    let revision = save(&storage, 2, 0).await;
    let writes = cancel.child_token();
    let writer = {
        let storage = storage.clone();
        let writes = writes.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {biased;_=writes.cancelled()=>break,_=save(&storage,3,0)=>{}}
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
    };
    let snapshot = tokio::time::timeout(
        Duration::from_secs(3),
        index.snapshot_at_least(revision, &cancel),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(snapshot.entries.contains_key(&2));
    writes.cancel();
    writer.await.unwrap();
    cancel.cancel();
    task.await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_049_progress_resume_survives_compacted_old_event_cursor() {
    let mut f = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = fresh(&f).await;
    let initial = store
        .read(Read::prefix(b"", Consistency::Linearizable))
        .await
        .unwrap()
        .revision;
    let mut watch = store
        .watch(b"/watched/".to_vec(), initial + 1)
        .await
        .unwrap();
    Base::new(store.as_ref())
        .save(b"/unrelated", b"one")
        .await
        .unwrap();
    let cancel = CancellationToken::new();
    let progress = tokio::time::timeout(Duration::from_secs(3), watch.next_with_progress(&cancel))
        .await
        .unwrap()
        .unwrap();
    assert!(progress.events.is_empty());
    assert!(progress.revision > initial);
    store.compact(progress.revision).await.unwrap();
    f.command("stop", "stopped");
    f.command("start", "restarted");
    Base::new(store.as_ref())
        .save(b"/watched/key", b"two")
        .await
        .unwrap();
    let received = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match watch.next_with_progress(&cancel).await {
                Ok(batch) if !batch.events.is_empty() => break batch,
                Ok(_) => {}
                Err(astersql_pd_storage_domain::StoreError::Unavailable) => {}
                Err(e) => panic!(
                    "confirmed progress must not replay the compacted old event cursor: {e:?}"
                ),
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(received.events.len(), 1);
}
