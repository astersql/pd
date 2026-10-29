// Copyright 2026 AsterSQL.
use astersql_pd_core::{
    BasicCluster, KeyRange, NodeState, Peer, RegionEpoch, RegionInfo, RegionMeta, StoreInfo,
    StoreMeta,
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_server::{
    grpc_split_regions::SplitRequest, region_heartbeat_adapter::RegionHeartbeatService,
};
use astersql_pd_storage_endpoint::StorageEndpoint;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
fn region(id: u64, start: &[u8], end: &[u8], version: u64) -> RegionInfo {
    let peer = Peer {
        id: 3,
        store_id: 1,
        ..Default::default()
    };
    let mut r = RegionInfo::new(
        RegionMeta {
            id,
            key_range: KeyRange {
                start_key: start.to_vec(),
                end_key: end.to_vec(),
            },
            peers: vec![peer.clone()],
            epoch: Some(RegionEpoch {
                version,
                conf_ver: 1,
            }),
            ..Default::default()
        },
        Some(peer),
        vec![],
        vec![],
    );
    r.set_source(astersql_pd_core::RegionSource::Heartbeat);
    r
}
async fn tick(
    host: &crate::cluster_host::ClusterHost,
    status: &mut tokio::sync::watch::Receiver<crate::cluster_host::ScheduleStatus>,
    duration: Duration,
) {
    let before = status.borrow().ticks;
    tokio::time::advance(duration).await;
    while status.borrow().ticks <= before {
        status.changed().await.unwrap();
    }
    host.operator(0).await.unwrap();
}
#[tokio::test(start_paused = true)]
async fn task_030_clock_timeout_and_cancelled_partial_result() {
    let store = Arc::new(astersql_pd_storage::MemoryStore::default());
    crate::node_test::seed_bootstrap(store.clone(), 3030).await;
    let endpoint = StorageEndpoint::new(store.clone());
    let paths = ClusterPaths::new(3030);
    endpoint.save_config(&paths,&serde_json::json!({"replication":{"max-replicas":1,"enable-placement-rules":"false"},"schedule":{"schedulers-v2":[]}})).await.unwrap();
    endpoint
        .save_region(&paths, region(100, b"a", b"z", 3).meta())
        .await
        .unwrap();
    let cluster = BasicCluster::new();
    cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 1,
        node_state: NodeState::Serving,
        ..Default::default()
    })));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node =
        crate::PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .with_cluster(cluster)
            .start(
                store,
                crate::NodeConfig {
                    cluster_id: 3030,
                    member_id: 1,
                    name: "split-clock".into(),
                    client_url: url,
                    peer_url: "http://peer".into(),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    node.ready().await.unwrap();
    let host = node.cluster();
    let mut status = host.watch_schedule();
    host.region_heartbeat(region(100, b"a", b"z", 3))
        .await
        .unwrap();
    tokio::time::advance(Duration::from_secs(3)).await;
    while status.borrow().phase != crate::cluster_host::SchedulePhase::Running {
        status.changed().await.unwrap();
    }
    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    host.bind_heartbeat(1, Arc::new(tx)).await.unwrap();
    let cancellation = CancellationToken::new();
    let token = cancellation.clone();
    let caller = host.clone();
    let task = tokio::spawn(async move {
        caller
            .split_regions(SplitRequest {
                keys: vec![b"m".to_vec(), b"t".to_vec()],
                retries: 0,
                cancellation: token,
            })
            .await
    });
    let command = rx.recv().await.unwrap();
    assert_eq!(command.region_id, 100);
    host.region_heartbeat(region(101, b"a", b"m", 4))
        .await
        .unwrap();
    host.region_heartbeat(region(102, b"m", b"z", 4))
        .await
        .unwrap();
    tick(&host, &mut status, Duration::from_millis(100)).await;
    host.operator(100).await.unwrap();
    assert!(!task.is_finished());
    cancellation.cancel();
    tick(&host, &mut status, Duration::from_millis(10)).await;
    tick(&host, &mut status, Duration::from_millis(110)).await;
    let result = task.await.unwrap().unwrap();
    assert_eq!(result.percentage, 50);
    assert_eq!(result.ids, vec![102]);
    // A true 60 second attempt deadline, advanced on the host clock, reports no
    // progress when there has been admission but no matching boundary heartbeat.
    let caller = host.clone();
    let task = tokio::spawn(async move {
        caller
            .split_regions(SplitRequest {
                keys: vec![b"d".to_vec()],
                retries: 0,
                cancellation: CancellationToken::new(),
            })
            .await
    });
    loop {
        if rx.recv().await.unwrap().region_id == 101 {
            break;
        }
    }
    for _ in 0..59 {
        tick(&host, &mut status, Duration::from_secs(1)).await;
    }
    assert!(!task.is_finished());
    tick(&host, &mut status, Duration::from_secs(1)).await;
    tick(&host, &mut status, Duration::from_millis(110)).await;
    let result = task.await.unwrap().unwrap();
    assert_eq!(result.percentage, 0);
    assert!(result.ids.is_empty());
    // Cancellation ends each watch, while the inclusive retry and its final
    // backoff still run. Already-admitted operations remain real operations.
    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    host.bind_heartbeat(1, Arc::new(tx)).await.unwrap();
    let caller = host.clone();
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let task = tokio::spawn(async move {
        caller
            .split_regions(SplitRequest {
                keys: vec![b"x".to_vec()],
                retries: 1,
                cancellation: cancelled,
            })
            .await
    });
    loop {
        if rx.recv().await.unwrap().region_id == 102 {
            break;
        }
    }
    tick(&host, &mut status, Duration::from_millis(10)).await;
    tick(&host, &mut status, Duration::from_millis(110)).await;
    assert!(
        !task.is_finished(),
        "cancelled request must retain its outer retry"
    );
    tick(&host, &mut status, Duration::from_millis(10)).await;
    tick(&host, &mut status, Duration::from_millis(210)).await;
    let result = task.await.unwrap().unwrap();
    assert_eq!(result.percentage, 0);
    assert!(result.ids.is_empty());
    // uint64 values above MaxInt64 become negative Go int values: no attempt.
    let result = host
        .split_regions(SplitRequest {
            keys: vec![b"x".to_vec()],
            retries: -1,
            cancellation: CancellationToken::new(),
        })
        .await
        .unwrap();
    assert_eq!(result.percentage, 0);
    node.shutdown().await.unwrap();
}
