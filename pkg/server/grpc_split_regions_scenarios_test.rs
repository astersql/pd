// Copyright 2026 AsterSQL.
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use prost::Message;
use serde_json::{Value, json};
use std::time::Duration;
type Client = PdClient<tonic::transport::Channel>;
fn header(id: u64) -> Option<pdpb::RequestHeader> {
    Some(pdpb::RequestHeader {
        cluster_id: id,
        ..Default::default()
    })
}
fn request(id: u64, keys: &[&[u8]], retry_limit: u64) -> pdpb::SplitRegionsRequest {
    pdpb::SplitRegionsRequest {
        header: header(id),
        split_keys: keys.iter().map(|k| k.to_vec()).collect(),
        retry_limit,
    }
}
fn record(mut response: pdpb::SplitRegionsResponse, id: u64) -> Value {
    assert_eq!(response.header.as_ref().unwrap().cluster_id, id);
    response.header.as_mut().unwrap().cluster_id = 0;
    response.regions_id.sort();
    json!(response.encode_to_vec())
}
pub async fn unbootstrapped(client: &mut Client, id: u64) -> Value {
    let r = client
        .split_regions(request(id, &[], 0))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        r.header.as_ref().unwrap().error.as_ref().unwrap().r#type,
        pdpb::ErrorType::NotBootstrapped as i32
    );
    record(r, id)
}
fn region(id: u64, start: &[u8], end: &[u8], version: u64) -> metapb::Region {
    metapb::Region {
        id,
        start_key: start.to_vec(),
        end_key: end.to_vec(),
        region_epoch: Some(metapb::RegionEpoch {
            version,
            conf_ver: 1,
        }),
        peers: (1..=3)
            .map(|store_id| metapb::Peer {
                id: 300 + store_id,
                store_id,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}
async fn send(
    tx: &tokio::sync::mpsc::Sender<pdpb::RegionHeartbeatRequest>,
    id: u64,
    r: metapb::Region,
) {
    tx.send(pdpb::RegionHeartbeatRequest {
        header: header(id),
        leader: r.peers.first().cloned(),
        region: Some(r),
        approximate_size: 1048576,
        ..Default::default()
    })
    .await
    .unwrap();
}
async fn wait_region(client: &mut Client, id: u64, r: &metapb::Region) {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let response = client
                .get_region_by_id(pdpb::GetRegionByIdRequest {
                    header: header(id),
                    region_id: r.id,
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner();
            if response.region.as_ref() == Some(r) && response.leader.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
/// Same requests and real heartbeat side effects run against both implementations.
pub async fn observe(client: &mut Client, id: u64) -> Value {
    let mut output = vec![];
    for (keys, retries) in [
        (vec![], 0),
        (vec![b"a".as_slice()], 0),
        (vec![b"b".as_slice()], u64::MAX),
    ] {
        let r = client
            .split_regions(request(id, &keys, retries))
            .await
            .unwrap()
            .into_inner();
        assert!(r.header.as_ref().unwrap().error.is_none());
        assert_eq!(r.finished_percentage, 0);
        assert!(r.regions_id.is_empty());
        output.push(record(r, id));
    }
    let error = client
        .split_regions(request(id + 1, &[], 0))
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::FailedPrecondition);
    // Cluster IDs vary; preserve both actual IDs before normalizing the text.
    assert_eq!(
        error.message(),
        format!("mismatch cluster id, need {id} but got {}", id + 1)
    );
    output.push(json!([
        format!("{:?}", error.code()),
        "mismatch cluster id"
    ]));
    for store_id in 1..=3 {
        let r = client
            .put_store(pdpb::PutStoreRequest {
                header: header(id),
                store: Some(metapb::Store {
                    id: store_id,
                    address: format!("split-store:{store_id}"),
                    version: "8.5.0".into(),
                    node_state: 1,
                    ..Default::default()
                }),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        assert!(r.header.unwrap().error.is_none());
    }
    let (tx, rx) = tokio::sync::mpsc::channel(16);
    let original = region(100, b"a", b"w", 3);
    send(&tx, id, original.clone()).await;
    let mut stream = client
        .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap()
        .into_inner();
    wait_region(client, id, &original).await;
    for partial in [false, true] {
        let keys: Vec<&[u8]> = if partial {
            vec![b"p", b"t"]
        } else {
            vec![b"m"]
        };
        let mut caller = client.clone();
        let req = request(id, &keys, 0);
        let task = tokio::spawn(async move { caller.split_regions(req).await });
        let expected_id = if partial { 102 } else { 100 };
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let message = stream.message().await.unwrap().unwrap();
                if message.region_id == expected_id && message.split_region.is_some() {
                    let split = message.split_region.unwrap();
                    assert_eq!(split.policy, 2);
                    assert_eq!(
                        split.keys,
                        keys.iter().map(|k| k.to_vec()).collect::<Vec<_>>()
                    );
                    assert_eq!(message.target_peer.unwrap().store_id, 1);
                    break;
                }
            }
        })
        .await
        .expect("real split command must reach heartbeat stream");
        assert!(
            !task.is_finished(),
            "admission alone must not count as a split"
        );
        let op = client
            .get_operator(pdpb::GetOperatorRequest {
                header: header(id),
                region_id: expected_id,
            })
            .await
            .unwrap()
            .into_inner();
        assert!(op.header.unwrap().error.is_none());
        assert_eq!(op.desc, b"region-splitter");
        assert_eq!(op.status, pdpb::OperatorStatus::Running as i32);
        let replacements = if partial {
            vec![region(102, b"m", b"p", 5), region(103, b"p", b"w", 5)]
        } else {
            vec![region(101, b"a", b"m", 4), region(102, b"m", b"w", 4)]
        };
        for r in &replacements {
            send(&tx, id, r.clone()).await;
        }
        for r in &replacements {
            wait_region(client, id, r).await;
        }
        let r = tokio::time::timeout(Duration::from_secs(65), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap()
            .into_inner();
        assert!(r.header.as_ref().unwrap().error.is_none());
        assert_eq!(r.finished_percentage, if partial { 50 } else { 100 });
        assert_eq!(r.regions_id, if partial { vec![103] } else { vec![102] });
        output.push(record(r, id));
    }
    // A client deadline cancels the active watch. It cannot invent a completed boundary.
    let mut caller = client.clone();
    let mut req = tonic::Request::new(request(id, &[b"d"], 0));
    req.set_timeout(Duration::from_millis(100));
    let cancelled = caller.split_regions(req).await.unwrap_err();
    assert!(matches!(
        cancelled.code(),
        tonic::Code::Cancelled | tonic::Code::DeadlineExceeded
    ));
    let r = client
        .get_region(pdpb::GetRegionRequest {
            header: header(id),
            region_key: b"d".to_vec(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(r.region.unwrap(), region(101, b"a", b"m", 4));
    output.push(json!([
        format!("{:?}", cancelled.code()),
        cancelled.message(),
        "cancelled-without-boundary"
    ]));
    drop(tx);
    drop(stream);
    json!(output)
}
