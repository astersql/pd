// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_025_unsynchronized_follower() {
    let fixture = crate::node_test::fixture::Fixture::start();
    let store: Arc<dyn astersql_pd_storage::MetaStore> = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let identity = crate::NodeConfig {
        cluster_id: 2502,
        member_id: 2,
        name: "not-synced".into(),
        client_url: url.clone(),
        peer_url: "http://peer2".into(),
    };
    // Real role service before a leader term or sync worker has been installed.
    let services = Arc::new(crate::member_role_worker::RoleServices::new(
        identity,
        store.clone(),
    ));
    let cancel = CancellationToken::new();
    let stop = CancellationToken::new();
    let shutdown = stop.clone();
    let router = astersql_pd_server::router(services, cancel.clone());
    let serving = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
            .unwrap()
    });
    let mut client = PdClient::connect(url).await.unwrap();
    let (tx, rx) = tokio::sync::mpsc::channel(2);
    let mut stream = client
        .query_region(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap()
        .into_inner();
    for _ in 0..2 {
        tx.send(pdpb::QueryRegionRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: 2502,
                ..Default::default()
            }),
            keys: vec![b"a".to_vec()],
            ids: vec![10],
            ..Default::default()
        })
        .await
        .unwrap();
        let r = stream.message().await.unwrap().unwrap();
        let h = r.header.unwrap();
        assert_eq!(h.cluster_id, 2502);
        let e = h.error.unwrap();
        assert_eq!(e.r#type, pdpb::ErrorType::RegionNotFound as i32);
        assert_eq!(e.message, "region not found");
        assert!(r.key_id_map.is_empty());
        assert!(r.regions_by_id.is_empty());
    }
    cancel.cancel();
    assert_eq!(
        stream.message().await.unwrap_err().code(),
        tonic::Code::Unavailable
    );
    drop(tx);
    drop(stream);
    drop(client);
    stop.cancel();
    serving.await.unwrap();
}
