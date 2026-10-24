// Copyright 2026 AsterSQL.
//! Read-only-source Go server oracle probe; runs against a disposable host.
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
#[tokio::main]
async fn main() {
    let url = std::env::var("PD_ORACLE_URL").unwrap();
    let mut client = PdClient::connect(url).await.unwrap();
    let members = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner();
    let header = Some(pdpb::RequestHeader {
        cluster_id: members.header.unwrap().cluster_id,
        ..Default::default()
    });
    let missing = client
        .store_heartbeat(pdpb::StoreHeartbeatRequest {
            header: header.clone(),
            ..Default::default()
        })
        .await
        .unwrap_err();
    println!("missing stats: {:?} {}", missing.code(), missing.message());
    let store = metapb::Store {
        id: 1,
        address: "127.0.0.1:29999".into(),
        version: "8.5.0".into(),
        ..Default::default()
    };
    let response = client
        .bootstrap(pdpb::BootstrapRequest {
            header: header.clone(),
            store: Some(store),
            region: Some(metapb::Region {
                id: 1,
                region_epoch: Some(metapb::RegionEpoch {
                    conf_ver: 1,
                    version: 1,
                }),
                peers: vec![metapb::Peer {
                    id: 2,
                    store_id: 1,
                    ..Default::default()
                }],
                ..Default::default()
            }),
        })
        .await
        .unwrap()
        .into_inner();
    println!("bootstrap: {:?}", response.header);
    for id in [99, 1] {
        let result = client
            .store_heartbeat(pdpb::StoreHeartbeatRequest {
                header: header.clone(),
                stats: Some(pdpb::StoreStats {
                    store_id: id,
                    ..Default::default()
                }),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        println!("store {id}: {:?}", result);
    }
    let removed = client
        .put_store(pdpb::PutStoreRequest {
            header: header.clone(),
            store: Some(metapb::Store {
                id: 3,
                address: "127.0.0.1:29998".into(),
                version: "8.5.0".into(),
                state: 2,
                node_state: 3,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    println!("register removed: {:?}", removed);
    let response = client
        .store_heartbeat(pdpb::StoreHeartbeatRequest {
            header,
            stats: Some(pdpb::StoreStats {
                store_id: 3,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    println!("removed heartbeat: {:?}", response);
}
