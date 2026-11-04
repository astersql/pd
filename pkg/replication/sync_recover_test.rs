// Copyright 2026 AsterSQL.
use crate::async_confirm_test::go_async_scenario;
use crate::async_wait_test::{local_manager, rules, stores};
use crate::{DRAutoSyncStatus, StoreDRStatus};
use astersql_pd_core::{KeyRange, RegionEpoch, RegionInfo, RegionMeta, RegionsInfo, StoreInfo};
use astersql_pd_proto::{metapb, pdpb, replication_modepb::RegionReplicationStatus};
use astersql_pd_storage_domain::{Consistency, MemoryStore, MetaStore, Read, StorageEndpoint};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

fn region(
    id: u64,
    start: &[u8],
    end: &[u8],
    version: u64,
    state: i32,
    state_id: u64,
) -> Arc<RegionInfo> {
    let mut r = RegionInfo::new(
        RegionMeta {
            id,
            key_range: KeyRange {
                start_key: start.into(),
                end_key: end.into(),
            },
            epoch: Some(RegionEpoch {
                version,
                conf_ver: 1,
            }),
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    );
    r.set_replication_status(state, state_id);
    Arc::new(r)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_007_behavior() {
    let now = chrono::Utc::now().timestamp_nanos_opt().unwrap();
    let metas = stores(now);
    let (mut server, backend, paths, _) = go_async_scenario(&metas).await;
    let mut client = pdpb::pd_client::PdClient::connect(server.url.clone())
        .await
        .unwrap();
    let cluster_id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let r = client
        .store_heartbeat(pdpb::StoreHeartbeatRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id,
                ..Default::default()
            }),
            stats: Some(pdpb::StoreStats {
                store_id: 700,
                capacity: 1_000_000_000,
                available: 900_000_000,
                used_size: 100_000_000,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(r.header.unwrap().error.is_none());
    let recovering = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let v = server.get("/pd/api/v1/replication_mode/status").unwrap().1;
            if v["dr-auto-sync"]["state"] == "sync_recover" {
                break v;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(recovering["dr-auto-sync"]["state_id"], 4);
    let load = || {
        backend.read(Read::prefix(
            paths.replication_mode_path("dr-auto-sync").as_bytes(),
            Consistency::Linearizable,
        ))
    };
    let initial_record = load().await.unwrap().entries[0].value.clone();
    let mut send = client.clone();
    let (tx, rx) = tokio::sync::mpsc::channel(8);
    let stream = tokio::spawn(async move {
        let response = send
            .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
            .await;
        if let Ok(response) = response {
            let mut stream = response.into_inner();
            while let Some(message) = stream.message().await.unwrap() {
                assert!(message.header.unwrap().error.is_none());
            }
        }
    });
    for (id, start, end, state) in [
        (5003, b"m".as_slice(), b"".as_slice(), 1),
        (5001, b"".as_slice(), b"m".as_slice(), 2),
    ] {
        tx.send(pdpb::RegionHeartbeatRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id,
                ..Default::default()
            }),
            region: Some(metapb::Region {
                id,
                start_key: start.into(),
                end_key: end.into(),
                region_epoch: Some(metapb::RegionEpoch {
                    version: 2,
                    conf_ver: 1,
                }),
                peers: vec![metapb::Peer {
                    id: 5002,
                    store_id: 100,
                    ..Default::default()
                }],
                ..Default::default()
            }),
            leader: Some(metapb::Peer {
                id: 5002,
                store_id: 100,
                ..Default::default()
            }),
            replication_status: Some(RegionReplicationStatus { state, state_id: 4 }),
            ..Default::default()
        })
        .await
        .unwrap();
    }
    let partial = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let v = server.get("/pd/api/v1/replication_mode/status").unwrap().1;
            if v["dr-auto-sync"]["recover_progress"] == 0.5 {
                break v;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(partial["dr-auto-sync"]["total_regions"], 2);
    assert_eq!(partial["dr-auto-sync"]["synced_regions"], 1);
    assert_eq!(
        load().await.unwrap().entries[0].value,
        initial_record,
        "sample progress is not a persisted transition"
    );
    tx.send(pdpb::RegionHeartbeatRequest {
        header: Some(pdpb::RequestHeader {
            cluster_id,
            ..Default::default()
        }),
        region: Some(metapb::Region {
            id: 5001,
            region_epoch: Some(metapb::RegionEpoch {
                version: 3,
                conf_ver: 1,
            }),
            peers: vec![metapb::Peer {
                id: 5002,
                store_id: 100,
                ..Default::default()
            }],
            ..Default::default()
        }),
        leader: Some(metapb::Peer {
            id: 5002,
            store_id: 100,
            ..Default::default()
        }),
        replication_status: Some(RegionReplicationStatus {
            state: 2,
            state_id: 4,
        }),
        ..Default::default()
    })
    .await
    .unwrap();
    let final_record = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let rows = load().await.unwrap();
            let v: Value = serde_json::from_slice(&rows.entries[0].value).unwrap();
            if v["state"] == "sync" {
                break v;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(final_record, json!({"state":"sync","state_id":5}));
    println!(
        "Go recovery partial: {partial}; initial durable: {}; merged final: {final_record}",
        String::from_utf8_lossy(&initial_record)
    );
    drop(tx);
    stream.abort();
    let _ = stream.await;
    server.stop().unwrap();

    let backend: Arc<dyn MetaStore> = Arc::new(MemoryStore::default());
    let (manager, leader, _, paths) = local_manager(backend.clone(), 707).await;
    let mut config = manager.snapshot().await.config;
    config.dr_auto_sync.wait_store_timeout =
        astersql_pd_typeutil::Duration::from_nanos(3_600_000_000_000);
    config.dr_auto_sync.pause_region_split = true;
    manager.update_config(config).await.unwrap();
    let mut infos: Vec<_> = metas
        .into_iter()
        .map(|s| Arc::new(StoreInfo::new(s)))
        .collect();
    manager
        .tick_sync(&infos, &rules(), now as i128)
        .await
        .unwrap();
    for id in [100, 200, 300, 400] {
        manager
            .update_store_dr_status(
                id,
                StoreDRStatus {
                    state: "async_wait".into(),
                    state_id: 2,
                },
            )
            .await;
    }
    manager
        .tick_async_wait(&infos, &rules(), now as i128)
        .await
        .unwrap();
    Arc::make_mut(&mut infos[6]).set_last_heartbeat_unix_nanos(now);
    let after = chrono::Utc::now().timestamp_nanos_opt().unwrap() as i128 + 1;
    assert!(
        manager.tick_async(&infos, &rules(), after).await.unwrap(),
        "Go entered recovery when DR returned"
    );
    assert_eq!(manager.snapshot().await.dr_auto_sync.state_id, 4);
    let mut regions = RegionsInfo::new();
    regions.put_region(region(5001, b"", b"m", 2, 2, 4));
    regions.put_region(region(5003, b"m", b"", 2, 1, 4));
    manager
        .tick_recover(&infos, &rules(), &regions, after)
        .await
        .unwrap();
    let p = manager.snapshot().await.dr_auto_sync;
    assert_eq!(
        (p.total_regions, p.synced_regions, p.recover_progress),
        (2, 1, 0.5)
    );
    regions.put_region(region(5001, b"", b"", 3, 2, 4));
    assert!(
        manager
            .tick_recover(&infos, &rules(), &regions, after)
            .await
            .unwrap()
    );
    assert_eq!(
        serde_json::to_value(manager.snapshot().await.dr_auto_sync).unwrap(),
        final_record
    );
    assert!(!manager.is_region_split_paused().await);
    assert_eq!(
        StorageEndpoint::new(backend)
            .load_replication_status::<DRAutoSyncStatus>(&paths, "dr-auto-sync")
            .await
            .unwrap(),
        Some(manager.snapshot().await.dr_auto_sync)
    );
    leader.resign().await.unwrap();
}

async fn enter_recovery(manager: &crate::ModeManager) -> u64 {
    let mut config = manager.snapshot().await.config;
    config.replication_mode = "majority".into();
    manager.update_config(config.clone()).await.unwrap();
    config.replication_mode = "dr-auto-sync".into();
    manager.update_config(config).await.unwrap();
    manager.snapshot().await.dr_auto_sync.state_id
}

#[tokio::test]
async fn mcs_007_sampling_and_region_gaps() {
    for (case, broken) in [(0, 9), (1, 1020)] {
        let backend: Arc<dyn MetaStore> = Arc::new(MemoryStore::default());
        let (manager, leader, _, paths) = local_manager(backend.clone(), 710 + case).await;
        let id = enter_recovery(&manager).await;
        let mut regions = RegionsInfo::new();
        for i in 0..1030_u64 {
            let start = if i == 0 {
                vec![]
            } else {
                i.to_be_bytes().to_vec()
            };
            let end = if i == 1029 {
                vec![]
            } else {
                (i + 1).to_be_bytes().to_vec()
            };
            let wrong = if case == 0 { i >= broken } else { i == broken };
            regions.put_region(region(
                i + 1,
                &start,
                &end,
                1,
                2,
                if wrong { id - 1 } else { id },
            ));
        }
        assert!(
            !manager
                .tick_recover(&[], &rules(), &regions, 0)
                .await
                .unwrap()
        );
        let partial = manager.snapshot().await.dr_auto_sync;
        assert_eq!(partial.synced_regions, broken as i64);
        assert_eq!(partial.total_regions, 1030);
        assert_eq!(
            partial.recover_progress,
            if case == 0 {
                9.0_f32 / 1030.0
            } else {
                1029.0_f32 / 1030.0
            }
        );
        assert!(partial.recover_progress < 1.0);
        let durable = StorageEndpoint::new(backend)
            .load_replication_status::<DRAutoSyncStatus>(&paths, "dr-auto-sync")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                durable.total_regions,
                durable.synced_regions,
                durable.recover_progress
            ),
            (0, 0, 0.0)
        );
        // Repair the remaining range in one merge, covering the retained cursor.
        regions.put_region(region(broken + 1, &broken.to_be_bytes(), b"", 2, 2, id));
        assert!(
            manager
                .tick_recover(&[], &rules(), &regions, 0)
                .await
                .unwrap()
        );
        assert_eq!(manager.snapshot().await.dr_auto_sync.state, "sync");
        leader.resign().await.unwrap();
    }
    let backend: Arc<dyn MetaStore> = Arc::new(MemoryStore::default());
    let (manager, leader, _, _) = local_manager(backend, 712).await;
    let id = enter_recovery(&manager).await;
    let mut regions = RegionsInfo::new();
    regions.put_region(region(1, b"a", b"b", 1, 2, id));
    regions.put_region(region(2, b"b", b"", 1, 2, id));
    assert!(
        !manager
            .tick_recover(&[], &rules(), &regions, 0)
            .await
            .unwrap()
    );
    let partial = manager.snapshot().await.dr_auto_sync;
    assert_eq!((partial.synced_regions, partial.recover_progress), (0, 0.5));
    regions.put_region(region(3, b"", b"a", 1, 2, id));
    assert!(
        manager
            .tick_recover(&[], &rules(), &regions, 0)
            .await
            .unwrap()
    );
    leader.resign().await.unwrap();
}

async fn enter_async(manager: &crate::ModeManager, infos: &[Arc<StoreInfo>], now: i128) {
    assert!(manager.tick_sync(infos, &rules(), now).await.unwrap());
    let id = manager.snapshot().await.dr_auto_sync.state_id;
    for store in [100, 200, 300, 400] {
        manager
            .update_store_dr_status(
                store,
                StoreDRStatus {
                    state: "async_wait".into(),
                    state_id: id,
                },
            )
            .await;
    }
    assert!(manager.tick_async_wait(infos, &rules(), now).await.unwrap());
}

#[tokio::test]
async fn mcs_007_recovery_timeout_and_async_membership() {
    let backend: Arc<dyn MetaStore> = Arc::new(MemoryStore::default());
    let (manager, leader, _, _) = local_manager(backend, 713).await;
    let now = chrono::Utc::now().timestamp_nanos_opt().unwrap();
    let mut infos: Vec<_> = stores(now)
        .into_iter()
        .map(|s| Arc::new(StoreInfo::new(s)))
        .collect();
    enter_async(&manager, &infos, now as i128).await;
    let mut config = manager.snapshot().await.config;
    config.dr_auto_sync.wait_recover_timeout = astersql_pd_typeutil::Duration::from_nanos(100);
    config.dr_auto_sync.wait_store_timeout =
        astersql_pd_typeutil::Duration::from_nanos(3_600_000_000_000);
    manager.update_config(config).await.unwrap();
    let start = manager
        .snapshot()
        .await
        .dr_auto_sync
        .async_start
        .unwrap()
        .timestamp_nanos_opt()
        .unwrap() as i128;
    Arc::make_mut(&mut infos[6]).set_last_heartbeat_unix_nanos(now);
    for elapsed in [99, 100] {
        assert!(
            !manager
                .tick_async(&infos, &rules(), start + elapsed)
                .await
                .unwrap()
        );
    }
    assert!(
        manager
            .tick_async(&infos, &rules(), start + 101)
            .await
            .unwrap()
    );
    assert_eq!(manager.snapshot().await.dr_auto_sync.state, "sync_recover");
    assert_eq!(manager.snapshot().await.dr_auto_sync.state_id, 4);
    // Loss of DR while a primary majority survives returns directly to async.
    Arc::make_mut(&mut infos[6]).set_last_heartbeat_unix_nanos(0);
    assert!(
        manager
            .tick_recover(&infos, &rules(), &RegionsInfo::new(), start + 102)
            .await
            .unwrap()
    );
    let current = manager.snapshot().await.dr_auto_sync;
    assert_eq!(current.state, "async");
    assert_eq!(current.state_id, 5);
    Arc::make_mut(&mut infos[0]).set_last_heartbeat_unix_nanos(0);
    assert!(
        !manager
            .tick_async(&infos, &rules(), start + 103)
            .await
            .unwrap()
    );
    for store in [200, 300, 400] {
        manager
            .update_store_dr_status(
                store,
                StoreDRStatus {
                    state: "async".into(),
                    state_id: 5,
                },
            )
            .await;
    }
    assert!(
        manager
            .tick_async(&infos, &rules(), start + 104)
            .await
            .unwrap()
    );
    let changed = manager.snapshot().await.dr_auto_sync;
    assert_eq!(changed.state_id, 6);
    assert_eq!(changed.available_stores, [200, 300, 400]);
    assert!(changed.async_start.is_some());
    leader.resign().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_007_real_write_failure_preserves_recovery() {
    use astersql_pd_storage_kv::EtcdMetaStore;
    let fixture = crate::fixture::Fixture::start();
    let proxy = crate::mode_proxy_test::Proxy::start().await;
    proxy.target(
        fixture
            .endpoint
            .trim_start_matches("http://")
            .parse()
            .unwrap(),
    );
    let backend: Arc<dyn MetaStore> = Arc::new(
        EtcdMetaStore::connect(
            &[format!("http://{}", proxy.address)],
            Duration::from_millis(500),
        )
        .await
        .unwrap(),
    );
    let (manager, leader, cancel, paths) = local_manager(backend.clone(), 714).await;
    let now = chrono::Utc::now().timestamp_nanos_opt().unwrap();
    let mut infos: Vec<_> = stores(now)
        .into_iter()
        .map(|s| Arc::new(StoreInfo::new(s)))
        .collect();
    enter_async(&manager, &infos, now as i128).await;
    let mut config = manager.snapshot().await.config;
    config.dr_auto_sync.wait_store_timeout =
        astersql_pd_typeutil::Duration::from_nanos(3_600_000_000_000);
    manager.update_config(config).await.unwrap();
    let original = manager.snapshot().await;
    Arc::make_mut(&mut infos[6]).set_last_heartbeat_unix_nanos(now);
    let later = chrono::Utc::now().timestamp_nanos_opt().unwrap() as i128 + 1;
    proxy.block(paths.replication_mode_path("dr-auto-sync").as_bytes());
    assert!(matches!(
        manager.tick_async(&infos, &rules(), later).await,
        Err(crate::ModeError::Storage(_))
    ));
    assert_eq!(manager.snapshot().await, original);
    proxy.unblock();
    assert!(manager.tick_async(&infos, &rules(), later).await.unwrap());
    let recovery = manager.snapshot().await;
    assert_eq!(recovery.dr_auto_sync.state_id, 5);
    let mut regions = RegionsInfo::new();
    regions.put_region(region(1, b"", b"", 1, 2, 5));
    proxy.block(paths.replication_mode_path("dr-auto-sync").as_bytes());
    assert!(matches!(
        manager
            .tick_recover(&infos, &rules(), &regions, later)
            .await,
        Err(crate::ModeError::Storage(_))
    ));
    assert_eq!(manager.snapshot().await, recovery);
    proxy.unblock();
    let endpoint = StorageEndpoint::new(backend);
    assert_eq!(
        endpoint
            .load_replication_status::<DRAutoSyncStatus>(&paths, "dr-auto-sync")
            .await
            .unwrap(),
        Some(recovery.dr_auto_sync)
    );
    assert!(
        manager
            .tick_recover(&infos, &rules(), &regions, later)
            .await
            .unwrap()
    );
    let saved = manager.snapshot().await;
    assert_eq!(saved.dr_auto_sync.state_id, 7);
    assert_eq!(saved.dr_auto_sync.state, "sync");
    cancel.cancel();
    assert!(matches!(
        manager
            .tick_recover(&infos, &rules(), &regions, later)
            .await,
        Err(crate::ModeError::Cancelled)
    ));
    assert_eq!(manager.snapshot().await, saved);
    leader.resign().await.unwrap();
    proxy.stop().await;
}
