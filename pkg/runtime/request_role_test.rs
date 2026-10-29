// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

type Client = PdClient<tonic::transport::Channel>;
fn status(error: tonic::Status, cluster_id: u64) -> Value {
    json!({"code": format!("{:?}", error.code()), "message":error.message().replace(&cluster_id.to_string(), "CLUSTER")})
}
fn header(header: pdpb::ResponseHeader, cluster_id: u64) -> Value {
    assert_eq!(header.cluster_id, cluster_id);
    json!({"type":header.error.as_ref().map(|e| e.r#type),"message":header.error.map(|e| e.message)})
}
async fn observe(client: &mut Client, cluster_id: u64, follower: bool) -> Value {
    let valid = Some(pdpb::RequestHeader {
        cluster_id,
        ..Default::default()
    });
    let mut result = Vec::new();
    for input in [
        None,
        Some(pdpb::RequestHeader::default()),
        Some(pdpb::RequestHeader {
            cluster_id: 1,
            ..Default::default()
        }),
    ] {
        result.push(status(
            client
                .alloc_id(pdpb::AllocIdRequest {
                    header: input.clone(),
                    ..Default::default()
                })
                .await
                .unwrap_err(),
            cluster_id,
        ));
        result.push(status(
            client
                .store_heartbeat(pdpb::StoreHeartbeatRequest {
                    header: input,
                    ..Default::default()
                })
                .await
                .unwrap_err(),
            cluster_id,
        ));
    }
    let store = client
        .store_heartbeat(pdpb::StoreHeartbeatRequest {
            header: valid.clone(),
            stats: Some(pdpb::StoreStats {
                store_id: 1,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await;
    result.push(match store {
        Ok(response) => header(response.into_inner().header.unwrap(), cluster_id),
        Err(error) => status(error, cluster_id),
    });
    let store = client
        .get_store(pdpb::GetStoreRequest {
            header: valid.clone(),
            store_id: 1,
        })
        .await;
    result.push(match store {
        Ok(response) => header(response.into_inner().header.unwrap(), cluster_id),
        Err(error) => status(error, cluster_id),
    });
    // A stream's missing header is deliberately masked by absent raft-cluster state.
    let mut stream = client
        .region_heartbeat(tokio_stream::iter(
            [pdpb::RegionHeartbeatRequest::default()],
        ))
        .await
        .unwrap()
        .into_inner();
    result.push(header(
        stream.message().await.unwrap().unwrap().header.unwrap(),
        cluster_id,
    ));
    assert!(stream.message().await.unwrap().is_none());
    let buckets = client
        .report_buckets(tokio_stream::iter([pdpb::ReportBucketsRequest::default()]))
        .await
        .unwrap()
        .into_inner();
    result.push(header(buckets.header.unwrap(), cluster_id));
    if follower {
        result.push(status(
            client
                .alloc_id(pdpb::AllocIdRequest {
                    header: valid,
                    ..Default::default()
                })
                .await
                .unwrap_err(),
            cluster_id,
        ));
    } else {
        assert!(
            client
                .alloc_id(pdpb::AllocIdRequest {
                    header: valid,
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner()
                .id
                > 0
        );
    }
    json!(result)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn readonly_request_role_differential() {
    use crate::{NodeConfig, PdNodeBuilder, node_test::fixture};
    use astersql_pd_compat_tests::harness::Server;
    let fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let mut nodes = Vec::new();
    let mut rust = Vec::new();
    for id in 1..=2 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mut node =
            PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
                .start(
                    store.clone(),
                    NodeConfig {
                        cluster_id: 1802,
                        member_id: id,
                        name: format!("request-{id}"),
                        client_url: url.clone(),
                        peer_url: "http://peer:2380".into(),
                    },
                    listener,
                    Arc::new(prometheus::Registry::new()),
                )
                .await
                .unwrap();
        node.ready().await.unwrap();
        let mut client = PdClient::connect(url).await.unwrap();
        rust.push(observe(&mut client, 1802, id == 2).await);
        nodes.push(node);
    }
    let binary = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    let mut leader = Server::start(&binary).unwrap();
    leader.ready().unwrap();
    let join = format!("--join={}", leader.url);
    let mut follower = Server::start_with_args(
        &binary,
        &["--name=request-follower", "--initial-cluster=", &join],
    )
    .unwrap();
    follower.ready().unwrap();
    let mut client = PdClient::connect(leader.url.clone()).await.unwrap();
    let cluster_id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let mut go = vec![observe(&mut client, cluster_id, false).await];
    let mut client = PdClient::connect(follower.url.clone()).await.unwrap();
    go.push(observe(&mut client, cluster_id, true).await);
    println!("Go observations: {go:?}\nRust observations: {rust:?}");
    assert_eq!(rust, go);
    follower
        .stop_with_timeout(std::time::Duration::from_secs(30))
        .unwrap();
    leader
        .stop_with_timeout(std::time::Duration::from_secs(30))
        .unwrap();
    for node in &mut nodes {
        node.shutdown().await.unwrap();
    }
}
