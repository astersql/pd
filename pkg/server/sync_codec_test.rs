// Copyright 2026 AsterSQL.
use super::sync_codec::*;
use astersql_pd_proto::pdpb;
use prost::Message;
fn fixtures() -> Vec<Vec<u8>> {
    let rows: Vec<String> = serde_json::from_str(include_str!(
        "../../tests/rust-compat/fixtures/state/task-50.json"
    ))
    .unwrap();
    rows.iter()
        .map(|s| {
            (0..s.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
                .collect()
        })
        .collect()
}
#[test]
fn state_task_50_sync_codec_go_wire() {
    let raw = fixtures();
    let first = response_from_proto(pdpb::SyncRegionResponse::decode(raw[0].as_slice()).unwrap())
        .into_apply();
    assert_eq!(first.regions.len(), 1);
    assert_eq!(first.stats[0].written_bytes, 1234);
    assert_eq!(first.buckets[0].as_ref().unwrap().version, 7);
    let end = response_from_proto(pdpb::SyncRegionResponse::decode(raw[1].as_slice()).unwrap())
        .into_apply();
    assert_eq!(end.start_index.0, 1);
}

use astersql_pd_core::{BasicCluster, KeyRange, RegionInfo, RegionMeta};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_proto::metapb;
use astersql_pd_storage::{MemoryStore, Storage};
use astersql_pd_syncer::{
    CatchUpFrame, ClientApply, FullSnapshot, HistoryBuffer, HistoryIndex as I, HistoryReplay,
    SyncFrame,
};
use std::{num::NonZeroUsize, sync::Arc};

fn roundtrip(p: pdpb::SyncRegionResponse) -> pdpb::SyncRegionResponse {
    let wire = p.encode_to_vec();
    let result = response_to_proto(response_from_proto(p.clone()));
    assert_eq!(result, p);
    assert_eq!(result.encode_to_vec(), wire);
    result
}
#[test]
fn state_task_50_sync_codec_request_presence_and_all_member_fields() {
    let p = pdpb::SyncRegionRequest {
        header: Some(pdpb::RequestHeader {
            cluster_id: 42,
            sender_id: 7,
            caller_id: "oracle".into(),
            caller_component: "pd".into(),
        }),
        member: Some(pdpb::Member {
            name: "pd2".into(),
            member_id: u64::MAX,
            peer_urls: vec!["peer1".into(), "peer2".into()],
            client_urls: vec!["client".into()],
            leader_priority: -1,
            deploy_path: "/pd".into(),
            binary_version: "v".into(),
            git_hash: "hash".into(),
            dc_location: "dc".into(),
        }),
        start_index: u64::MAX,
    };
    for p in [
        p,
        pdpb::SyncRegionRequest::default(),
        pdpb::SyncRegionRequest {
            header: Some(pdpb::RequestHeader::default()),
            member: Some(pdpb::Member::default()),
            ..Default::default()
        },
    ] {
        let bytes = p.encode_to_vec();
        let restored = request_to_proto(request_from_proto(p.clone()));
        assert_eq!(restored, p);
        assert_eq!(restored.encode_to_vec(), bytes);
    }
}
#[test]
fn state_task_50_sync_codec_lossless_pinned_response_fields() {
    let peer = metapb::Peer {
        id: 3,
        store_id: 4,
        role: 97,
        is_witness: true,
    };
    let p = pdpb::SyncRegionResponse {
        header: Some(pdpb::ResponseHeader {
            cluster_id: 42,
            error: Some(pdpb::Error {
                r#type: 123,
                message: "stopped".into(),
            }),
        }),
        start_index: u64::MAX,
        regions: vec![
            metapb::Region {
                id: 99,
                start_key: vec![0, 1],
                end_key: vec![255],
                peers: vec![peer],
                region_epoch: Some(metapb::RegionEpoch {
                    conf_ver: 7,
                    version: 8,
                }),
                encryption_meta: Some(astersql_pd_proto::encryptionpb::EncryptionMeta {
                    key_id: 12,
                    iv: vec![0, 2],
                }),
                is_in_flashback: true,
                flashback_start_ts: 321,
            },
            metapb::Region::default(),
        ],
        region_stats: vec![pdpb::RegionStat {
            bytes_written: u64::MAX,
            keys_written: 22,
            bytes_read: 33,
            keys_read: 44,
        }],
        region_leaders: vec![peer, metapb::Peer::default(), peer],
        buckets: vec![
            metapb::Buckets {
                region_id: 99,
                version: 7,
                keys: vec![vec![], vec![0, 1], vec![]],
                stats: Some(metapb::BucketStats {
                    read_bytes: vec![1, 2],
                    read_keys: vec![3],
                    read_qps: vec![4],
                    write_bytes: vec![5],
                    write_keys: vec![6],
                    write_qps: vec![7],
                }),
                period_in_ms: 2000,
            },
            metapb::Buckets::default(),
        ],
        down_peers: vec![
            pdpb::PeersStats {
                peers: vec![
                    pdpb::PeerStats {
                        peer: Some(peer),
                        down_seconds: u64::MAX,
                    },
                    pdpb::PeerStats::default(),
                ],
            },
            pdpb::PeersStats::default(),
        ],
        pending_peers: vec![pdpb::Peers { peers: vec![peer] }, pdpb::Peers::default()],
    };
    roundtrip(p);
    roundtrip(pdpb::SyncRegionResponse::default());
    roundtrip(pdpb::SyncRegionResponse {
        header: Some(pdpb::ResponseHeader::default()),
        ..Default::default()
    });
}
#[test]
fn state_task_50_sync_codec_send_matches_go_raw_wire() {
    let raw = fixtures();
    let p = pdpb::SyncRegionResponse::decode(raw[0].as_slice()).unwrap();
    assert_eq!(roundtrip(p.clone()).encode_to_vec(), raw[0]);
    let mut r = RegionInfo::new(
        region_from(p.regions[0].clone()),
        Some(crate::core_region_adapter::peer_from_proto(
            p.region_leaders[0],
        )),
        vec![],
        vec![],
    );
    r.stats_mut().written_bytes = 1234;
    r.stats_mut().read_bytes = 4567;
    r.stats_mut().written_keys = 23;
    r.stats_mut().read_keys = 89;
    r.set_bucket_meta(Some(astersql_pd_core::RegionBucketMeta {
        version: 7,
        keys: vec![vec![], b"m".to_vec(), vec![]],
    }));
    let cluster_id = p.header.unwrap().cluster_id;
    let records = [Arc::new(r)];
    assert_eq!(
        catch_up_to_proto(
            cluster_id,
            CatchUpFrame::Records {
                start_index: 0,
                records: &records
            }
        )
        .encode_to_vec(),
        raw[0]
    );
    assert_eq!(
        history_to_proto(cluster_id, &SyncFrame::keepalive(I(1))).encode_to_vec(),
        raw[1]
    );
    let missing = [Arc::new(RegionInfo::new(
        RegionMeta::default(),
        None,
        vec![],
        vec![],
    ))];
    let p = catch_up_to_proto(
        42,
        CatchUpFrame::Records {
            start_index: 10,
            records: &missing,
        },
    );
    assert_eq!(p.region_leaders, [metapb::Peer::default()]);
    assert_eq!(p.buckets, [metapb::Buckets::default()]);
    assert_eq!(p.region_stats, [pdpb::RegionStat::default()]);
}
fn region_from(p: metapb::Region) -> RegionMeta {
    crate::core_region_adapter::region_from_proto(Some(p)).unwrap()
}
fn follower() -> (
    ClientApply,
    BasicCluster,
    Storage,
    ClusterPaths,
    HistoryBuffer,
) {
    (
        ClientApply::default(),
        BasicCluster::new(),
        Storage::new(Arc::new(MemoryStore::default())),
        ClusterPaths::new(50),
        HistoryBuffer::with_capacity_unit(2048, 1).unwrap(),
    )
}
#[tokio::test]
async fn state_task_50_sync_codec_mismatch_go_application() {
    // Same production inputs as task37's saved Delve fields/mismatch oracle.
    for mismatch in [false, true] {
        let mut p = pdpb::SyncRegionResponse {
            start_index: 10,
            ..Default::default()
        };
        for (id, version) in [(1, 5), (2, 4), (3, 6)] {
            p.regions.push(metapb::Region {
                id,
                start_key: vec![0, 1],
                end_key: vec![0, 2],
                region_epoch: Some(metapb::RegionEpoch {
                    version,
                    conf_ver: 1,
                }),
                peers: vec![metapb::Peer {
                    id: id + 1,
                    store_id: 1,
                    ..Default::default()
                }],
                ..Default::default()
            });
        }
        p.region_stats = vec![
            pdpb::RegionStat {
                bytes_written: 101,
                ..Default::default()
            },
            pdpb::RegionStat::default(),
            pdpb::RegionStat {
                bytes_written: 303,
                ..Default::default()
            },
        ];
        p.buckets = vec![
            metapb::Buckets::default(),
            metapb::Buckets::default(),
            metapb::Buckets {
                version: 7,
                ..Default::default()
            },
        ];
        p.region_leaders = vec![metapb::Peer {
            id: if mismatch { 0 } else { 2 },
            store_id: 1,
            ..Default::default()
        }];
        if mismatch {
            p.region_stats.truncate(1);
            p.buckets.truncate(1);
        }
        let (mut client, mut cluster, storage, paths, mut history) = follower();
        let p = roundtrip(p);
        let result = client
            .apply(
                response_from_proto(p).into_apply(),
                &mut cluster,
                &storage,
                &paths,
                &mut history,
            )
            .await
            .unwrap();
        assert_eq!((result.applied, result.stale), (2, 1));
        assert_eq!(history.next_index(), I(12));
        assert_eq!(cluster.regions.get_regions().len(), 1);
        let r = cluster.regions.get_region(3).unwrap();
        assert_eq!(r.stats().written_bytes, if mismatch { 0 } else { 303 });
        assert!(r.leader().is_none());
        if mismatch {
            assert!(r.buckets().is_none());
            assert!(history.get(I(10)).unwrap().leader().is_none());
        } else {
            assert_eq!(r.buckets().unwrap().version, 7);
            assert_eq!(history.get(I(10)).unwrap().leader().unwrap().id, 2);
        }
        assert_eq!(
            storage.load_region(&paths, 3).await.unwrap().unwrap(),
            *r.meta()
        );
    }
}
#[tokio::test]
async fn state_task_50_sync_codec_error_header_and_keepalive() {
    let (mut client, mut cluster, storage, paths, mut history) = follower();
    history.reset_with_index(I(10));
    for code in [0, 2, 999] {
        let p = pdpb::SyncRegionResponse {
            header: Some(pdpb::ResponseHeader {
                cluster_id: 42,
                error: Some(pdpb::Error {
                    r#type: code,
                    message: "stop".into(),
                }),
            }),
            start_index: 999,
            regions: vec![metapb::Region {
                id: 1,
                ..Default::default()
            }],
            ..Default::default()
        };
        let out = client
            .apply(
                response_from_proto(roundtrip(p)).into_apply(),
                &mut cluster,
                &storage,
                &paths,
                &mut history,
            )
            .await
            .unwrap();
        assert!(!out.handled);
        assert_eq!(history.next_index(), I(10));
        assert!(!client.is_running());
        assert_eq!(cluster.regions.get_regions().len(), 0);
    }
    // Go accepts a missing response header, and keepalive establishes readiness.
    let p = pdpb::SyncRegionResponse {
        start_index: 43,
        ..Default::default()
    };
    let out = client
        .apply(
            response_from_proto(roundtrip(p)).into_apply(),
            &mut cluster,
            &storage,
            &paths,
            &mut history,
        )
        .await
        .unwrap();
    assert!(out.handled);
    assert!(client.is_running());
    assert_eq!(history.next_index(), I(43));
}
#[tokio::test]
async fn state_task_50_sync_codec_snapshot_offsets_and_incremental() {
    let mut source = BasicCluster::new();
    let mut source_history = HistoryBuffer::with_capacity_unit(2048, 1).unwrap();
    source_history.reset_with_index(I(1043));
    for id in 1u64..=1001 {
        let r = Arc::new(RegionInfo::new(
            RegionMeta {
                id,
                key_range: KeyRange {
                    start_key: id.to_be_bytes().to_vec(),
                    end_key: (id + 1).to_be_bytes().to_vec(),
                },
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        ));
        source.regions.put_region(r);
    }
    let snapshot = FullSnapshot::bind(&mut source_history, &source);
    let (mut client, mut cluster, storage, paths, mut history) = follower();
    let mut offsets = vec![];
    for batch in snapshot.batches(NonZeroUsize::new(1000).unwrap()) {
        let p = snapshot_to_proto(42, batch);
        offsets.push((p.start_index, p.regions.len()));
        let out = client
            .apply(
                response_from_proto(roundtrip(p)).into_apply(),
                &mut cluster,
                &storage,
                &paths,
                &mut history,
            )
            .await
            .unwrap();
        assert!(out.handled);
        assert!(!client.is_running());
        assert!(client.full_syncing());
        assert_eq!(history.next_index(), I(0));
    }
    assert_eq!(offsets, [(0, 1000), (1000, 1)]);
    let p = catch_up_to_proto(
        42,
        CatchUpFrame::Complete {
            next_index: I(1043),
        },
    );
    client
        .apply(
            response_from_proto(roundtrip(p)).into_apply(),
            &mut cluster,
            &storage,
            &paths,
            &mut history,
        )
        .await
        .unwrap();
    assert!(client.is_running());
    assert!(!client.full_syncing());
    assert_eq!(history.next_index(), I(1043));
    assert_eq!(cluster.regions.get_regions().len(), 1001);
    let r = source.regions.get_region(1).unwrap();
    source_history.record(r).unwrap();
    let HistoryReplay::Incremental(frames) = source_history
        .plan_replay(I(1043), I(1044), NonZeroUsize::new(1000).unwrap())
        .unwrap()
    else {
        panic!("incremental")
    };
    for f in frames {
        let p = history_to_proto(42, &f);
        client
            .apply(
                response_from_proto(roundtrip(p)).into_apply(),
                &mut cluster,
                &storage,
                &paths,
                &mut history,
            )
            .await
            .unwrap();
    }
    assert_eq!(history.next_index(), I(1044));
}
