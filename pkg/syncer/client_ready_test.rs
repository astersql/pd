// Copyright 2026 AsterSQL.
use super::{
    CatchUpFrame, ClientApply, FullSnapshot, HistoryBuffer, HistoryIndex as I, SyncApplyResponse,
    SyncResponseError,
};
use astersql_pd_core::{BasicCluster, KeyRange, RegionEpoch, RegionInfo, RegionMeta};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage::Storage;
use astersql_pd_storage_kv::MemoryStore;
use std::{
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

fn region(id: u64) -> RegionMeta {
    RegionMeta {
        id,
        key_range: KeyRange {
            start_key: id.to_be_bytes().to_vec(),
            end_key: (id + 1).to_be_bytes().to_vec(),
        },
        epoch: Some(RegionEpoch {
            version: 1,
            conf_ver: 1,
        }),
        ..Default::default()
    }
}
fn frame(start: u64, ids: impl IntoIterator<Item = u64>) -> SyncApplyResponse {
    SyncApplyResponse {
        start_index: I(start),
        regions: ids.into_iter().map(region).collect(),
        ..Default::default()
    }
}
fn state(client: &ClientApply, history: &HistoryBuffer, full: bool, running: bool, next: u64) {
    assert_eq!(client.full_syncing(), full);
    assert_eq!(client.is_running(), running);
    assert_eq!(history.next_index(), I(next));
}

async fn completion(storage: &Storage, old_leader: bool) {
    let paths = ClusterPaths::new(38);
    let mut cluster = BasicCluster::new();
    let mut history = HistoryBuffer::new(10).unwrap();
    history.reset_with_index(I(if old_leader { 42 } else { 0 }));
    let mut client = ClientApply::default();
    let out = client
        .apply(frame(0, [1]), &mut cluster, storage, &paths, &mut history)
        .await
        .unwrap();
    assert!(out.handled);
    state(&client, &history, true, false, 0);
    assert!(history.is_empty());
    assert!(cluster.regions.get_region(1).is_some());
    assert_eq!(
        storage.load_region(&paths, 1).await.unwrap(),
        Some(region(1))
    );
    if old_leader {
        let out = client
            .apply(frame(42, [2]), &mut cluster, storage, &paths, &mut history)
            .await
            .unwrap();
        assert!(out.handled);
        state(&client, &history, true, false, 0);
        assert!(history.is_empty());
        assert!(cluster.regions.get_region(2).is_some());
        assert_eq!(
            storage.load_region(&paths, 2).await.unwrap(),
            Some(region(2))
        );
    }
    let end = if old_leader { 43 } else { 1 };
    let out = client
        .apply(frame(end, []), &mut cluster, storage, &paths, &mut history)
        .await
        .unwrap();
    assert!(out.handled);
    state(&client, &history, false, true, end);
    assert!(history.is_empty());
    // Once running, start=0 is an incremental reset, rather than a new full scan.
    client
        .apply(frame(0, [3]), &mut cluster, storage, &paths, &mut history)
        .await
        .unwrap();
    state(&client, &history, false, true, 1);
    assert_eq!(history.get(I(0)).unwrap().meta().id, 3);
}

#[tokio::test]
async fn state_task_38_client_ready_waits_for_full_sync_completion() {
    completion(&Storage::new(Arc::new(MemoryStore::default())), false).await;
}
#[tokio::test]
async fn state_task_38_client_ready_old_leader_keepalive() {
    completion(&Storage::new(Arc::new(MemoryStore::default())), true).await;
}

async fn empty_snapshot(storage: &Storage) {
    let mut leader_history = HistoryBuffer::new(2000).unwrap();
    leader_history.reset_with_index(I(42));
    let snapshot = FullSnapshot::bind(&mut leader_history, &BasicCluster::new());
    assert_eq!(
        snapshot.batches(NonZeroUsize::new(1000).unwrap()).count(),
        0
    );
    for id in 1..=1001 {
        leader_history
            .record(Arc::new(RegionInfo::new(region(id), None, vec![], vec![])))
            .unwrap();
    }
    let mut frames = vec![];
    let next = snapshot
        .catch_up(
            &Mutex::new(leader_history),
            NonZeroUsize::new(1000).unwrap(),
            |batch| {
                frames.push(match batch {
                    CatchUpFrame::Records {
                        start_index,
                        records,
                    } => SyncApplyResponse {
                        start_index: I(start_index),
                        regions: records.iter().map(|r| r.meta().clone()).collect(),
                        ..Default::default()
                    },
                    CatchUpFrame::Complete { next_index } => SyncApplyResponse {
                        start_index: next_index,
                        ..Default::default()
                    },
                });
                Ok::<_, ()>(())
            },
            || panic!("healthy stream disconnected"),
        )
        .unwrap();
    assert_eq!(next, I(1043));
    assert_eq!(
        frames
            .iter()
            .map(|f| (f.start_index.0, f.regions.len()))
            .collect::<Vec<_>>(),
        vec![(0, 1000), (1000, 1), (1043, 0)]
    );
    let mut client = ClientApply::default();
    let mut cluster = BasicCluster::new();
    let paths = ClusterPaths::new(3801);
    let mut history = HistoryBuffer::new(10).unwrap();
    history.reset_with_index(I(42));
    for (i, response) in frames.into_iter().enumerate() {
        assert!(
            client
                .apply(response, &mut cluster, storage, &paths, &mut history)
                .await
                .unwrap()
                .handled
        );
        state(
            &client,
            &history,
            i < 2,
            i == 2,
            if i < 2 { 0 } else { 1043 },
        );
        assert!(history.is_empty());
    }
    assert_eq!(cluster.regions.get_regions().len(), 1001);
    for id in 1..=1001 {
        assert!(cluster.regions.get_region(id).is_some());
        assert_eq!(
            storage.load_region(&paths, id).await.unwrap(),
            Some(region(id))
        );
    }
}
#[tokio::test]
async fn state_task_38_client_ready_empty_snapshot_catchup_completion() {
    empty_snapshot(&Storage::new(Arc::new(MemoryStore::default()))).await;
}
#[tokio::test]
async fn state_task_38_client_ready_empty_snapshot_without_catchup() {
    let storage = Storage::new(Arc::new(MemoryStore::default()));
    let mut client = ClientApply::default();
    let mut cluster = BasicCluster::new();
    let mut history = HistoryBuffer::new(10).unwrap();
    client
        .apply(
            frame(42, []),
            &mut cluster,
            &storage,
            &ClusterPaths::new(38),
            &mut history,
        )
        .await
        .unwrap();
    state(&client, &history, false, true, 42);
    assert!(cluster.regions.get_regions().is_empty());
}
#[tokio::test]
async fn state_task_38_client_ready_error_and_leader_reconnect() {
    let storage = Storage::new(Arc::new(MemoryStore::default()));
    let paths = ClusterPaths::new(38);
    let mut client = ClientApply::default();
    let mut cluster = BasicCluster::new();
    let mut history = HistoryBuffer::new(10).unwrap();
    client
        .apply(frame(0, [1]), &mut cluster, &storage, &paths, &mut history)
        .await
        .unwrap();
    let mut error = frame(999, [99]);
    error.error = Some(SyncResponseError {
        code: 0,
        message: "leader stopped".into(),
    });
    assert!(
        !client
            .apply(error, &mut cluster, &storage, &paths, &mut history)
            .await
            .unwrap()
            .handled
    );
    state(&client, &history, true, false, 0);
    assert!(cluster.regions.get_region(99).is_none());
    assert!(storage.load_region(&paths, 99).await.unwrap().is_none());
    client.reset_stream();
    state(&client, &history, false, false, 0);
    assert!(cluster.regions.get_region(1).is_some());
    // A different leader's first nonzero frame is incremental after reconnect.
    client
        .apply(frame(42, [2]), &mut cluster, &storage, &paths, &mut history)
        .await
        .unwrap();
    state(&client, &history, false, true, 43);
    assert_eq!(history.get(I(42)).unwrap().meta().id, 2);
    client.reset_stream();
    state(&client, &history, false, false, 43);
    client
        .apply(frame(0, [3]), &mut cluster, &storage, &paths, &mut history)
        .await
        .unwrap();
    state(&client, &history, true, false, 0);
    assert!(history.is_empty());
    client
        .apply(frame(77, []), &mut cluster, &storage, &paths, &mut history)
        .await
        .unwrap();
    state(&client, &history, false, true, 77);
    client
        .apply(
            SyncApplyResponse {
                error: Some(SyncResponseError {
                    code: 0,
                    message: "stopped".into(),
                }),
                ..frame(999, [])
            },
            &mut cluster,
            &storage,
            &paths,
            &mut history,
        )
        .await
        .unwrap();
    state(&client, &history, false, false, 77);
}
#[tokio::test]
async fn state_task_38_client_ready_etcd_completion() {
    let fixture = super::persist_fixture::Fixture::start();
    let storage = Storage::new(Arc::new(fixture.store().await));
    completion(&storage, false).await;
    completion(&storage, true).await;
    empty_snapshot(&storage).await;
    drop(storage);
    drop(fixture); // EOF, wait/join and directory cleanup via existing fixture.
}
