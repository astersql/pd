// Copyright 2026 AsterSQL.
use super::node_test::fixture;
use super::*;
use astersql_pd_core::{BasicCluster, NodeState, StoreInfo, StoreMeta};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_storage_endpoint::StorageEndpoint;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

async fn sync_scenario() -> serde_json::Value {
    let fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let registry = Arc::new(prometheus::Registry::new());
    let parent = CancellationToken::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let url = format!("http://{address}");
    let cluster = BasicCluster::new();
    cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 1,
        node_state: NodeState::Serving,
        ..Default::default()
    })));
    let config = NodeConfig {
        cluster_id: 505,
        member_id: 5,
        name: "shared".into(),
        client_url: url.clone(),
        peer_url: "http://127.0.0.1:2380".into(),
    };
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), parent.clone())
        .with_cluster(cluster)
        .start(store.clone(), config.clone(), listener, registry.clone())
        .await
        .unwrap();
    node.ready().await.unwrap();
    let host = node.cluster();
    let mut client = PdClient::connect(url.clone()).await.unwrap();
    let storage = StorageEndpoint::new(store.clone());
    let observed = exercise(&mut client, 505, &storage).await;
    let snapshot = host.snapshot().await.unwrap();
    assert_eq!(
        snapshot.regions[0].meta().epoch.as_ref().unwrap().version,
        5
    );
    assert_eq!(
        storage
            .load_region(&ClusterPaths::new(505), 7)
            .await
            .unwrap(),
        Some(snapshot.regions[0].meta().clone())
    );
    let syncer = host.region_sync_handle().unwrap();
    let (sync_tx, mut stream) = subscribe(&mut client, 505, u64::MAX).await;
    assert_eq!(
        next(&mut stream).await.regions[0]
            .region_epoch
            .as_ref()
            .unwrap()
            .version,
        5
    );
    next(&mut stream).await;
    parent.cancel();
    let error = tokio::time::timeout(Duration::from_secs(5), stream.message())
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::Unavailable);
    drop((sync_tx, stream, client));
    tokio::time::timeout(Duration::from_secs(5), node.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(syncer.downstream_names().is_empty());
    assert!(host.region_sync_handle().is_some()); // this test owns the last handle
    drop(syncer);
    assert!(host.region_sync_handle().is_none());
    assert!(host.snapshot().await.is_err());
    drop(storage);
    assert_eq!(Arc::strong_count(&store), 1);
    assert!(registry.gather().is_empty());
    assert!(tokio::net::TcpStream::connect(address).await.is_err());
    observed
}
fn report(cluster_id: u64, version: u64) -> pdpb::RegionHeartbeatRequest {
    let peer = metapb::Peer {
        id: 2,
        store_id: 1,
        ..Default::default()
    };
    pdpb::RegionHeartbeatRequest {
        header: Some(pdpb::RequestHeader {
            cluster_id,
            ..Default::default()
        }),
        region: Some(metapb::Region {
            id: 7,
            peers: vec![peer.clone()],
            region_epoch: Some(metapb::RegionEpoch {
                version,
                conf_ver: 1,
            }),
            ..Default::default()
        }),
        leader: Some(peer),
        approximate_size: version * 1048576,
        approximate_keys: version * 10,
        bytes_written: version * 100,
        bytes_read: version * 200,
        keys_written: version * 3,
        keys_read: version * 4,
        ..Default::default()
    }
}
async fn wait_saved(storage: &StorageEndpoint, id: u64, version: u64) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if storage
                .load_region(&ClusterPaths::new(id), 7)
                .await
                .unwrap()
                .is_some_and(|r| r.epoch.unwrap().version == version)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
async fn exercise(
    client: &mut PdClient<tonic::transport::Channel>,
    id: u64,
    storage: &StorageEndpoint,
) -> serde_json::Value {
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    tx.send(report(id, 2)).await.unwrap();
    let mut heartbeat_client = client.clone();
    let hb = tokio::spawn(async move {
        heartbeat_client
            .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
            .await
            .unwrap()
            .into_inner()
            .message()
            .await
            .unwrap()
            .unwrap()
    });
    wait_saved(storage, id, 2).await;
    let (sync_tx, mut stream) = subscribe(client, id, 0).await;
    let full = next(&mut stream).await;
    check_frame(&full, id, 2);
    assert_eq!(full.start_index, 0);
    let mut completion = next(&mut stream).await;
    while !completion.regions.is_empty() {
        completion = next(&mut stream).await;
    }
    let before = completion.start_index;
    tx.send(report(id, 3)).await.unwrap();
    let delta = until_version(&mut stream, 3).await;
    check_frame(&delta, id, 3);
    assert!(delta.start_index >= before);
    let resume = delta.start_index + delta.regions.len() as u64;
    assert!(resume > 0);
    drop((sync_tx, stream));
    tx.send(report(id, 4)).await.unwrap();
    wait_saved(storage, id, 4).await;
    let (sync_tx, mut stream) = subscribe(client, id, resume).await;
    let replay = until_version(&mut stream, 4).await;
    assert_eq!(
        replay.start_index, resume,
        "resume uses history cursor, not snapshot offset or etcd revision"
    );
    check_frame(&replay, id, 4);
    drop(sync_tx); // Go owns the response stream after its first request, even after half-close.
    tx.send(report(id, 5)).await.unwrap();
    let live = until_version(&mut stream, 5).await;
    assert_eq!(live.start_index, resume + 1);
    check_frame(&live, id, 5);
    wait_saved(storage, id, 5).await;
    drop(stream);
    // Out-of-range cursors recover by a fresh snapshot.
    let (sync_tx, mut stream) = subscribe(client, id, u64::MAX).await;
    check_frame(&next(&mut stream).await, id, 5);
    let completion = next(&mut stream).await;
    assert!(completion.regions.is_empty());
    assert_eq!(completion.start_index, resume + 2);
    drop((sync_tx, stream));
    let wrong = client
        .sync_regions(tokio_stream::iter([request(id + 1, 0)]))
        .await;
    let error = match wrong {
        Err(e) => e,
        Ok(r) => r.into_inner().message().await.unwrap_err(),
    };
    assert_eq!(error.code(), tonic::Code::FailedPrecondition);
    assert!(error.message().contains("mismatch cluster id"));
    let mut empty = client
        .sync_regions(tokio_stream::empty::<pdpb::SyncRegionRequest>())
        .await
        .unwrap()
        .into_inner();
    assert!(empty.message().await.unwrap().is_none());
    // A rejected stale heartbeat must not create another history record.
    tx.send(report(id, 1)).await.unwrap();
    let response = tokio::time::timeout(Duration::from_secs(5), hb)
        .await
        .unwrap()
        .unwrap();
    assert!(response.header.unwrap().error.is_some());
    let (sync_tx, mut stream) = subscribe(client, id, completion.start_index).await;
    let unchanged = next(&mut stream).await;
    assert!(unchanged.regions.is_empty());
    assert_eq!(unchanged.start_index, completion.start_index);
    drop((sync_tx, stream, tx));
    serde_json::json!({"snapshot":2,"delta":3,"replay":4,"live":5,"fallback":5,"stale_rejected":true,"wrong_cluster":"FailedPrecondition","empty_eof":true,"request_half_close":true})
}
fn check_frame(frame: &pdpb::SyncRegionResponse, id: u64, version: u64) {
    assert_eq!(frame.header.as_ref().unwrap().cluster_id, id);
    assert!(frame.header.as_ref().unwrap().error.is_none());
    assert_eq!(frame.regions.len(), 1);
    assert_eq!(frame.regions[0], report(id, version).region.unwrap());
    assert_eq!(
        frame.region_leaders,
        vec![report(id, version).leader.unwrap()]
    );
    assert_eq!(frame.region_stats.len(), 1);
    assert_eq!(frame.region_stats[0].bytes_written, version * 100);
    assert_eq!(frame.region_stats[0].bytes_read, version * 200);
    assert_eq!(frame.buckets.len(), 1);
}
async fn until_version(
    stream: &mut tonic::Streaming<pdpb::SyncRegionResponse>,
    version: u64,
) -> pdpb::SyncRegionResponse {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let frame = next(stream).await;
            if frame
                .regions
                .iter()
                .any(|r| r.region_epoch.as_ref().unwrap().version == version)
            {
                return frame;
            }
        }
    })
    .await
    .unwrap()
}
fn request(cluster_id: u64, start_index: u64) -> pdpb::SyncRegionRequest {
    pdpb::SyncRegionRequest {
        header: Some(pdpb::RequestHeader {
            cluster_id,
            ..Default::default()
        }),
        member: Some(pdpb::Member {
            name: "task011".into(),
            client_urls: vec!["http://127.0.0.1:29998".into()],
            ..Default::default()
        }),
        start_index,
    }
}
async fn subscribe(
    client: &mut PdClient<tonic::transport::Channel>,
    cluster_id: u64,
    start_index: u64,
) -> (
    tokio::sync::mpsc::Sender<pdpb::SyncRegionRequest>,
    tonic::Streaming<pdpb::SyncRegionResponse>,
) {
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    tx.send(request(cluster_id, start_index)).await.unwrap();
    let response = client
        .sync_regions(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .expect("runtime must supply canonical RegionSyncer")
        .into_inner();
    (tx, response)
}
async fn next(stream: &mut tonic::Streaming<pdpb::SyncRegionResponse>) -> pdpb::SyncRegionResponse {
    tokio::time::timeout(Duration::from_secs(5), stream.message())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_011_behavior() {
    sync_scenario().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go PD binary and read-only supervisor"]
async fn readonly_sync_regions_differential() {
    use astersql_pd_compat_tests::harness::Server;
    use astersql_pd_storage_kv::EtcdMetaStore;
    let rust = sync_scenario().await;
    let go = std::env::var("PD_COMPAT_GO_BIN").unwrap();
    let config = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_WORK_DIR").unwrap())
        .join("sync-go.toml");
    std::fs::write(&config, "[pd-server]\nuse-region-storage = false\n").unwrap();
    let mut server = Server::start_with_args(
        std::path::Path::new(&go),
        &[&format!("--config={}", config.display())],
    )
    .unwrap();
    server.ready().unwrap();
    let mut client = PdClient::connect(server.url.clone()).await.unwrap();
    let id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let boot = client
        .bootstrap(pdpb::BootstrapRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: id,
                ..Default::default()
            }),
            store: Some(metapb::Store {
                id: 1,
                address: "127.0.0.1:29999".into(),
                version: "8.5.0".into(),
                ..Default::default()
            }),
            region: report(id, 1).region,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(boot.header.unwrap().error.is_none());
    let backend = Arc::new(
        EtcdMetaStore::connect(std::slice::from_ref(&server.url), Duration::from_secs(3))
            .await
            .unwrap(),
    );
    let observed = exercise(&mut client, id, &StorageEndpoint::new(backend)).await;
    assert_eq!(observed, rust);
    let (status, health) = server.get("/pd/api/v1/health").unwrap();
    assert_eq!(status, 200);
    assert_eq!(health[0]["health"], true);
    let (status, region) = server.get("/pd/api/v1/region/id/7").unwrap();
    assert_eq!(status, 200);
    assert_eq!(region["epoch"]["version"], 5);
    println!("task011 dual observations: {observed}");
    let (sync_tx, mut stream) = subscribe(&mut client, id, u64::MAX).await;
    check_frame(&next(&mut stream).await, id, 5);
    next(&mut stream).await;
    server.stop().unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), stream.message())
            .await
            .unwrap()
            .unwrap_err()
            .code(),
        tonic::Code::Unavailable
    );
    drop((sync_tx, stream));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_011_role_retirement() {
    use astersql_pd_storage::{Consistency, MetaStore, Read};
    let fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let registry = Arc::new(prometheus::Registry::new());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = NodeConfig {
        cluster_id: 511,
        member_id: 11,
        name: "sync-role".into(),
        client_url: url.clone(),
        peer_url: "http://127.0.0.1:2380".into(),
    };
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(store.clone(), config, listener, registry.clone())
        .await
        .unwrap();
    node.ready().await.unwrap();
    let old = node.cluster();
    let syncer = old.region_sync_handle().unwrap();
    let mut client = PdClient::connect(url).await.unwrap();
    let (tx, mut stream) = subscribe(&mut client, 511, 0).await;
    assert!(next(&mut stream).await.regions.is_empty());
    let leader = store
        .read(Read::range(
            b"/pd/511/leader",
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    store
        .revoke(leader.entries[0].lease.unwrap())
        .await
        .unwrap();
    let error = tokio::time::timeout(Duration::from_secs(5), stream.message())
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::Unavailable);
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if node
                .try_cluster()
                .is_some_and(|current| !Arc::ptr_eq(&current, &old))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(syncer.downstream_names().is_empty());
    assert!(old.snapshot().await.is_err());
    drop((tx, stream, syncer));
    assert!(old.region_sync_handle().is_none());
    let (tx, mut stream) = subscribe(&mut client, 511, 0).await;
    assert!(next(&mut stream).await.regions.is_empty());
    node.shutdown().await.unwrap();
    assert_eq!(
        stream.message().await.unwrap_err().code(),
        tonic::Code::Unavailable
    );
    drop((tx, stream, client));
    assert!(registry.gather().is_empty());
    assert_eq!(Arc::strong_count(&store), 1);
}
