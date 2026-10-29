// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_029_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let cancel = CancellationToken::new();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 2900,
                member_id: 1,
                name: "scatter".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let mut client = PdClient::connect(url.clone()).await.unwrap();
    let h = super::grpc_region_lookup_scenarios_test::header(2900);
    let result = client
        .scatter_region(pdpb::ScatterRegionRequest {
            header: h.clone(),
            ..Default::default()
        })
        .await;
    assert!(
        result.is_ok(),
        "ScatterRegion must serve real requests: {result:?}"
    );
    assert_eq!(
        result
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .error
            .unwrap()
            .r#type,
        pdpb::ErrorType::NotBootstrapped as i32
    );
    let result = client
        .get_operator(pdpb::GetOperatorRequest {
            header: h.clone(),
            region_id: 10,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        result.header.unwrap().error.unwrap().r#type,
        pdpb::ErrorType::NotBootstrapped as i32
    );
    super::grpc_region_lookup_scenarios_test::prepare(&mut client, 2900).await;
    let missing = client
        .get_operator(pdpb::GetOperatorRequest {
            header: h.clone(),
            region_id: 999,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(missing.header.unwrap().error.unwrap().message, "Not Found");
    let result = client
        .scatter_region(pdpb::ScatterRegionRequest {
            header: h.clone(),
            regions_id: vec![10, 999],
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(result.finished_percentage, 0);
    assert_eq!(result.failed_regions_id, vec![10, 999]);
    let result = client
        .scatter_region(pdpb::ScatterRegionRequest {
            header: h.clone(),
            region_id: 999,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        result.header.unwrap().error.unwrap().message,
        "region %d not found"
    );
    super::grpc_scatter_scenarios_test::errors(&mut client, 2900).await;
    let observations = super::grpc_scatter_scenarios_test::observe(&mut client, 2900).await;
    println!("{observations}");
    let endpoint = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
    let paths = astersql_pd_keypath::ClusterPaths::new(2900);
    for id in 100..=102 {
        let meta = endpoint.load_region(&paths, id).await.unwrap().unwrap();
        assert_eq!(meta.id, id);
        assert_eq!(
            meta.peers.iter().map(|p| p.store_id).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }
    assert!(endpoint.load_region(&paths, 999).await.unwrap().is_none());
    assert!(
        std::process::Command::new("curl")
            .args(["-fsS", &format!("{url}/pd/api/v1/ping")])
            .output()
            .unwrap()
            .status
            .success()
    );
    let mut retry_client = client.clone();
    let pending = tokio::spawn(async move {
        retry_client
            .scatter_region(pdpb::ScatterRegionRequest {
                header: super::grpc_region_lookup_scenarios_test::header(2900),
                regions_id: vec![11],
                retry_limit: 30,
                ..Default::default()
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(
        !pending.is_finished(),
        "scatter retry must still be running before cancellation"
    );
    cancel.cancel();
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(5), pending)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    assert!(
        client
            .get_operator(pdpb::GetOperatorRequest {
                header: h,
                region_id: 10
            })
            .await
            .is_err()
    );
    node.shutdown().await.unwrap();
}
