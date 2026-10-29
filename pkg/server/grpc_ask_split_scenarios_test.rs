// Copyright 2026 AsterSQL.
#![allow(deprecated)]
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use prost::Message;
use std::collections::BTreeSet;
fn header(id: u64) -> Option<pdpb::RequestHeader> {
    Some(pdpb::RequestHeader {
        cluster_id: id,
        ..Default::default()
    })
}
fn record_header(h: &mut pdpb::ResponseHeader, id: u64) {
    assert_eq!(h.cluster_id, id);
    h.cluster_id = 0;
}
fn normalize(id: &mut u64, seen: &mut BTreeSet<u64>) {
    assert!(*id > 0);
    assert!(seen.insert(*id), "duplicate split ID {id}");
    *id = seen.len() as u64;
}
pub(crate) async fn observe(
    client: &mut PdClient<tonic::transport::Channel>,
    id: u64,
) -> Vec<Vec<u8>> {
    // Register the bootstrapped store through the public admission path too.
    // Go updates its minimum cluster version on PutStore.
    let put = client
        .put_store(pdpb::PutStoreRequest {
            header: header(id),
            store: Some(metapb::Store {
                id: 1,
                version: "8.5.0".into(),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(put.header.unwrap().error.is_none());
    let mut records = vec![];
    let mut seen = BTreeSet::new();
    let region = client
        .get_region_by_id(pdpb::GetRegionByIdRequest {
            header: header(id),
            region_id: 10,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .region
        .unwrap();
    for batch in [false, true] {
        // Missing message and an unowned key must reject without allocation.
        let mut rich = region.clone();
        rich.start_key = b"z".to_vec();
        rich.end_key = vec![0xff];
        rich.is_in_flashback = true;
        rich.flashback_start_ts = 9;
        rich.peers[1].is_witness = true;
        for r in [
            None,
            Some(metapb::Region {
                id: 99,
                start_key: b"z".to_vec(),
                ..Default::default()
            }),
            Some(rich),
        ] {
            if batch {
                let mut v = client
                    .ask_batch_split(pdpb::AskBatchSplitRequest {
                        header: header(id),
                        region: r,
                        split_count: 2,
                        ..Default::default()
                    })
                    .await
                    .unwrap()
                    .into_inner();
                assert!(v.ids.is_empty());
                record_header(v.header.as_mut().unwrap(), id);
                assert!(v.header.as_ref().unwrap().error.is_some());
                records.push(v.encode_to_vec());
            } else {
                let mut v = client
                    .ask_split(pdpb::AskSplitRequest {
                        header: header(id),
                        region: r,
                        ..Default::default()
                    })
                    .await
                    .unwrap()
                    .into_inner();
                assert_eq!(v.new_region_id, 0);
                assert!(v.new_peer_ids.is_empty());
                record_header(v.header.as_mut().unwrap(), id);
                assert!(v.header.as_ref().unwrap().error.is_some());
                records.push(v.encode_to_vec());
            }
        }
        for epoch in [
            Some((1, 1)),
            Some((2, 0)),
            Some((0, 0)),
            None,
            Some((2, 1)),
            Some((3, 2)),
        ] {
            let (version, conf_ver) = epoch.unwrap_or_default();
            let mut r = region.clone();
            r.region_epoch =
                epoch.map(|(version, conf_ver)| metapb::RegionEpoch { version, conf_ver });
            // Validation follows start key, not region ID or end key.
            if version == 3 {
                r.id = 999;
                r.end_key = b"different".to_vec();
                r.peers.clear();
            }
            let stale = version < 2 || conf_ver < 1;
            if batch {
                let mut v = client
                    .ask_batch_split(pdpb::AskBatchSplitRequest {
                        header: header(id),
                        region: Some(r),
                        split_count: 3,
                        ..Default::default()
                    })
                    .await
                    .unwrap()
                    .into_inner();
                assert_eq!(
                    v.ids.len(),
                    if stale { 0 } else { 3 },
                    "batch epoch {version}/{conf_ver}: {v:?}"
                );
                record_header(v.header.as_mut().unwrap(), id);
                assert_eq!(v.header.as_ref().unwrap().error.is_some(), stale);
                for s in &mut v.ids {
                    assert_eq!(s.new_peer_ids.len(), if version == 3 { 0 } else { 3 });
                    normalize(&mut s.new_region_id, &mut seen);
                    for p in &mut s.new_peer_ids {
                        normalize(p, &mut seen);
                    }
                }
                records.push(v.encode_to_vec());
            } else {
                let mut v = client
                    .ask_split(pdpb::AskSplitRequest {
                        header: header(id),
                        region: Some(r),
                        ..Default::default()
                    })
                    .await
                    .unwrap()
                    .into_inner();
                record_header(v.header.as_mut().unwrap(), id);
                assert_eq!(v.header.as_ref().unwrap().error.is_some(), stale);
                if !stale {
                    assert_eq!(v.new_peer_ids.len(), if version == 3 { 0 } else { 3 });
                    normalize(&mut v.new_region_id, &mut seen);
                    for p in &mut v.new_peer_ids {
                        normalize(p, &mut seen);
                    }
                }
                records.push(v.encode_to_vec());
            }
        }
    }
    let zero = client
        .ask_batch_split(pdpb::AskBatchSplitRequest {
            header: header(id),
            region: Some(region.clone()),
            split_count: 0,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(zero.header.unwrap().error.is_none());
    assert!(zero.ids.is_empty());
    // Rejected requests and zero count consume no allocator IDs.
    let a = client
        .alloc_id(pdpb::AllocIdRequest {
            header: header(id),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .id;
    let mut stale = region.clone();
    stale.region_epoch = None;
    client
        .ask_batch_split(pdpb::AskBatchSplitRequest {
            header: header(id),
            region: Some(stale),
            split_count: 3,
            ..Default::default()
        })
        .await
        .unwrap();
    let b = client
        .alloc_id(pdpb::AllocIdRequest {
            header: header(id),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .id;
    assert_eq!(b, a + 1);
    let mut workers = vec![];
    for _ in 0..8 {
        let mut c = client.clone();
        let r = region.clone();
        workers.push(tokio::spawn(async move {
            c.ask_batch_split(pdpb::AskBatchSplitRequest {
                header: header(id),
                region: Some(r),
                split_count: 4,
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner()
        }));
    }
    for w in workers {
        let r = w.await.unwrap();
        assert!(r.header.unwrap().error.is_none());
        assert_eq!(r.ids.len(), 4);
        for mut s in r.ids {
            normalize(&mut s.new_region_id, &mut seen);
            for mut p in s.new_peer_ids {
                normalize(&mut p, &mut seen);
            }
        }
    }
    assert_eq!(seen.len(), 148);
    assert_eq!(
        client
            .ask_split(pdpb::AskSplitRequest {
                header: header(id + 1),
                region: Some(region),
                ..Default::default()
            })
            .await
            .unwrap_err()
            .code(),
        tonic::Code::FailedPrecondition
    );
    records
}

pub(crate) async fn observe_follower(
    client: &mut PdClient<tonic::transport::Channel>,
    id: u64,
) -> (String, Vec<u8>) {
    let error = client
        .ask_split(pdpb::AskSplitRequest {
            header: header(id),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::Unavailable);
    let mut r = client
        .ask_batch_split(pdpb::AskBatchSplitRequest {
            header: header(id),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        r.header.as_ref().unwrap().error.as_ref().unwrap().r#type,
        pdpb::ErrorType::NotBootstrapped as i32
    );
    record_header(r.header.as_mut().unwrap(), id);
    (error.message().into(), r.encode_to_vec())
}

pub(crate) async fn observe_version_gate(
    client: &mut PdClient<tonic::transport::Channel>,
    id: u64,
) -> Vec<u8> {
    let mut r = client
        .ask_batch_split(pdpb::AskBatchSplitRequest {
            header: header(id),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    let h = r.header.as_mut().unwrap();
    assert_eq!(
        h.error.as_ref().unwrap().r#type,
        pdpb::ErrorType::IncompatibleVersion as i32
    );
    record_header(h, id);
    r.encode_to_vec()
}
