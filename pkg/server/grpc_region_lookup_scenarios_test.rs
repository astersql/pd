// Copyright 2026 AsterSQL.
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use prost::Message;
use std::time::Duration;
pub(crate) type Client = PdClient<tonic::transport::Channel>;
pub(crate) fn header(id: u64) -> Option<pdpb::RequestHeader> {
    Some(pdpb::RequestHeader {
        cluster_id: id,
        ..Default::default()
    })
}
pub(crate) async fn prepare(client: &mut Client, id: u64) {
    let missing = client
        .get_region(pdpb::GetRegionRequest {
            header: header(id),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        missing.header.unwrap().error.unwrap().r#type,
        pdpb::ErrorType::NotBootstrapped as i32
    );
    let peer = metapb::Peer {
        id: 3,
        store_id: 1,
        ..Default::default()
    };
    let down = metapb::Peer {
        id: 4,
        store_id: 2,
        ..Default::default()
    };
    let pending = metapb::Peer {
        id: 5,
        store_id: 3,
        role: 1,
        ..Default::default()
    };
    let boot = client
        .bootstrap(pdpb::BootstrapRequest {
            header: header(id),
            store: Some(metapb::Store {
                id: 1,
                version: "8.5.0".into(),
                ..Default::default()
            }),
            region: Some(metapb::Region {
                id: 2,
                peers: vec![peer.clone()],
                ..Default::default()
            }),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(boot.header.unwrap().error.is_none());
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    for (rid, start, end) in [
        (10, b"a".as_slice(), b"m".as_slice()),
        (11, b"m", b"z"),
        (12, b"zz", b""),
    ] {
        tx.send(pdpb::RegionHeartbeatRequest {
            header: header(id),
            region: Some(metapb::Region {
                id: rid,
                start_key: start.into(),
                end_key: end.into(),
                peers: vec![peer.clone(), down.clone(), pending.clone()],
                region_epoch: Some(metapb::RegionEpoch {
                    version: 2,
                    conf_ver: 1,
                }),
                ..Default::default()
            }),
            leader: Some(peer.clone()),
            down_peers: vec![pdpb::PeerStats {
                peer: Some(down.clone()),
                down_seconds: 17,
            }],
            pending_peers: vec![pending.clone()],
            approximate_size: 1048576,
            ..Default::default()
        })
        .await
        .unwrap();
    }
    let mut stream = client
        .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap()
        .into_inner();
    for rid in [10, 11, 12] {
        tokio::time::timeout(Duration::from_secs(20),async {loop{
   let r=client.get_region_by_id(pdpb::GetRegionByIdRequest{header:header(id),region_id:rid,..Default::default()}).await.unwrap().into_inner();
   if r.region.as_ref().is_some_and(|r|r.id==rid) && r.leader==Some(peer.clone()){break;}
   tokio::select!{r=stream.message()=>panic!("unexpected heartbeat error: {r:?}"),_=tokio::time::sleep(Duration::from_millis(20))=>{}}
  }}).await.unwrap();
    }
    drop(tx);
    drop(stream);
}
fn request<T>(r: T, follower: bool) -> tonic::Request<T> {
    let mut r = tonic::Request::new(r);
    if follower {
        r.metadata_mut()
            .insert("pd-allow-follower-handle", "false".parse().unwrap());
    }
    r
}
fn record(mut r: pdpb::GetRegionResponse, id: u64, found: Option<u64>, follower: bool) -> Vec<u8> {
    let h = r.header.as_mut().unwrap();
    assert_eq!(h.cluster_id, id);
    assert_eq!(r.region.as_ref().map(|r| r.id), found);
    if follower && found.is_none() {
        assert_eq!(
            h.error.as_ref().unwrap().r#type,
            pdpb::ErrorType::RegionNotFound as i32
        );
        assert_eq!(h.error.as_ref().unwrap().message, "region not found");
    } else {
        assert!(h.error.is_none());
    }
    if found.is_some() {
        assert_eq!(r.leader.as_ref().unwrap().id, 3);
        if follower {
            assert!(r.down_peers.is_empty());
            assert!(r.pending_peers.is_empty());
        } else {
            assert_eq!(r.down_peers[0].down_seconds, 17);
            assert_eq!(r.down_peers[0].peer.as_ref().unwrap().id, 4);
            assert_eq!(r.pending_peers[0].id, 5);
        }
    }
    assert!(r.buckets.is_none());
    h.cluster_id = 0;
    r.encode_to_vec()
}
pub(crate) async fn observe(client: &mut Client, id: u64, follower: bool) -> Vec<Vec<u8>> {
    if follower {
        let denied = client
            .get_region(pdpb::GetRegionRequest {
                header: header(id),
                ..Default::default()
            })
            .await
            .unwrap_err();
        assert_eq!(denied.code(), tonic::Code::Unavailable);
        assert_eq!(
            denied.message(),
            "not leader and follower handling not allowed"
        );
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let r = client
                    .get_region_by_id(request(
                        pdpb::GetRegionByIdRequest {
                            header: header(id),
                            region_id: 12,
                            ..Default::default()
                        },
                        true,
                    ))
                    .await
                    .unwrap()
                    .into_inner();
                if r.region.is_some() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap();
    }
    let mut out = vec![];
    for (key, current, previous) in [
        (b"".as_slice(), None, None),
        (b"a", Some(10), None),
        (b"l", Some(10), None),
        (b"m", Some(11), Some(10)),
        (b"y", Some(11), Some(10)),
        (b"z", None, None),
        (b"zz", Some(12), None),
        (&[255], Some(12), None),
    ] {
        for need_buckets in [false, true] {
            let r = pdpb::GetRegionRequest {
                header: header(id),
                region_key: key.into(),
                need_buckets,
            };
            out.push(record(
                client
                    .get_region(request(r.clone(), follower))
                    .await
                    .unwrap()
                    .into_inner(),
                id,
                current,
                follower,
            ));
            out.push(record(
                client
                    .get_prev_region(request(r, follower))
                    .await
                    .unwrap()
                    .into_inner(),
                id,
                previous,
                follower,
            ));
        }
    }
    for rid in [0, 2, 10, 11, 12, 999] {
        for need_buckets in [false, true] {
            out.push(record(
                client
                    .get_region_by_id(request(
                        pdpb::GetRegionByIdRequest {
                            header: header(id),
                            region_id: rid,
                            need_buckets,
                        },
                        follower,
                    ))
                    .await
                    .unwrap()
                    .into_inner(),
                id,
                if (10..=12).contains(&rid) {
                    Some(rid)
                } else {
                    None
                },
                follower,
            ));
        }
    }
    let e = client
        .get_region(request(
            pdpb::GetRegionRequest {
                header: header(id + 1),
                ..Default::default()
            },
            follower,
        ))
        .await
        .unwrap_err();
    assert_eq!(e.code(), tonic::Code::FailedPrecondition);
    out
}
