// Copyright 2026 AsterSQL.
use super::*;
use crate::etcd_fixture as fixture;
use astersql_pd_core::RegionsInfo;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_storage_domain::{MetaStore, StorageEndpoint};
use std::sync::{Arc, RwLock};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
#[tokio::test]
async fn mcs_036_behavior() {
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let labeler = Arc::new(Mutex::new(
        RegionLabeler::load(StorageEndpoint::new(store), paths, 0)
            .await
            .unwrap(),
    ));
    let split = KeyspaceRegionSplit::new(
        Some(labeler.clone()),
        Some(RegionsInfo::new()),
        Arc::new(RwLock::new(KeyspaceSplitConfig::default())),
    );
    split
        .split(&CancellationToken::new(), 36, false, BoundType::Txn, 0)
        .await
        .unwrap();
    assert!(
        labeler.lock().await.index().rule("keyspaces/36").is_some(),
        "creating a keyspace must persist and publish its boundary rule"
    );
}

fn add_boundaries(regions: &mut RegionsInfo, id: u32, bound: BoundType) {
    use astersql_pd_core::{KeyRange, Peer, RegionEpoch, RegionInfo, RegionMeta};
    let boundaries = make_region_bound(id);
    let (left, right) = bound.bounds(&boundaries);
    for (offset, start, end) in [
        (0, left.to_vec(), right.to_vec()),
        (1, right.to_vec(), vec![]),
    ] {
        let region_id = u64::from(id) * 10 + 100 + offset;
        let peer = Peer {
            id: region_id + 1,
            store_id: 1,
            ..Default::default()
        };
        regions.put_region(Arc::new(RegionInfo::new(
            RegionMeta {
                id: region_id,
                key_range: KeyRange {
                    start_key: start,
                    end_key: end,
                },
                epoch: Some(RegionEpoch {
                    conf_ver: 1,
                    version: 1,
                }),
                peers: vec![peer.clone()],
                ..Default::default()
            },
            Some(peer),
            vec![],
            vec![],
        )));
    }
}
#[tokio::test]
async fn mcs_036_polling_existing_boundaries_and_live_configuration() {
    use std::time::Duration;
    tokio::time::pause();
    let mut regions = RegionsInfo::new();
    add_boundaries(&mut regions, 40, BoundType::Txn);
    let config = Arc::new(RwLock::new(KeyspaceSplitConfig {
        interval: Duration::from_millis(100),
        timeout: Duration::from_secs(1),
    }));
    let split = KeyspaceRegionSplit::new(None, Some(regions.clone()), config.clone());
    let worker = split.clone();
    let task = tokio::spawn(async move {
        worker
            .wait_for_boundaries(&CancellationToken::new(), 40, BoundType::Txn)
            .await
    });
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_millis(99)).await;
    assert!(
        !task.is_finished(),
        "existing bounds still wait for the first configured tick"
    );
    tokio::time::advance(Duration::from_millis(1)).await;
    task.await.unwrap().unwrap();
    *config.write().unwrap() = KeyspaceSplitConfig {
        interval: Duration::from_millis(20),
        timeout: Duration::from_millis(200),
    };
    let worker = split.clone();
    let task = tokio::spawn(async move {
        worker
            .wait_for_boundaries(&CancellationToken::new(), 41, BoundType::Txn)
            .await
    });
    tokio::task::yield_now().await;
    config.write().unwrap().interval = Duration::from_millis(5);
    tokio::time::advance(Duration::from_millis(20)).await;
    tokio::task::yield_now().await;
    assert!(!task.is_finished());
    add_boundaries(&mut regions, 41, BoundType::Txn);
    tokio::time::advance(Duration::from_millis(4)).await;
    assert!(!task.is_finished());
    tokio::time::advance(Duration::from_millis(1)).await;
    task.await.unwrap().unwrap();
    *config.write().unwrap() = KeyspaceSplitConfig {
        interval: Duration::from_millis(30),
        timeout: Duration::from_millis(90),
    };
    let worker = split.clone();
    let task = tokio::spawn(async move {
        worker
            .wait_for_boundaries(&CancellationToken::new(), 42, BoundType::Txn)
            .await
    });
    tokio::task::yield_now().await;
    config.write().unwrap().timeout = Duration::from_secs(30);
    tokio::time::advance(Duration::from_millis(90)).await;
    assert_eq!(
        task.await.unwrap().unwrap_err().to_string(),
        "[PD:keyspace:ErrRegionSplitTimeout]region split timeout"
    );
}
#[tokio::test]
async fn mcs_036_actual_region_index_rejects_containing_ranges_and_wrong_mode() {
    use astersql_pd_core::{KeyRange, RegionInfo, RegionMeta};
    let mut regions = RegionsInfo::new();
    regions.put_region(Arc::new(RegionInfo::new(
        RegionMeta {
            id: 1,
            key_range: KeyRange::default(),
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    )));
    let split = KeyspaceRegionSplit::new(
        None,
        Some(regions.clone()),
        Arc::new(RwLock::new(KeyspaceSplitConfig::default())),
    );
    assert!(!split.has_boundaries(36, BoundType::Txn));
    add_boundaries(&mut regions, 36, BoundType::Txn);
    assert!(split.has_boundaries(36, BoundType::Txn));
    assert!(!split.has_boundaries(36, BoundType::Raw));
    assert!(!split.has_boundaries(37, BoundType::Txn));
    add_boundaries(&mut regions, 36, BoundType::Raw);
    assert!(split.has_boundaries(36, BoundType::Raw));
    assert!(!split.has_boundaries(36, BoundType::Txn));
    add_boundaries(&mut regions, 0x00ff_ffff, BoundType::Txn);
    assert!(split.has_boundaries(0x00ff_ffff, BoundType::Txn));
}
#[tokio::test]
async fn mcs_036_timeout_cancel_cleanup_and_same_wal_rules() {
    use astersql_pd_storage_domain::{Consistency, Read};
    use std::time::Duration;
    let mut fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(42);
    let labeler = Arc::new(Mutex::new(
        RegionLabeler::load(StorageEndpoint::new(store.clone()), paths, 0)
            .await
            .unwrap(),
    ));
    let config = Arc::new(RwLock::new(KeyspaceSplitConfig {
        interval: Duration::from_millis(10),
        timeout: Duration::from_millis(80),
    }));
    let split = KeyspaceRegionSplit::new(
        Some(labeler.clone()),
        Some(RegionsInfo::new()),
        config.clone(),
    );
    let error = split
        .split(&CancellationToken::new(), 37, true, BoundType::Txn, 0)
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "[PD:keyspace:ErrRegionSplitTimeout]region split timeout"
    );
    assert!(error.cleanup_error.is_none());
    assert!(labeler.lock().await.index().rule("keyspaces/37").is_none());
    assert!(
        store
            .read(Read::range(
                paths.region_label_key_path("keyspaces/37").as_bytes(),
                b"",
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    config.write().unwrap().timeout = Duration::from_secs(30);
    let cancel = CancellationToken::new();
    let worker = split.clone();
    let request_cancel = cancel.clone();
    let task = tokio::spawn(async move {
        worker
            .split(&request_cancel, 38, true, BoundType::Txn, 0)
            .await
    });
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if labeler.lock().await.index().rule("keyspaces/38").is_some() {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline);
        tokio::task::yield_now().await;
    }
    cancel.cancel();
    let error = task.await.unwrap().unwrap_err();
    assert_eq!(error.to_string(), "[keyspace] wait region split canceled");
    assert!(error.cleanup_error.is_none());
    assert!(labeler.lock().await.index().rule("keyspaces/38").is_none());
    split
        .split(&CancellationToken::new(), 39, false, BoundType::Raw, 0)
        .await
        .unwrap();
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let restored = RegionLabeler::load(
        StorageEndpoint::new(Arc::new(fixture.store().await)),
        paths,
        0,
    )
    .await
    .unwrap();
    assert!(restored.index().rule("keyspaces/39").is_some());
    assert!(restored.index().rule("keyspaces/37").is_none());
    assert!(restored.index().rule("keyspaces/38").is_none());
}
#[tokio::test]
async fn mcs_036_cleanup_failure_preserves_cancellation_and_persisted_rule() {
    use std::time::Duration;
    let mut fixture = fixture::Fixture::start();
    let paths = ClusterPaths::new(42);
    let labeler = Arc::new(Mutex::new(
        RegionLabeler::load(
            StorageEndpoint::new(Arc::new(fixture.store().await)),
            paths,
            0,
        )
        .await
        .unwrap(),
    ));
    let split = KeyspaceRegionSplit::new(
        Some(labeler.clone()),
        Some(RegionsInfo::new()),
        Arc::new(RwLock::new(KeyspaceSplitConfig::default())),
    );
    let cancel = CancellationToken::new();
    let worker = split.clone();
    let request_cancel = cancel.clone();
    let task = tokio::spawn(async move {
        worker
            .split(&request_cancel, 50, true, BoundType::Txn, 0)
            .await
    });
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if labeler.lock().await.index().rule("keyspaces/50").is_some() {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline);
        tokio::task::yield_now().await;
    }
    fixture.command("stop", "stopped");
    cancel.cancel();
    let error = task.await.unwrap().unwrap_err();
    assert_eq!(error.to_string(), "[keyspace] wait region split canceled");
    assert!(error.cleanup_error.is_some());
    assert!(labeler.lock().await.index().rule("keyspaces/50").is_some());
    fixture.command("start", "restarted");
    let restored = RegionLabeler::load(
        StorageEndpoint::new(Arc::new(fixture.store().await)),
        paths,
        0,
    )
    .await
    .unwrap();
    assert!(restored.index().rule("keyspaces/50").is_some());
}
#[tokio::test]
async fn mcs_036_unsupported_labeler_and_actual_set_failure() {
    let config = Arc::new(RwLock::new(KeyspaceSplitConfig::default()));
    let split = KeyspaceRegionSplit::new(None, None, config.clone());
    assert_eq!(
        split
            .split(&CancellationToken::new(), 51, false, BoundType::Txn, 0)
            .await
            .unwrap_err()
            .to_string(),
        "cluster does not support region label"
    );
    let mut fixture = fixture::Fixture::start();
    let paths = ClusterPaths::new(42);
    let labeler = Arc::new(Mutex::new(
        RegionLabeler::load(
            StorageEndpoint::new(Arc::new(fixture.store().await)),
            paths,
            0,
        )
        .await
        .unwrap(),
    ));
    let split = KeyspaceRegionSplit::new(Some(labeler.clone()), None, config);
    fixture.command("stop", "stopped");
    let error = split
        .split(&CancellationToken::new(), 51, false, BoundType::Txn, 0)
        .await
        .unwrap_err();
    assert!(matches!(error.error, KeyspaceSplitError::Labeler(_)));
    assert!(error.cleanup_error.is_none());
    assert!(labeler.lock().await.index().rule("keyspaces/51").is_none());
}
