// Copyright 2026 AsterSQL.
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
pub fn header(id: u64) -> Option<pdpb::RequestHeader> {
    Some(pdpb::RequestHeader {
        cluster_id: id,
        ..Default::default()
    })
}
fn region(start: &[u8], end: &[u8]) -> metapb::Region {
    metapb::Region {
        id: 42,
        start_key: start.into(),
        end_key: end.into(),
        ..Default::default()
    }
}
pub async fn observe(
    client: &mut PdClient<tonic::transport::Channel>,
    id: u64,
) -> Vec<pdpb::ResponseHeader> {
    let query = pdpb::GetRegionByIdRequest {
        header: header(id),
        region_id: 10,
        ..Default::default()
    };
    let before = client
        .get_region_by_id(query.clone())
        .await
        .unwrap()
        .into_inner();
    let a = region(b"a", b"m");
    let b = region(b"m", b"z");
    let mut epoch = a.clone();
    epoch.region_epoch = Some(metapb::RegionEpoch {
        version: 999,
        conf_ver: 1,
    });
    let cases = vec![
        (None, None, false),
        (Some(a.clone()), None, false),
        (Some(a.clone()), Some(b.clone()), true),
        (Some(epoch), Some(b.clone()), true),
        (Some(a.clone()), Some(region(b"n", b"z")), false),
        (Some(region(b"z", b"m")), Some(b.clone()), false),
        (Some(a.clone()), Some(region(b"m", b"")), true),
        (Some(region(b"z", b"m")), Some(region(b"m", b"")), true),
    ];
    let mut out = vec![];
    for (left, right, valid) in cases {
        let mut h = client
            .report_split(pdpb::ReportSplitRequest {
                header: header(id),
                left,
                right,
            })
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap();
        assert_eq!(h.error.is_none(), valid, "{h:?}");
        if let Some(e) = &h.error {
            assert_eq!(e.message, "invalid split region");
            assert_eq!(e.r#type, pdpb::ErrorType::Unknown as i32);
        }
        assert_eq!(h.cluster_id, id);
        h.cluster_id = 0;
        out.push(h);
    }
    for (regions, valid) in [
        (vec![], false),
        (vec![a.clone()], false),
        (vec![a.clone(), b.clone()], true),
        (vec![a.clone(), region(b"n", b"z")], false),
        (vec![region(b"z", b"m"), b.clone()], false),
        (vec![a, b, region(b"z", b"")], true),
        (vec![region(b"z", b"m"), region(b"m", b"")], true),
    ] {
        let mut h = client
            .report_batch_split(pdpb::ReportBatchSplitRequest {
                header: header(id),
                regions,
            })
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap();
        assert_eq!(h.error.is_none(), valid, "{h:?}");
        if let Some(e) = &h.error {
            assert_eq!(e.message, "invalid split region");
        }
        assert_eq!(h.cluster_id, id);
        h.cluster_id = 0;
        out.push(h);
    }
    assert!(
        client
            .report_split(pdpb::ReportSplitRequest {
                header: header(id + 1),
                ..Default::default()
            })
            .await
            .is_err()
    );
    assert_eq!(
        before,
        client.get_region_by_id(query).await.unwrap().into_inner()
    );
    out
}
pub async fn observe_follower(
    client: &mut PdClient<tonic::transport::Channel>,
    id: u64,
) -> Vec<(tonic::Code, String)> {
    let single = client
        .report_split(pdpb::ReportSplitRequest {
            header: header(id),
            ..Default::default()
        })
        .await
        .unwrap_err();
    let batch = client
        .report_batch_split(pdpb::ReportBatchSplitRequest {
            header: header(id),
            ..Default::default()
        })
        .await
        .unwrap_err();
    [single, batch]
        .into_iter()
        .map(|e| {
            assert_eq!(e.code(), tonic::Code::Unavailable);
            (e.code(), e.message().into())
        })
        .collect()
}
