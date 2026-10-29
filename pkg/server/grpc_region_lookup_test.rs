// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::pd_client::PdClient;
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_023_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let mut nodes = vec![];
    let mut clients = vec![];
    for member_id in 1..=2 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mut node =
            PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
                .start(
                    store.clone(),
                    NodeConfig {
                        cluster_id: 2300,
                        member_id,
                        name: format!("lookup{member_id}"),
                        client_url: url.clone(),
                        peer_url: format!("http://peer{member_id}"),
                    },
                    listener,
                    Arc::new(prometheus::Registry::new()),
                )
                .await
                .unwrap();
        node.ready().await.unwrap();
        clients.push(PdClient::connect(url.clone()).await.unwrap());
        nodes.push(node);
        assert!(
            std::process::Command::new("curl")
                .args(["-fsS", &format!("{url}/pd/api/v1/ping")])
                .output()
                .unwrap()
                .status
                .success()
        );
    }
    let mut cold = tonic::Request::new(astersql_pd_proto::pdpb::GetRegionRequest {
        header: super::grpc_region_lookup_scenarios_test::header(2300),
        ..Default::default()
    });
    cold.metadata_mut()
        .insert("pd-allow-follower-handle", "".parse().unwrap());
    assert_eq!(
        clients[1]
            .get_region(cold)
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .error
            .unwrap()
            .r#type,
        astersql_pd_proto::pdpb::ErrorType::RegionNotFound as i32
    );
    super::grpc_region_lookup_scenarios_test::prepare(&mut clients[0], 2300).await;
    for (i, client) in clients.iter_mut().enumerate() {
        let result = super::grpc_region_lookup_scenarios_test::observe(client, 2300, i == 1).await;
        assert_eq!(result.len(), 44);
    }
    // Query reads must not mutate persisted region metadata.
    use astersql_pd_storage::{Consistency, MetaStore, Read};
    let before = store
        .read(Read::prefix(b"/pd/2300/raft/r/", Consistency::Linearizable))
        .await
        .unwrap();
    super::grpc_region_lookup_scenarios_test::observe(&mut clients[0], 2300, false).await;
    let after = store
        .read(Read::prefix(b"/pd/2300/raft/r/", Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(before.entries, after.entries);
    for node in nodes.iter_mut().rev() {
        node.shutdown().await.unwrap();
    }
}
