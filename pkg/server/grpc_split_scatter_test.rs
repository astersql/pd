// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_031_behavior() {
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
        .split_and_scatter_regions(pdpb::SplitAndScatterRegionsRequest {
            header: h.clone(),
            ..Default::default()
        })
        .await;
    assert!(
        result.is_ok(),
        "SplitAndScatterRegions must serve real requests: {result:?}"
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
    super::grpc_region_lookup_scenarios_test::prepare(&mut client, 2900).await;
    println!(
        "{}",
        super::grpc_split_scatter_scenarios_test::observe(&mut client, 2900).await
    );
    let endpoint = astersql_pd_storage_endpoint::StorageEndpoint::new(store);
    let paths = astersql_pd_keypath::ClusterPaths::new(2900);
    for id in [101, 102, 103] {
        let r = endpoint.load_region(&paths, id).await.unwrap().unwrap();
        assert_eq!(r.id, id);
        assert_eq!(r.peers.len(), if id == 103 { 1 } else { 3 });
    }
    assert!(
        std::process::Command::new("curl")
            .args(["-fsS", &format!("{url}/pd/api/v1/ping")])
            .output()
            .unwrap()
            .status
            .success()
    );
    let mut pending_client = client.clone();
    let pending = tokio::spawn(async move {
        pending_client
            .split_and_scatter_regions(pdpb::SplitAndScatterRegionsRequest {
                header: super::grpc_region_lookup_scenarios_test::header(2900),
                split_keys: vec![b"x".to_vec()],
                retry_limit: 2,
                ..Default::default()
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(!pending.is_finished());
    cancel.cancel();
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(5), pending)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    node.shutdown().await.unwrap();
}
