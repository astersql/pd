// Copyright 2026 AsterSQL.
use super::state_regression::*;
use crate::harness::fixture;
use astersql_pd_storage_kv::MemoryStore;
use serde_json::json;
use std::sync::Arc;
#[tokio::test]
async fn state_theme_replay() {
    replay(Arc::new(MemoryStore::default()), None).await;
}
#[tokio::test]
async fn state_theme_replay_etcd() {
    let mut f = fixture::Fixture::start();
    replay(Arc::new(f.store().await), Some(&mut f)).await;
}
#[test]
fn state_theme_diff_reports_field() {
    let f = fixture();
    let mut wrong = f["snapshots"][0].clone();
    wrong["regions"]["regions"][0]["epoch"]["version"] = json!(999);
    let difference = compare(&wrong, &f["snapshots"][0]).unwrap_err();
    assert!(difference.contains("$/regions/regions/0/epoch/version: 999 != 2"));
}

use astersql_pd_core::{RegionInfo, RegionMeta};
use astersql_pd_storage_kv::{MetaStore, Mutation, StoreError};
use astersql_pd_syncer::{
    DownstreamSender, Downstreams, HistoryBuffer, HistoryIndex, MetadataWatch, PersistentHistory,
    SyncFrame,
};
use tokio::{
    runtime::Handle,
    time::{Duration, timeout},
};
use tokio_util::sync::CancellationToken;

async fn recovery(store: Arc<dyn MetaStore>) {
    let cancel = CancellationToken::new();
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
    let mut view = MetadataWatch::load(store.clone(), b"/meta/".to_vec(), 1, &cancel)
        .await
        .unwrap();
    assert_eq!(view.entries().len(), 3);
    assert_eq!(view.next_revision(), initial.revision + 1);
    let updated = store
        .transact(
            vec![],
            vec![
                Mutation::put(b"/meta/a", b"new"),
                Mutation::put(b"/meta/b", b"new"),
            ],
        )
        .await
        .unwrap();
    assert_eq!(
        timeout(Duration::from_secs(5), view.advance(&cancel))
            .await
            .unwrap()
            .unwrap(),
        updated.revision + 1
    );
    assert_eq!(view.entries()[b"/meta/a".as_slice()].value, b"new");
    drop(view);
    let mut view = MetadataWatch::load(store.clone(), b"/meta/".to_vec(), 1, &cancel)
        .await
        .unwrap();
    let commit = store
        .transact(
            vec![],
            vec![
                Mutation::Delete {
                    key: b"/meta/a".to_vec(),
                },
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
    assert_eq!(
        timeout(Duration::from_secs(5), view.advance(&cancel))
            .await
            .unwrap()
            .unwrap(),
        boundary.revision + 1
    );
    assert_eq!(view.entries().len(), 1);
    assert_eq!(
        view.entries()[b"/meta/d".as_slice()].mod_revision,
        commit.revision
    );
    let live = store
        .transact(vec![], vec![Mutation::put(b"/meta/e", b"live")])
        .await
        .unwrap();
    assert_eq!(
        timeout(Duration::from_secs(5), view.advance(&cancel))
            .await
            .unwrap()
            .unwrap(),
        live.revision + 1
    );
    let child = cancel.clone();
    let mut task = tokio::spawn(async move {
        let result = view.advance(&child).await;
        (view, result)
    });
    // The call has entered the actual observation path: it remains pending on an empty prefix tail.
    assert!(timeout(Duration::from_millis(30), &mut task).await.is_err());
    cancel.cancel();
    let (mut view, result) = timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result, Err(StoreError::Cancelled));
    assert_eq!(
        view.advance(&CancellationToken::new()).await,
        Err(StoreError::Cancelled)
    );
    let watch_cursor = view.next_revision();
    let mut history =
        PersistentHistory::load(HistoryBuffer::with_config(2, 2, 1).unwrap(), store.as_ref())
            .await
            .unwrap();
    for id in 1..=6 {
        history
            .record(Arc::new(RegionInfo::new(
                RegionMeta {
                    id,
                    ..Default::default()
                },
                None,
                vec![],
                vec![],
            )))
            .await
            .unwrap();
    }
    assert_eq!(history.history().next_index(), HistoryIndex(6));
    assert_eq!(history.history().first_index(), HistoryIndex(4));
    assert!(history.history().get(HistoryIndex(3)).is_none());
    history.persist().await.unwrap();
    drop(history);
    let restored =
        PersistentHistory::load(HistoryBuffer::with_config(2, 2, 1).unwrap(), store.as_ref())
            .await
            .unwrap();
    assert_eq!(restored.history().first_index(), HistoryIndex(6));
    assert_eq!(restored.history().next_index(), HistoryIndex(6));
    assert!(restored.history().is_empty());
    assert_eq!(
        view.next_revision(),
        watch_cursor,
        "history persistence must not advance the metadata watch"
    );
    struct Block(tokio::sync::mpsc::Sender<()>);
    impl DownstreamSender for Block {
        async fn send(&mut self, _: SyncFrame) -> Result<(), String> {
            self.0.send(()).await.unwrap();
            std::future::pending().await
        }
    }
    let streams = Downstreams::new(HistoryBuffer::with_capacity_unit(2, 1).unwrap());
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let task = streams.bind(
        &Handle::current(),
        "blocked".into(),
        HistoryIndex(0),
        Block(tx),
    );
    streams
        .append([Arc::new(RegionInfo::new(
            RegionMeta {
                id: 1,
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        ))])
        .unwrap();
    streams.broadcast(false);
    timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(task.send_index(), HistoryIndex(0));
    task.cancel();
    assert!(
        timeout(Duration::from_secs(5), task.join())
            .await
            .unwrap()
            .unwrap()
            .is_ok()
    );
    assert!(streams.names().is_empty());
    assert!(rx.recv().await.is_none());
}
#[tokio::test]
async fn state_theme_fault_recovery() {
    recovery(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn state_theme_fault_recovery_etcd() {
    let mut f = fixture::Fixture::start();
    let store = Arc::new(f.store().await);
    recovery(store.clone()).await;
    let cancel = CancellationToken::new();
    let mut view = MetadataWatch::load(store.clone(), b"/meta/".to_vec(), 1, &cancel)
        .await
        .unwrap();
    let before = view.entries().clone();
    let cursor = view.next_revision();
    f.command("stop", "stopped");
    assert_eq!(
        timeout(Duration::from_secs(8), view.advance(&cancel))
            .await
            .unwrap(),
        Err(StoreError::Unavailable)
    );
    assert_eq!(view.entries(), &before);
    assert_eq!(view.next_revision(), cursor);
    f.command("start", "restarted");
    let restored = Arc::new(f.store().await);
    let commit = restored
        .transact(vec![], vec![Mutation::put(b"/meta/restarted", b"live")])
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
    loop {
        match tokio::time::timeout_at(deadline, view.advance(&cancel))
            .await
            .unwrap()
        {
            Ok(revision) => {
                assert_eq!(revision, commit.revision + 1);
                break;
            }
            Err(StoreError::Unavailable) => tokio::time::sleep(Duration::from_millis(100)).await,
            Err(e) => panic!("{e}"),
        }
    }
    assert_eq!(view.entries()[b"/meta/restarted".as_slice()].value, b"live");
    let h = PersistentHistory::load(
        HistoryBuffer::with_config(2, 2, 1).unwrap(),
        restored.as_ref(),
    )
    .await
    .unwrap();
    assert_eq!(h.history().next_index(), HistoryIndex(6));
    assert!(h.history().is_empty());
    cancel.cancel();
    assert_eq!(view.advance(&cancel).await, Err(StoreError::Cancelled));
}

#[test]
fn state_theme_history_saved_go_snapshot() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/state/task-30.json")).unwrap();
    let mut history =
        HistoryBuffer::with_capacity_unit(f["capacity"].as_u64().unwrap() as usize, 1).unwrap();
    history.reset_with_index(HistoryIndex(f["reset"].as_u64().unwrap()));
    assert!(!f["input"].as_array().unwrap().is_empty());
    for id in f["input"].as_array().unwrap() {
        history
            .record(Arc::new(RegionInfo::new(
                RegionMeta {
                    id: id.as_u64().unwrap(),
                    ..Default::default()
                },
                None,
                vec![],
                vec![],
            )))
            .unwrap();
    }
    assert_eq!(history.first_index().0, f["first"].as_u64().unwrap());
    assert_eq!(history.next_index().0, f["next"].as_u64().unwrap());
    assert!(
        history
            .records_from(HistoryIndex(f["reset"].as_u64().unwrap()))
            .is_empty()
    );
    assert_eq!(
        history
            .records_from(history.first_index())
            .iter()
            .map(|r| r.meta().id)
            .collect::<Vec<_>>(),
        f["records"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_u64().unwrap())
            .collect::<Vec<_>>()
    );
}
