// Copyright 2026 AsterSQL.
use super::*;
use crate::{NodeConfig, PdNodeBuilder, node_test::fixture};
use std::time::Duration;

async fn rust_scenario() -> f64 {
    let fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let registry = Arc::new(prometheus::Registry::new());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            NodeConfig {
                cluster_id: 515,
                member_id: 15,
                name: "compat".into(),
                client_url: format!("http://{address}"),
                peer_url: "http://127.0.0.1:2380".into(),
            },
            listener,
            registry.clone(),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    let result = loop {
        if let Some(metric) = registry
            .gather()
            .into_iter()
            .find(|m| m.name() == "pd_cluster_health_status")
        {
            break metric.get_metric()[0].get_gauge().get_value();
        }
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(result, 1.);
    node.shutdown().await.unwrap();
    assert!(registry.gather().is_empty());
    result
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_015_runtime() {
    rust_scenario().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go PD binary and read-only supervisor"]
async fn readonly_cluster_metrics_differential() {
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
    let response = client
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
            region: Some(metapb::Region {
                id: 1,
                peers: vec![metapb::Peer {
                    id: 1,
                    store_id: 1,
                    ..Default::default()
                }],
                region_epoch: Some(metapb::RegionEpoch {
                    version: 1,
                    conf_ver: 1,
                }),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(response.header.unwrap().error.is_none());
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let output = std::process::Command::new("curl")
            .args(["-fsS", &format!("{}/metrics", server.url)])
            .output()
            .unwrap();
        assert!(output.status.success());
        let body = String::from_utf8(output.stdout).unwrap();
        if let Some(value) = body
            .lines()
            .find_map(|line| line.strip_prefix("pd_cluster_health_status{name=\"compat\"} "))
        {
            assert_eq!(value.parse::<f64>().unwrap(), rust);
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "missing Go health metric"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    server.stop().unwrap();
}
