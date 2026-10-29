// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use prost::Message;
type Client = PdClient<tonic::transport::Channel>;
fn request<T>(r: T, follower: bool) -> tonic::Request<T> {
    let mut r = tonic::Request::new(r);
    if follower {
        r.metadata_mut()
            .insert("pd-allow-follower-handle", "false".parse().unwrap());
    }
    r
}
fn header(id: u64) -> Option<pdpb::RequestHeader> {
    Some(pdpb::RequestHeader {
        cluster_id: id,
        ..Default::default()
    })
}
pub(crate) async fn observe(client: &mut Client, id: u64, follower: bool) -> Vec<Vec<u8>> {
    let mut out = vec![];
    if follower {
        assert_eq!(
            client
                .scan_regions(pdpb::ScanRegionsRequest {
                    header: header(id),
                    ..Default::default()
                })
                .await
                .unwrap_err()
                .code(),
            tonic::Code::Unavailable
        );
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            loop {
                let r = client
                    .scan_regions(request(
                        pdpb::ScanRegionsRequest {
                            header: header(id),
                            ..Default::default()
                        },
                        true,
                    ))
                    .await
                    .unwrap()
                    .into_inner();
                if r.regions.len() == 3 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
    }
    for (start, end, limit, ids) in [
        (b"".as_slice(), b"".as_slice(), 0, vec![10, 11, 12]),
        (b"b", b"z", 1, vec![10]),
        (b"m", b"zz", 0, vec![11]),
        (b"z", b"zz", 0, vec![]),
        (b"z", b"", 0, vec![12]),
        (b"b", b"b", 0, vec![10]),
        (&[255], b"", 0, vec![12]),
    ] {
        let mut r = client
            .scan_regions(request(
                pdpb::ScanRegionsRequest {
                    header: header(id),
                    start_key: start.into(),
                    end_key: end.into(),
                    limit,
                },
                follower,
            ))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(
            r.regions
                .iter()
                .map(|r| r.region.as_ref().unwrap().id)
                .collect::<Vec<_>>(),
            ids
        );
        assert_eq!(
            r.region_metas,
            r.regions
                .iter()
                .map(|r| r.region.clone().unwrap())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            r.leaders,
            r.regions
                .iter()
                .map(|r| r.leader.clone().unwrap())
                .collect::<Vec<_>>()
        );
        let h = r.header.as_mut().unwrap();
        assert_eq!(h.cluster_id, id);
        h.cluster_id = 0;
        out.push(r.encode_to_vec());
    }
    for (ranges, limit, all, expected) in [
        (vec![], 0, false, vec![]),
        (
            vec![(b"".as_slice(), b"".as_slice())],
            0,
            false,
            vec![10, 11, 12],
        ),
        (vec![(b"a", b"m"), (b"m", b"z")], 0, true, vec![10, 11]),
        (vec![(b"b", b"c"), (b"d", b"e")], 0, false, vec![10]),
        (vec![(b"a", b"")], 1, true, vec![10]),
        (vec![(b"a", b"z"), (b"m", b"zz")], 0, false, vec![]),
        (vec![(b"z", b"a")], 0, false, vec![]),
        (vec![(b"a", b"zz")], 0, true, vec![]),
        (vec![(b"b", b"zzzz")], 0, true, vec![]),
        (vec![(b"z", b"zz")], 0, true, vec![]),
    ] {
        for need_buckets in [false, true] {
            let ranges = ranges
                .iter()
                .map(|(a, b)| pdpb::KeyRange {
                    start_key: a.to_vec(),
                    end_key: b.to_vec(),
                })
                .collect();
            let mut r = client
                .batch_scan_regions(request(
                    pdpb::BatchScanRegionsRequest {
                        header: header(id),
                        ranges,
                        limit,
                        contain_all_key_range: all,
                        need_buckets,
                    },
                    follower,
                ))
                .await
                .unwrap()
                .into_inner();
            assert_eq!(
                r.regions
                    .iter()
                    .map(|r| r.region.as_ref().unwrap().id)
                    .collect::<Vec<_>>(),
                expected
            );
            let h = r.header.as_mut().unwrap();
            assert_eq!(h.cluster_id, id);
            h.cluster_id = 0;
            out.push(r.encode_to_vec());
        }
    }
    assert_eq!(
        client
            .batch_scan_regions(request(
                pdpb::BatchScanRegionsRequest {
                    header: header(id + 1),
                    ..Default::default()
                },
                follower
            ))
            .await
            .unwrap_err()
            .code(),
        tonic::Code::FailedPrecondition
    );
    out
}
