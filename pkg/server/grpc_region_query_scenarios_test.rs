// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use std::time::Duration;
type Client = PdClient<tonic::transport::Channel>;
fn header(id: u64) -> Option<pdpb::RequestHeader> {
    Some(pdpb::RequestHeader {
        cluster_id: id,
        ..Default::default()
    })
}
pub(crate) async fn observe(
    client: &mut Client,
    id: u64,
    follower: bool,
) -> Vec<pdpb::QueryRegionResponse> {
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    tx.send(pdpb::QueryRegionRequest {
        header: header(id),
        ..Default::default()
    })
    .await
    .unwrap();
    let mut stream = client
        .query_region(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap()
        .into_inner();
    let warmup = stream.message().await.unwrap().unwrap();
    assert_eq!(warmup.header.unwrap().cluster_id, id);
    // QueryRegion permits follower reads without the unary opt-in metadata.
    if follower {
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                tx.send(pdpb::QueryRegionRequest {
                    header: header(id),
                    ids: vec![12],
                    ..Default::default()
                })
                .await
                .unwrap();
                let r = stream.message().await.unwrap().unwrap();
                if r.regions_by_id.get(&12).is_some_and(|r| r.region.is_some()) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        })
        .await
        .unwrap();
    }
    let mut out = vec![];
    for need_buckets in [false, true] {
        // Pipelined requests, repeated keys and IDs, zero IDs, missing IDs and holes.
        let requests = vec![
            pdpb::QueryRegionRequest {
                header: header(id),
                need_buckets,
                keys: vec![
                    b"a".to_vec(),
                    b"m".to_vec(),
                    b"z".to_vec(),
                    vec![255],
                    b"a".to_vec(),
                ],
                prev_keys: vec![b"m".to_vec(), b"zz".to_vec(), vec![]],
                ids: vec![0, 10, 999, 999],
            },
            pdpb::QueryRegionRequest {
                header: header(id),
                ..Default::default()
            },
            pdpb::QueryRegionRequest {
                header: header(id),
                ids: vec![11],
                ..Default::default()
            },
        ];
        for r in requests {
            tx.send(r).await.unwrap();
        }
        for i in 0..3 {
            let mut r = tokio::time::timeout(Duration::from_secs(10), stream.message())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let h = r.header.as_mut().unwrap();
            assert_eq!(h.cluster_id, id);
            assert!(h.error.is_none());
            h.cluster_id = 0;
            if i == 0 {
                assert_eq!(r.key_id_map, vec![10, 11, 0, 12, 10]);
                assert_eq!(r.prev_key_id_map, vec![10, 0, 0]);
                assert_eq!(r.regions_by_id.len(), 4);
                assert_eq!(r.regions_by_id[&999], pdpb::RegionResponse::default());
                assert!(!r.regions_by_id.contains_key(&0));
                for rid in [10, 11, 12] {
                    let region = &r.regions_by_id[&rid];
                    assert_eq!(region.region.as_ref().unwrap().id, rid);
                    assert_eq!(region.leader.as_ref().unwrap().id, 3);
                    assert!(region.buckets.is_none());
                    if follower {
                        assert!(region.down_peers.is_empty());
                        assert!(region.pending_peers.is_empty());
                    } else {
                        assert_eq!(region.down_peers[0].down_seconds, 17);
                        assert_eq!(region.pending_peers[0].id, 5);
                    }
                }
            } else if i == 1 {
                assert!(r.regions_by_id.is_empty());
                assert!(r.key_id_map.is_empty());
            } else {
                assert_eq!(r.regions_by_id.len(), 1);
                assert!(r.regions_by_id.contains_key(&11));
            }
            out.push(r);
        }
    }
    drop(tx);
    assert!(stream.message().await.unwrap().is_none());
    // Wrong cluster ID terminates the stream after earlier successful responses.
    let requests = [
        pdpb::QueryRegionRequest {
            header: header(id),
            ..Default::default()
        },
        pdpb::QueryRegionRequest {
            header: header(id + 1),
            ..Default::default()
        },
    ];
    let mut stream = client
        .query_region(tokio_stream::iter(requests))
        .await
        .unwrap()
        .into_inner();
    assert!(
        stream
            .message()
            .await
            .unwrap()
            .unwrap()
            .header
            .unwrap()
            .error
            .is_none()
    );
    let error = stream.message().await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::FailedPrecondition);
    assert_eq!(
        error.message(),
        format!("mismatch cluster id, need {id} but got {}", id + 1)
    );
    // Empty half-close and repeated client cancellation leave the connection usable.
    let mut empty = client
        .query_region(tokio_stream::empty::<pdpb::QueryRegionRequest>())
        .await
        .unwrap()
        .into_inner();
    assert!(empty.message().await.unwrap().is_none());
    for _ in 0..3 {
        let (tx, rx) = tokio::sync::mpsc::channel::<pdpb::QueryRegionRequest>(1);
        tx.send(pdpb::QueryRegionRequest {
            header: header(id),
            ..Default::default()
        })
        .await
        .unwrap();
        let stream = client
            .query_region(tokio_stream::wrappers::ReceiverStream::new(rx))
            .await
            .unwrap();
        drop(stream);
        drop(tx);
    }
    assert!(
        client
            .get_members(pdpb::GetMembersRequest::default())
            .await
            .is_ok()
    );
    out
}
