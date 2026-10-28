// Copyright 2026 AsterSQL.
use super::*;
use crate::{NodeConfig, PdNodeBuilder, node_test::fixture};
use astersql_pd_core::{StoreInfo, StoreMeta};
use std::time::Duration;

async fn rust_scenario() -> u64 {
    let fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let storage = StorageEndpoint::new(store.clone());
    let paths = ClusterPaths::new(516);
    storage.save_min_resolved_ts(&paths, 20).await.unwrap();
    let mut config = astersql_pd_server_config::PersistedConfig::default();
    config
        .config
        .adjust(
            &Default::default(),
            astersql_pd_server_config::ConfigDefaultsContext {
                hostname: "",
                cwd: &std::env::current_dir().unwrap(),
            },
            false,
            &mut Default::default(),
        )
        .unwrap();
    config.config.replication.max_replicas = 1;
    config.persist(&storage, &paths).await.unwrap();
    let cluster = BasicCluster::new();
    let mut info = StoreInfo::new(StoreMeta {
        id: 1,
        node_state: NodeState::Serving,
        ..Default::default()
    });
    info.set_min_resolved_ts(42);
    cluster.stores.put_store(Arc::new(info));
    let peer = astersql_pd_core::Peer {
        id: 1,
        store_id: 1,
        ..Default::default()
    };
    let region = Arc::new(RegionInfo::new(
        astersql_pd_core::RegionMeta {
            id: 1,
            peers: vec![peer.clone()],
            ..Default::default()
        },
        Some(peer),
        vec![],
        vec![],
    ));
    let mut cluster = cluster;
    let (old, overlaps, changed) = cluster.regions.set_region(region.clone());
    cluster
        .regions
        .update_sub_tree(region, old.as_deref(), &overlaps, changed);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .with_cluster(cluster)
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 516,
                member_id: 16,
                name: "resolved".into(),
                client_url: format!("http://{address}"),
                peer_url: "http://127.0.0.1:2380".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let host = node.cluster();
    assert_eq!(host.min_resolved_ts(), 20);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if storage.load_min_resolved_ts(&paths).await.unwrap() == 42 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "worker did not persist minimum: {:?}",
            host.resolved_ts_error()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(host.min_resolved_ts(), 42);
    assert!(host.resolved_ts_error().is_none());
    node.shutdown().await.unwrap();
    assert_eq!(storage.load_min_resolved_ts(&paths).await.unwrap(), 42);
    42
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_016_runtime() {
    rust_scenario().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go PD binary and read-only supervisor"]
async fn readonly_resolved_ts_differential() {
    use astersql_pd_compat_tests::harness::Server;
    use astersql_pd_proto::{
        metapb,
        pdpb::{self, pd_client::PdClient},
    };
    let rust = rust_scenario().await;
    let mut server = Server::start(std::path::Path::new(
        &std::env::var("PD_COMPAT_GO_BIN").unwrap(),
    ))
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
    let header = Some(pdpb::RequestHeader {
        cluster_id: id,
        ..Default::default()
    });
    let peer = metapb::Peer {
        id: 1,
        store_id: 1,
        ..Default::default()
    };
    let region = metapb::Region {
        id: 1,
        peers: vec![peer.clone()],
        region_epoch: Some(metapb::RegionEpoch {
            version: 1,
            conf_ver: 1,
        }),
        ..Default::default()
    };
    let response = client
        .bootstrap(pdpb::BootstrapRequest {
            header: header.clone(),
            store: Some(metapb::Store {
                id: 1,
                address: "127.0.0.1:29999".into(),
                version: "8.5.0".into(),
                ..Default::default()
            }),
            region: Some(region.clone()),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(response.header.unwrap().error.is_none());
    let output = std::process::Command::new("curl")
        .args([
            "-fsS",
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "-d",
            r#"{"replication.max-replicas":1}"#,
            &format!("{}/pd/api/v1/config", server.url),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "configure Go replicas: {:?}",
        output
    );

    let (tx, rx) = tokio::sync::mpsc::channel(4);
    tx.send(pdpb::RegionHeartbeatRequest {
        header: header.clone(),
        region: Some(region),
        leader: Some(peer),
        approximate_size: 1048576,
        ..Default::default()
    })
    .await
    .unwrap();
    let _output = client
        .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let response = client
        .report_min_resolved_ts(pdpb::ReportMinResolvedTsRequest {
            header,
            store_id: 1,
            min_resolved_ts: 42,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(response.header.unwrap().error.is_none());
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let output = std::process::Command::new("curl")
            .args(["-fsS", &format!("{}/pd/api/v1/min-resolved-ts", server.url)])
            .output()
            .unwrap();
        assert!(output.status.success());
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        if value["min_resolved_ts"].as_u64() == Some(rust) {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "Go worker did not aggregate: {value}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    drop(tx);
    server
        .restart_with_args(
            std::path::Path::new(&std::env::var("PD_COMPAT_GO_BIN").unwrap()),
            &[],
        )
        .unwrap();
    server.ready().unwrap();
    let (status, value) = server.get("/pd/api/v1/min-resolved-ts").unwrap();
    assert_eq!(status, 200);
    assert_eq!(
        value["min_resolved_ts"].as_u64(),
        Some(rust),
        "Go persisted minimum must survive restart"
    );
    server.stop().unwrap();
}
