// Copyright 2026 AsterSQL.
use super::leader_watch::{CHECK_INTERVAL, UNHEALTHY_TIMEOUT};
use super::*;
use astersql_pd_storage_kv::*;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

// Fault injection is confined to the watch IO boundary. All state and events
// continue to come from the real MemoryStore/EtcdMetaStore implementations.
struct InterruptedWatch {
    inner: Box<dyn MetaWatch>,
    failures: usize,
}
#[async_trait::async_trait]
impl MetaWatch for InterruptedWatch {
    async fn next(
        &mut self,
        cancel: &CancellationToken,
    ) -> astersql_pd_storage_kv::Result<WatchBatch> {
        if self.failures > 0 {
            self.failures -= 1;
            Err(StoreError::Unavailable)
        } else {
            self.inner.next(cancel).await
        }
    }
}
async fn put(store: &dyn MetaStore, key: &[u8], value: &[u8]) -> Revision {
    store
        .transact(vec![], vec![Mutation::put(key, value)])
        .await
        .unwrap()
        .revision
}
async fn receive(watch: &mut LeaderWatch, cancel: &CancellationToken) -> Event {
    tokio::time::timeout(Duration::from_secs(15), watch.next(cancel))
        .await
        .expect("watch deadline")
        .unwrap()
}
async fn behavior(store: Arc<dyn MetaStore>) {
    let key = b"/watch/leader";
    put(store.as_ref(), key, b"before").await;
    let start = put(store.as_ref(), key, b"inclusive").await;
    put(store.as_ref(), b"/watch/leader-neighbor", b"neighbor").await;
    let latest = put(store.as_ref(), key, b"latest").await;
    let cancel = CancellationToken::new();
    let mut watch = LeaderWatch::new(store.clone(), key.to_vec(), start).unwrap();
    assert_eq!(
        receive(&mut watch, &cancel).await,
        Event::Put(Entry {
            key: key.to_vec(),
            value: b"inclusive".to_vec(),
            create_revision: start - 1,
            mod_revision: start,
            lease: None,
        })
    );
    match receive(&mut watch, &cancel).await {
        Event::Put(entry) => {
            assert_eq!(entry.value, b"latest");
            assert_eq!(entry.mod_revision, latest);
        }
        event => panic!("unexpected {event:?}"),
    }
    watch.watch = Some(Box::new(InterruptedWatch {
        inner: watch.watch.take().unwrap(),
        failures: 1,
    }));
    let recovered = put(store.as_ref(), key, b"after retry").await;
    assert!(
        matches!(receive(&mut watch, &cancel).await, Event::Put(entry) if entry.mod_revision == recovered && entry.value == b"after retry")
    );
    // No historical replay, no neighboring-prefix events, idle cancellation.
    assert!(
        tokio::time::timeout(Duration::from_millis(80), watch.next(&cancel))
            .await
            .is_err()
    );
    let (result, ()) = tokio::join!(watch.next(&cancel), async {
        tokio::time::sleep(Duration::from_millis(30)).await;
        cancel.cancel();
    });
    assert!(matches!(result, Err(ElectionError::Cancelled)));
    assert!(matches!(
        watch.next(&CancellationToken::new()).await,
        Err(ElectionError::Cancelled)
    ));

    let cancel = CancellationToken::new();
    let mut watch = LeaderWatch::new(store.clone(), key.to_vec(), recovered + 1).unwrap();
    store
        .transact(
            vec![],
            vec![Mutation::Delete {
                key: b"/watch/leader-neighbor".to_vec(),
            }],
        )
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(80), watch.next(&cancel))
            .await
            .is_err()
    );
    let deleted = store
        .transact(vec![], vec![Mutation::Delete { key: key.to_vec() }])
        .await
        .unwrap()
        .revision;
    assert_eq!(
        receive(&mut watch, &cancel).await,
        Event::Delete {
            key: key.to_vec(),
            revision: deleted
        }
    );
    put(store.as_ref(), key, b"new leader").await;
    assert!(matches!(
        watch.next(&cancel).await,
        Err(ElectionError::NotLeader)
    ));

    // Compaction must be explicit: never silently skip potentially deleted leadership.
    let revision = put(store.as_ref(), key, b"compact").await;
    store.compact(revision).await.unwrap();
    let mut compacted = LeaderWatch::new(store.clone(), key.to_vec(), start).unwrap();
    assert!(matches!(
        compacted.next(&cancel).await,
        Err(ElectionError::Store(StoreError::Compacted { .. }))
    ));
    assert!(matches!(
        LeaderWatch::new(store.clone(), vec![], 1),
        Err(ElectionError::InvalidArgument)
    ));
    assert!(matches!(
        LeaderWatch::new(store, key.to_vec(), 0),
        Err(ElectionError::InvalidArgument)
    ));
}
#[tokio::test]
async fn leader_watch_memory() {
    behavior(Arc::new(MemoryStore::default())).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn leader_watch_etcd() {
    let mut fixture = super::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    behavior(store.clone()).await;
    let key = b"/restart/leader";
    let revision = put(store.as_ref(), key, b"before stop").await;
    let mut watch = LeaderWatch::new(store.clone(), key.to_vec(), revision).unwrap();
    let cancel = CancellationToken::new();
    assert!(
        matches!(receive(&mut watch, &cancel).await, Event::Put(entry) if entry.mod_revision == revision)
    );
    fixture.command("stop", "stopped");
    {
        let receive = watch.next(&cancel);
        tokio::pin!(receive);
        assert!(
            tokio::time::timeout(Duration::from_millis(250), &mut receive)
                .await
                .is_err()
        );
        fixture.command("start", "restarted");
        let revision = put(store.as_ref(), key, b"after restart").await;
        assert!(
            matches!(tokio::time::timeout(Duration::from_secs(15), &mut receive).await.unwrap().unwrap(), Event::Put(entry) if entry.mod_revision == revision && entry.value == b"after restart")
        );
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(100), watch.next(&cancel))
            .await
            .is_err()
    );
    let revision = store
        .transact(vec![], vec![Mutation::Delete { key: key.to_vec() }])
        .await
        .unwrap()
        .revision;
    assert_eq!(
        receive(&mut watch, &cancel).await,
        Event::Delete {
            key: key.to_vec(),
            revision
        }
    );
}

#[tokio::test(start_paused = true)]
async fn leader_watch_unhealthy_timeout() {
    let store: Arc<dyn MetaStore> = Arc::new(MemoryStore::default());
    let key = b"/unhealthy/leader";
    let revision = put(store.as_ref(), key, b"leader").await;
    let mut watch = LeaderWatch::new(store.clone(), key.to_vec(), revision).unwrap();
    watch.watch = Some(Box::new(InterruptedWatch {
        inner: store.watch(key.to_vec(), revision).await.unwrap(),
        failures: usize::MAX,
    }));
    let start = tokio::time::Instant::now();
    assert!(matches!(
        watch.next(&CancellationToken::new()).await,
        Err(ElectionError::Store(StoreError::Unavailable))
    ));
    assert!(start.elapsed() >= UNHEALTHY_TIMEOUT);
    assert!(start.elapsed() < UNHEALTHY_TIMEOUT + Duration::from_secs(1));
    assert!(matches!(
        watch.next(&CancellationToken::new()).await,
        Err(ElectionError::Store(StoreError::Unavailable))
    ));
    assert_eq!(
        store
            .read(Read::range(key, b"", Consistency::Linearizable))
            .await
            .unwrap()
            .entries[0]
            .value,
        b"leader"
    );
}

#[tokio::test(start_paused = true)]
async fn leader_watch_healthy_idle_reopen() {
    let store: Arc<dyn MetaStore> = Arc::new(MemoryStore::default());
    let key = b"/idle/leader";
    let revision = put(store.as_ref(), key, b"leader").await;
    let mut watch = LeaderWatch::new(store.clone(), key.to_vec(), revision + 1).unwrap();
    let cancel = CancellationToken::new();
    let (result, ()) = tokio::join!(watch.next(&cancel), async {
        tokio::time::sleep(CHECK_INTERVAL * 8).await;
        cancel.cancel();
    });
    assert!(matches!(result, Err(ElectionError::Cancelled)));
    assert!(watch.watch.is_none());
}
