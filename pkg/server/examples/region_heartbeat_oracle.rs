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
    let peer = metapb::Peer {
        id: 2,
        store_id: 1,
        ..Default::default()
    };
    let base = pdpb::RegionHeartbeatRequest {
        header: header.clone(),
        region: Some(metapb::Region {
            id: 1,
            region_epoch: Some(metapb::RegionEpoch {
                conf_ver: 2,
                version: 2,
            }),
            peers: vec![peer.clone()],
            ..Default::default()
        }),
        leader: Some(peer),
        term: 7,
        interval: Some(pdpb::TimeInterval {
            start_timestamp: 10,
            end_timestamp: 20,
        }),
        query_stats: Some(pdpb::QueryStats {
            get: 11,
            put: 13,
            ..Default::default()
        }),
        cpu_stats: Some(pdpb::CpuStats {
            unified_read: 17,
            scheduler: 999,
        }),
        approximate_size: 2097152,
        ..Default::default()
    };
    for (name, request) in [
        ("accepted", base.clone()),
        ("duplicate", base.clone()),
        ("stale", {
            let mut r = base.clone();
            r.region
                .as_mut()
                .unwrap()
                .region_epoch
                .as_mut()
                .unwrap()
                .version = 1;
            r
        }),
        ("invalid region", {
            let mut r = base.clone();
            r.region.as_mut().unwrap().id = 0;
            r
        }),
        ("nil leader", {
            let mut r = base.clone();
            r.leader = None;
            r
        }),
    ] {
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        tx.send(request).await.unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_millis(500), async {
            match client
                .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
                .await
            {
                Ok(s) => s.into_inner().message().await,
                Err(e) => Err(e),
            }
        })
        .await;
        println!("{name}: {result:?}");
        let snapshot = client
            .get_region_by_id(pdpb::GetRegionByIdRequest {
                header: header.clone(),
                region_id: 1,
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        println!("snapshot: {snapshot:?}");
        drop(tx);
    }
}
