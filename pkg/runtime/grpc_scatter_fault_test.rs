// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_storage::{Consistency, MetaStore, Read};
use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use tokio_util::sync::CancellationToken;
#[path = "../server/grpc_region_lookup_scenarios_test.rs"]
mod prepare;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_029_storage_error_and_cancel() {
    let fixture = crate::node_test::fixture::Fixture::start();
    let backend = Arc::new(crate::grpc_ask_split_fault_test::ReservationStore {
        inner: Arc::new(fixture.store().await),
        _fixture: std::sync::Mutex::new(fixture),
        mode: std::sync::atomic::AtomicU8::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let cancel = CancellationToken::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = crate::PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .start(
            backend.clone(),
            crate::NodeConfig {
                cluster_id: 2902,
                member_id: 1,
                name: "scatter-fault".into(),
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
    prepare::prepare(&mut client, 2902).await;
    let allocated = client
        .alloc_id(pdpb::AllocIdRequest {
            header: prepare::header(2902),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .id;
    let remaining = (1000 - allocated % 1000) % 1000;
    if remaining > 0 {
        client
            .alloc_id(pdpb::AllocIdRequest {
                header: prepare::header(2902),
                count: remaining as u32,
            })
            .await
            .unwrap();
    }
    let key = b"/pd/2902/alloc_id";
    let before = backend
        .read(Read::prefix(key, Consistency::Linearizable))
        .await
        .unwrap()
        .entries;
    let request = pdpb::ScatterRegionRequest {
        header: prepare::header(2902),
        regions_id: vec![10],
        ..Default::default()
    };
    backend.mode.store(1, Ordering::SeqCst);
    let error = tokio::time::timeout(
        Duration::from_secs(20),
        client.scatter_region(request.clone()),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert_eq!(error.code(), tonic::Code::Unknown);
    assert_eq!(error.message(), "unexpected metadata backend response");
    assert_eq!(
        backend
            .read(Read::prefix(key, Consistency::Linearizable))
            .await
            .unwrap()
            .entries,
        before
    );
    let missing = client
        .get_operator(pdpb::GetOperatorRequest {
            header: prepare::header(2902),
            region_id: 10,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(missing.header.unwrap().error.unwrap().message, "Not Found");
    backend.mode.store(2, Ordering::SeqCst);
    let pending = tokio::spawn(async move { client.scatter_region(request).await });
    tokio::time::timeout(Duration::from_secs(20), backend.entered.notified())
        .await
        .unwrap();
    cancel.cancel();
    assert!(
        tokio::time::timeout(Duration::from_secs(10), pending)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    node.shutdown().await.unwrap();
    assert_eq!(
        backend
            .read(Read::prefix(key, Consistency::Linearizable))
            .await
            .unwrap()
            .entries,
        before
    );
    assert!(
        backend
            .read(Read::prefix(
                b"/task026/oversized",
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
}
