// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
fn header() -> Option<pdpb::RequestHeader> {
    Some(pdpb::RequestHeader {
        cluster_id: 3500,
        ..Default::default()
    })
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_035_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let cancel = CancellationToken::new();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 3500,
                member_id: 1,
                name: "external".into(),
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
    let result = client
        .get_external_timestamp(pdpb::GetExternalTimestampRequest { header: header() })
        .await;
    assert!(
        result.is_ok(),
        "external timestamp RPC must be implemented: {result:?}"
    );
    assert_eq!(result.unwrap().into_inner().timestamp, 0);
    super::grpc_region_lookup_scenarios_test::prepare(&mut client, 3500).await;
    for value in [1, 100, 101] {
        let response = client
            .set_external_timestamp(pdpb::SetExternalTimestampRequest {
                header: header(),
                timestamp: value,
            })
            .await
            .unwrap()
            .into_inner();
        assert!(response.header.unwrap().error.is_none());
        assert_eq!(
            client
                .get_external_timestamp(pdpb::GetExternalTimestampRequest { header: header() })
                .await
                .unwrap()
                .into_inner()
                .timestamp,
            value
        );
    }
    for (value, message) in [
        (
            0,
            "the external timestamp should be larger than current external timestamp",
        ),
        (
            101,
            "the external timestamp should be larger than current external timestamp",
        ),
        (
            u64::MAX,
            "the external timestamp should not be larger than global ts",
        ),
    ] {
        let error = client
            .set_external_timestamp(pdpb::SetExternalTimestampRequest {
                header: header(),
                timestamp: value,
            })
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .error
            .unwrap();
        assert_eq!(error.r#type, pdpb::ErrorType::InvalidValue as i32);
        assert_eq!(error.message, message);
    }
    let storage = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
    assert_eq!(
        storage
            .load_external_ts(&astersql_pd_keypath::ClusterPaths::new(3500))
            .await
            .unwrap(),
        101
    );
    cancel.cancel();
    assert!(
        client
            .get_external_timestamp(pdpb::GetExternalTimestampRequest { header: header() })
            .await
            .is_err()
    );
    node.shutdown().await.unwrap();
    let listener = tokio::net::TcpListener::bind(url.trim_start_matches("http://"))
        .await
        .unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            NodeConfig {
                cluster_id: 3500,
                member_id: 1,
                name: "external".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let mut client = PdClient::connect(url).await.unwrap();
    assert_eq!(
        client
            .get_external_timestamp(pdpb::GetExternalTimestampRequest { header: header() })
            .await
            .unwrap()
            .into_inner()
            .timestamp,
        101
    );
    node.shutdown().await.unwrap();
}
