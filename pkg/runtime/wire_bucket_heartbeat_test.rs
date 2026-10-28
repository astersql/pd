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

async fn bucket_scenario() -> serde_json::Value {
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
    let header = Some(pdpb::RequestHeader {
        cluster_id: 505,
        ..Default::default()
    });
    let peer = metapb::Peer {
        id: 2,
        store_id: 1,
        ..Default::default()
    };
    let report = pdpb::RegionHeartbeatRequest {
        header: header.clone(),
        region: Some(metapb::Region {
            id: 7,
            peers: vec![peer.clone()],
            region_epoch: Some(metapb::RegionEpoch {
                version: 2,
                conf_ver: 1,
            }),
            ..Default::default()
        }),
        leader: Some(peer),
        approximate_size: 2097152,
        ..Default::default()
    };
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    let mut output = client
        .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap()
        .into_inner();
    tx.send(report.clone()).await.unwrap();
    let storage = StorageEndpoint::new(store.clone());
    let paths = ClusterPaths::new(505);
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if storage.load_region(&paths, 7).await.unwrap().is_some() {
                break;
            }
            // The receive stream drives admission; successful reports have no ack.
            tokio::select! {
                message = output.message() => panic!("unexpected ack: {message:?}"),
                _ = tokio::time::sleep(Duration::from_millis(10)) => {}
            }
        }
    })
    .await
    .unwrap();
    let snapshot = host.snapshot().await.unwrap();
    assert_eq!(snapshot.regions.len(), 1);
    assert_eq!(snapshot.regions[0].meta().id, 7);
    assert_eq!(snapshot.regions[0].stats().approximate_size, 2);
    assert_eq!(
        storage.load_region(&paths, 7).await.unwrap(),
        Some(snapshot.regions[0].meta().clone())
    );
    drop(tx);
    assert!(output.message().await.unwrap().is_none());
    let mut observed = Vec::new();
    for (id, version, key) in [
        (999, 8, "missing"),
        (7, 3, "m"),
        (7, 2, "stale"),
        (7, 3, "equal"),
        (7, 4, "n"),
    ] {
        let error = client
            .report_buckets(tokio_stream::iter([bucket_request(
                header.clone(),
                id,
                version,
                key,
            )]))
            .await
            .unwrap_err();
        assert_eq!(
            error.code(),
            tonic::Code::Unknown,
            "runtime must supply BucketHeartbeatService: {error}"
        );
        assert_eq!(error.message(), "EOF");
        let snapshot = host.snapshot().await.unwrap();
        if id == 999 {
            assert!(snapshot.regions[0].report_buckets().is_none());
            assert!(snapshot.statistics.buckets.is_empty());
        } else {
            let buckets = snapshot.regions[0].report_buckets().unwrap();
            let expected = if version == 4 { (4, "n") } else { (3, "m") };
            assert_eq!(buckets.version, expected.0);
            assert_eq!(buckets.keys[1], expected.1.as_bytes());
            assert_eq!(buckets.period_in_ms, 2000);
            assert_eq!(
                buckets.stats.as_ref().unwrap().read_bytes,
                vec![200000, 400000]
            );
            assert!(
                !snapshot.statistics.buckets[&7].is_empty(),
                "same HotStat worker must consume reports"
            );
            observed.push(serde_json::json!(
                String::from_utf8(buckets.keys[1].clone()).unwrap()
            ));
        }
        assert_eq!(
            storage.load_region(&paths, 7).await.unwrap(),
            Some(snapshot.regions[0].meta().clone()),
            "buckets remain volatile; region metadata is unchanged"
        );
    }
    let mut wrong = bucket_request(header.clone(), 7, 99, "wrong");
    wrong.header.as_mut().unwrap().cluster_id += 1;
    assert_eq!(
        client
            .report_buckets(tokio_stream::iter([wrong]))
            .await
            .unwrap_err()
            .code(),
        tonic::Code::FailedPrecondition
    );
    assert_eq!(
        host.snapshot().await.unwrap().regions[0]
            .report_buckets()
            .unwrap()
            .version,
        4
    );
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    let mut stream_client = client.clone();
    let call = tokio::spawn(async move {
        stream_client
            .report_buckets(tokio_stream::wrappers::ReceiverStream::new(rx))
            .await
    });
    tx.send(bucket_request(header, 7, 5, "open")).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if host.snapshot().await.unwrap().regions[0]
                .report_buckets()
                .unwrap()
                .version
                == 5
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(!call.is_finished(), "no success ack before EOF");
    parent.cancel();
    tokio::time::timeout(Duration::from_secs(5), node.wait())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        call.await.unwrap().unwrap_err().code(),
        tonic::Code::Cancelled
    );
    assert!(host.snapshot().await.is_err());
    drop((tx, output, client, storage));
    assert_eq!(Arc::strong_count(&store), 1);
    assert!(registry.gather().is_empty());
    assert!(tokio::net::TcpStream::connect(address).await.is_err());
    serde_json::json!(observed)
}
fn bucket_request(
    header: Option<pdpb::RequestHeader>,
    id: u64,
    version: u64,
    key: &str,
) -> pdpb::ReportBucketsRequest {
    pdpb::ReportBucketsRequest {
        header,
        buckets: Some(metapb::Buckets {
            region_id: id,
            version,
            keys: vec![vec![], key.as_bytes().to_vec(), vec![]],
            period_in_ms: 2000,
            stats: Some(metapb::BucketStats {
                read_bytes: vec![200000, 400000],
                read_keys: vec![4, 8],
                read_qps: vec![6, 12],
                write_bytes: vec![800000, 1600000],
                write_keys: vec![10, 20],
                write_qps: vec![12, 24],
            }),
        }),
        ..Default::default()
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_010_behavior() {
    bucket_scenario().await;
}

/// The supervisor owns process groups, input hashes and disposable directories.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go PD binary and read-only supervisor"]
async fn readonly_bucket_heartbeat_differential() {
    use astersql_pd_compat_tests::harness::Server;
    use astersql_pd_storage::{Consistency, MetaStore, Read};
    use astersql_pd_storage_kv::EtcdMetaStore;
    let rust = bucket_scenario().await;
    let go = std::env::var("PD_COMPAT_GO_BIN").unwrap();
    let config = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_WORK_DIR").unwrap())
        .join("bucket-go.toml");
    std::fs::write(&config, "[pd-server]\nuse-region-storage = false\n").unwrap();
    let mut server = Server::start_with_args(
        std::path::Path::new(&go),
        &[&format!("--config={}", config.display())],
    )
    .unwrap();
    server.ready().unwrap();
    let mut client = PdClient::connect(server.url.clone()).await.unwrap();
    let cluster_id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let header = Some(pdpb::RequestHeader {
        cluster_id,
        ..Default::default()
    });
    let result = client
        .bootstrap(pdpb::BootstrapRequest {
            header: header.clone(),
            store: Some(metapb::Store {
                id: 1,
                address: "127.0.0.1:29999".into(),
                version: "8.5.0".into(),
                ..Default::default()
            }),
            region: Some(metapb::Region {
                id: 7,
                peers: vec![metapb::Peer {
                    id: 2,
                    store_id: 1,
                    ..Default::default()
                }],
                region_epoch: Some(metapb::RegionEpoch {
                    version: 2,
                    conf_ver: 1,
                }),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(result.header.unwrap().error.is_none());
    let backend = EtcdMetaStore::connect(std::slice::from_ref(&server.url), Duration::from_secs(3))
        .await
        .unwrap();
    let key = format!("/pd/{cluster_id}/raft/r/00000000000000000007");
    let before = backend
        .read(Read::prefix(key.as_bytes(), Consistency::Linearizable))
        .await
        .unwrap()
        .entries;
    assert_eq!(before.len(), 1);
    let mut observed = Vec::new();
    for (id, version, key) in [
        (999, 8, "missing"),
        (7, 3, "m"),
        (7, 2, "stale"),
        (7, 3, "equal"),
        (7, 4, "n"),
    ] {
        let error = client
            .report_buckets(tokio_stream::iter([bucket_request(
                header.clone(),
                id,
                version,
                key,
            )]))
            .await
            .unwrap_err();
        assert_eq!(error.code(), tonic::Code::Unknown);
        assert_eq!(error.message(), "EOF");
        let (status, region) = server.get("/pd/api/v1/region/id/7").unwrap();
        assert_eq!(status, 200);
        if id == 999 {
            assert!(region.get("buckets").is_none());
        } else {
            let hex = region["buckets"][1].as_str().unwrap();
            let decoded: Vec<u8> = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect();
            observed.push(serde_json::json!(String::from_utf8(decoded).unwrap()));
        }
        let after = backend
            .read(Read::prefix(
                key_path(cluster_id).as_bytes(),
                Consistency::Linearizable,
            ))
            .await
            .unwrap()
            .entries;
        assert_eq!(
            after, before,
            "bucket reports do not persist into region metadata"
        );
    }
    let mut wrong = bucket_request(header, 7, 99, "wrong");
    wrong.header.as_mut().unwrap().cluster_id += 1;
    assert_eq!(
        client
            .report_buckets(tokio_stream::iter([wrong]))
            .await
            .unwrap_err()
            .code(),
        tonic::Code::FailedPrecondition
    );
    let (status, health) = server.get("/pd/api/v1/health").unwrap();
    assert_eq!(status, 200);
    assert_eq!(health[0]["health"], true);
    let go = serde_json::json!(observed);
    println!(
        "bucket observations Go={go}; Rust={rust}; EOF=Unknown/EOF; wrong_cluster=FailedPrecondition; persistent_region_unchanged=true"
    );
    assert_eq!(go, rust);
    drop((client, backend));
    server.stop().unwrap();
}
fn key_path(cluster_id: u64) -> String {
    format!("/pd/{cluster_id}/raft/r/00000000000000000007")
}
