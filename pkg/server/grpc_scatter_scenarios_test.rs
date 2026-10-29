// Copyright 2026 AsterSQL.
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use serde_json::{Value, json};
use std::time::Duration;
type Client = PdClient<tonic::transport::Channel>;
fn header(id: u64) -> Option<pdpb::RequestHeader> {
    Some(pdpb::RequestHeader {
        cluster_id: id,
        ..Default::default()
    })
}
/// Real stores and heartbeats; scatter can only rebalance the three existing voters.
pub async fn observe(client: &mut Client, id: u64) -> Value {
    for store_id in 1..=3 {
        let response = client
            .put_store(pdpb::PutStoreRequest {
                header: header(id),
                store: Some(metapb::Store {
                    id: store_id,
                    address: format!("store:{store_id}"),
                    version: "8.5.0".into(),
                    node_state: 1,
                    ..Default::default()
                }),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        assert!(response.header.unwrap().error.is_none());
        client
            .store_heartbeat(pdpb::StoreHeartbeatRequest {
                header: header(id),
                stats: Some(pdpb::StoreStats {
                    store_id,
                    capacity: 1_000_000_000_000,
                    available: 900_000_000_000,
                    used_size: 100_000_000_000,
                    region_count: 3,
                    ..Default::default()
                }),
                ..Default::default()
            })
            .await
            .unwrap();
    }
    let peers: Vec<_> = (1..=3)
        .map(|store_id| metapb::Peer {
            id: 300 + store_id,
            store_id,
            ..Default::default()
        })
        .collect();
    let (tx, rx) = tokio::sync::mpsc::channel(16);
    let mut regions = vec![];
    for region_id in 100..=102 {
        let region = metapb::Region {
            id: region_id,
            start_key: vec![region_id as u8],
            end_key: vec![region_id as u8 + 1],
            peers: peers.clone(),
            region_epoch: Some(metapb::RegionEpoch {
                version: 3,
                conf_ver: 1,
            }),
            ..Default::default()
        };
        regions.push(region.clone());
        tx.send(pdpb::RegionHeartbeatRequest {
            header: header(id),
            region: Some(region),
            leader: Some(peers[0].clone()),
            approximate_size: 1048576,
            ..Default::default()
        })
        .await
        .unwrap();
    }
    let mut stream = tokio::time::timeout(
        Duration::from_secs(20),
        client.region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx)),
    )
    .await
    .unwrap()
    .unwrap()
    .into_inner();
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let mut ready = true;
            for region_id in 100..=102 {
                let r = client
                    .get_region_by_id(pdpb::GetRegionByIdRequest {
                        header: header(id),
                        region_id,
                        ..Default::default()
                    })
                    .await
                    .unwrap()
                    .into_inner();
                ready &= r.region.is_some() && r.leader.is_some();
            }
            if ready {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let r = client
        .scatter_region(pdpb::ScatterRegionRequest {
            header: header(id),
            regions_id: vec![100, 101, 102, 999],
            group: "task029".into(),
            skip_store_limit: true,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(r.header.unwrap().error.is_none());
    assert_eq!(r.finished_percentage, 75);
    assert_eq!(r.failed_regions_id, vec![999]);
    let mut running = vec![];
    for region_id in 100..=102 {
        let r = client
            .get_operator(pdpb::GetOperatorRequest {
                header: header(id),
                region_id,
            })
            .await
            .unwrap()
            .into_inner();
        if r.header.unwrap().error.is_none() {
            assert_eq!(r.region_id, region_id);
            assert_eq!(r.desc, b"scatter-region");
            assert_eq!(r.status, pdpb::OperatorStatus::Running as i32);
            running.push((region_id, String::from_utf8(r.kind).unwrap()));
        }
    }
    assert_eq!(
        running.len(),
        2,
        "balanced three-store scatter must move two leaders"
    );
    let active = running[0].0;
    let repeated = client
        .scatter_region(pdpb::ScatterRegionRequest {
            header: header(id),
            region_id: active,
            group: "task029".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(repeated.header.unwrap().error.is_none());
    assert_eq!(repeated.finished_percentage, 100);
    let conflict = client
        .scatter_region(pdpb::ScatterRegionRequest {
            header: header(id),
            region_id: active,
            group: "different".into(),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(conflict.code(), tonic::Code::Unknown);
    assert_eq!(
        conflict.message(),
        format!("the operator of region {active} already exist")
    );
    let mut commands = std::collections::BTreeMap::new();
    tokio::time::timeout(Duration::from_secs(15), async {
        while commands.len() < 2 {
            let r = stream.message().await.unwrap().unwrap();
            if let Some(transfer) = r.transfer_leader {
                commands.insert(r.region_id, transfer.peer.unwrap());
            }
        }
    })
    .await
    .expect("scatter operators must reach heartbeat transport");
    for (region_id, leader) in &commands {
        tx.send(pdpb::RegionHeartbeatRequest {
            header: header(id),
            region: regions.iter().find(|r| r.id == *region_id).cloned(),
            leader: Some(leader.clone()),
            approximate_size: 1048576,
            ..Default::default()
        })
        .await
        .unwrap();
    }
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let mut done = true;
            for (region_id, _) in &running {
                let r = client
                    .get_operator(pdpb::GetOperatorRequest {
                        header: header(id),
                        region_id: *region_id,
                    })
                    .await
                    .unwrap()
                    .into_inner();
                assert!(r.header.unwrap().error.is_none());
                done &= r.status == pdpb::OperatorStatus::Success as i32;
            }
            if done {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("completed operators must remain queryable");
    let mut kinds: Vec<_> = running.into_iter().map(|(_, kind)| kind).collect();
    kinds.sort();
    let single = client
        .scatter_region(pdpb::ScatterRegionRequest {
            header: header(id),
            region_id: 100,
            group: "task029".into(),
            skip_store_limit: true,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(single.header.unwrap().error.is_none());
    assert_eq!(single.finished_percentage, 100);
    drop(tx);
    drop(stream);
    json!({"percentage":r.finished_percentage,"failed":r.failed_regions_id,"running_kinds":kinds,"completed":2,"single":single.finished_percentage})
}

pub async fn unbootstrapped(client: &mut Client, id: u64) -> Value {
    let scatter = client
        .scatter_region(pdpb::ScatterRegionRequest {
            header: header(id),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    let operator = client
        .get_operator(pdpb::GetOperatorRequest {
            header: header(id),
            region_id: 100,
        })
        .await
        .unwrap()
        .into_inner();
    let errors = [scatter.header.unwrap(), operator.header.unwrap()].map(|h| {
        assert_eq!(h.cluster_id, id);
        let e = h.error.unwrap();
        assert_eq!(e.r#type, pdpb::ErrorType::NotBootstrapped as i32);
        json!([e.r#type, e.message])
    });
    json!(errors)
}
pub async fn errors(client: &mut Client, id: u64) -> Value {
    use prost::Message;
    let mut results = vec![];
    let mut missing = client
        .get_operator(pdpb::GetOperatorRequest {
            header: header(id),
            region_id: 999,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        missing
            .header
            .as_ref()
            .unwrap()
            .error
            .as_ref()
            .unwrap()
            .message,
        "Not Found"
    );
    assert_eq!(missing.header.as_ref().unwrap().cluster_id, id);
    missing.header.as_mut().unwrap().cluster_id = 0;
    results.push(json!(missing.encode_to_vec()));
    let mut single = client
        .scatter_region(pdpb::ScatterRegionRequest {
            header: header(id),
            region_id: 999,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        single
            .header
            .as_ref()
            .unwrap()
            .error
            .as_ref()
            .unwrap()
            .message,
        "region %d not found"
    );
    assert_eq!(single.header.as_ref().unwrap().cluster_id, id);
    single.header.as_mut().unwrap().cluster_id = 0;
    results.push(json!(single.encode_to_vec()));
    for request in [
        pdpb::ScatterRegionRequest {
            header: header(id),
            regions_id: vec![999],
            ..Default::default()
        },
        pdpb::ScatterRegionRequest {
            header: header(id),
            regions_id: vec![998, 999],
            ..Default::default()
        },
        pdpb::ScatterRegionRequest {
            header: header(id),
            region_id: 10,
            ..Default::default()
        },
        pdpb::ScatterRegionRequest {
            header: header(id),
            region_id: 998,
            region: Some(metapb::Region {
                id: 998,
                peers: (1..=1)
                    .map(|store_id| metapb::Peer {
                        id: 300 + store_id,
                        store_id,
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            }),
            ..Default::default()
        },
    ] {
        let error = client.scatter_region(request).await.unwrap_err();
        assert_eq!(error.code(), tonic::Code::Unknown);
        results.push(json!([format!("{:?}", error.code()), error.message()]));
    }
    json!(results)
}
