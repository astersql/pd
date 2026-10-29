// Copyright 2026 AsterSQL.
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

async fn observe(client: &mut PdClient<tonic::transport::Channel>, id: u64) -> Value {
    let header = Some(pdpb::RequestHeader {
        cluster_id: id,
        ..Default::default()
    });
    assert!(
        !client
            .is_bootstrapped(pdpb::IsBootstrappedRequest {
                header: header.clone()
            })
            .await
            .unwrap()
            .into_inner()
            .bootstrapped
    );
    let valid = pdpb::BootstrapRequest {
        header: header.clone(),
        store: Some(metapb::Store {
            id: 1,
            address: "127.0.0.1:20160".into(),
            ..Default::default()
        }),
        region: Some(metapb::Region {
            id: 2,
            peers: vec![metapb::Peer {
                id: 3,
                store_id: 1,
                ..Default::default()
            }],
            ..Default::default()
        }),
    };
    let mut errors = Vec::new();
    for case in 0..8 {
        let mut r = valid.clone();
        match case {
            0 => r.store = None,
            1 => r.store.as_mut().unwrap().id = 0,
            2 => r.region = None,
            3 => r.region.as_mut().unwrap().start_key = vec![1],
            4 => r.region.as_mut().unwrap().id = 0,
            5 => r.region.as_mut().unwrap().peers.clear(),
            6 => r.region.as_mut().unwrap().peers[0].store_id = 9,
            _ => r.region.as_mut().unwrap().peers[0].id = 0,
        }
        let response = client.bootstrap(r).await.unwrap().into_inner();
        let error = response.header.unwrap().error.unwrap();
        assert_eq!(error.r#type, pdpb::ErrorType::Unknown as i32);
        assert!(response.replication_status.is_none());
        errors.push(json!({"type":error.r#type,"message":error.message.replace(&id.to_string(),"<cluster>")}));
        assert!(
            !client
                .is_bootstrapped(pdpb::IsBootstrappedRequest {
                    header: header.clone()
                })
                .await
                .unwrap()
                .into_inner()
                .bootstrapped
        );
    }
    let response = client.bootstrap(valid.clone()).await.unwrap().into_inner();
    assert_eq!(
        response.header,
        Some(pdpb::ResponseHeader {
            cluster_id: id,
            error: None
        })
    );
    let status = response.replication_status.unwrap();
    assert_eq!(
        status.mode,
        astersql_pd_proto::replication_modepb::ReplicationMode::Majority as i32
    );
    assert!(status.dr_auto_sync.is_none());
    let error = client
        .bootstrap(pdpb::BootstrapRequest {
            header: header.clone(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .error
        .unwrap();
    assert_eq!(error.r#type, pdpb::ErrorType::AlreadyBootstrapped as i32);
    assert_eq!(error.message, "cluster is already bootstrapped");
    assert!(
        client
            .is_bootstrapped(pdpb::IsBootstrappedRequest {
                header: header.clone()
            })
            .await
            .unwrap()
            .into_inner()
            .bootstrapped
    );
    let store = client
        .get_store(pdpb::GetStoreRequest {
            header: header.clone(),
            store_id: 1,
        })
        .await
        .unwrap()
        .into_inner()
        .store
        .unwrap();
    assert_eq!(store, valid.store.unwrap());
    let region = client
        .get_region_by_id(pdpb::GetRegionByIdRequest {
            header: header.clone(),
            region_id: 2,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .region
        .unwrap();
    assert_eq!(region, valid.region.unwrap());
    let bad = client
        .is_bootstrapped(pdpb::IsBootstrappedRequest { header: None })
        .await
        .unwrap_err();
    assert_eq!(bad.code(), tonic::Code::FailedPrecondition);
    json!({"errors":errors,"replication_mode":status.mode,"duplicate":error.message,"store":store.id,"region":region.id,"bootstrapped":true})
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn readonly_bootstrap_differential() {
    use astersql_pd_compat_tests::harness::Server;
    let fixture = crate::node_test::fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node =
        crate::PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store,
                crate::NodeConfig {
                    cluster_id: 2000,
                    member_id: 1,
                    name: "bootstrap".into(),
                    client_url: url.clone(),
                    peer_url: "http://peer".into(),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    node.ready().await.unwrap();
    let rust = observe(&mut PdClient::connect(url).await.unwrap(), 2000).await;
    let binary = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    let mut server = Server::start(&binary).unwrap();
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
    let go = observe(&mut client, id).await;
    println!("Go bootstrap: {go}\nRust bootstrap: {rust}");
    assert_eq!(go, rust);
    server.stop().unwrap();
    node.shutdown().await.unwrap();
}
