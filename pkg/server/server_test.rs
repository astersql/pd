// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
#[allow(dead_code)]
#[path = "../storage/kv/etcd_fixture_test.rs"]
mod fixture;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn validation_and_open_stream_shutdown() {
    let fixture = fixture::Fixture::start();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let url = format!("http://{address}");
    let registry = Arc::new(prometheus::Registry::new());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            Arc::new(fixture.store().await),
            NodeConfig {
                cluster_id: 42,
                member_id: 8,
                name: "node".into(),
                client_url: url.clone(),
                peer_url: "http://127.0.0.1:2380".into(),
            },
            listener,
            registry.clone(),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let mut client = PdClient::connect(url.clone()).await.unwrap();
    let header = Some(pdpb::RequestHeader {
        cluster_id: 42,
        ..Default::default()
    });
    let error = client
        .alloc_id(pdpb::AllocIdRequest::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::FailedPrecondition);
    assert_eq!(
        client
            .alloc_id(pdpb::AllocIdRequest {
                header: header.clone(),
                count: 2
            })
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Unimplemented
    );
    for (count, dc, code) in [
        (0, "", tonic::Code::Unknown),
        (1, "dc-1", tonic::Code::Unimplemented),
    ] {
        let mut stream = client
            .tso(tokio_stream::iter([pdpb::TsoRequest {
                header: header.clone(),
                count,
                dc_location: dc.into(),
                ..Default::default()
            }]))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(stream.message().await.unwrap_err().code(), code);
    }
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    let mut stream = client
        .tso(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap()
        .into_inner();
    tx.send(pdpb::TsoRequest {
        header,
        count: 1,
        ..Default::default()
    })
    .await
    .unwrap();
    assert!(stream.message().await.unwrap().is_some());
    assert_eq!(
        registry
            .gather()
            .iter()
            .find(|m| m.name() == "pd_rust_requests_total")
            .unwrap()
            .get_metric()[0]
            .get_counter()
            .get_value(),
        1.0
    );
    tokio::time::timeout(Duration::from_secs(5), node.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert!(stream.message().await.unwrap().is_none());
    assert!(registry.gather().is_empty());
    assert!(tokio::net::TcpStream::connect(address).await.is_err());
}
