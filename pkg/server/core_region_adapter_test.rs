// Copyright 2026 AsterSQL.
use super::core_region_adapter::*;
use astersql_pd_proto::{metapb, pdpb};
#[test]
fn core_task_61_metadata_roles() {
    assert!(region_from_proto(None).is_none());
    for role in [0, 1, 2, 3, -7] {
        let peer = metapb::Peer {
            id: u64::MAX,
            store_id: 9,
            role,
            is_witness: true,
        };
        assert_eq!(peer_to_proto(peer_from_proto(peer)), peer);
    }
    for epoch in [
        None,
        Some(metapb::RegionEpoch {
            conf_ver: u64::MAX,
            version: 2,
        }),
    ] {
        let meta = metapb::Region {
            id: 9,
            start_key: vec![255, 0],
            end_key: vec![],
            region_epoch: epoch,
            encryption_meta: Some(astersql_pd_proto::encryptionpb::EncryptionMeta {
                key_id: 99,
                iv: vec![0, 255],
            }),
            is_in_flashback: true,
            flashback_start_ts: u64::MAX,
            ..Default::default()
        };
        assert_eq!(
            region_to_proto(region_from_proto(Some(meta.clone())).unwrap()),
            meta
        );
    }
}
#[test]
fn core_task_61_sizes() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../core/rust-fixtures/core_task_61.json")).unwrap();
    for c in fixture["sizes"].as_array().unwrap() {
        let r = region_from_heartbeat(
            pdpb::RegionHeartbeatRequest {
                region: Some(metapb::Region::default()),
                approximate_size: c["bytes"].as_u64().unwrap(),
                approximate_kv_size: 100 * 1048576,
                approximate_ia_kv_size: c["ia_bytes"].as_u64().unwrap(),
                approximate_columnar_kv_size: 3145729,
                approximate_keys: u64::MAX,
                ..Default::default()
            },
            10,
        )
        .unwrap();
        assert_eq!(r.stats().approximate_size, c["mib"].as_i64().unwrap());
        assert_eq!(
            r.stats().approximate_ia_kv_size,
            c["ia_mib"].as_i64().unwrap()
        );
        assert_eq!(r.stats().approximate_kv_size, 100);
        assert_eq!(r.stats().approximate_columnar_kv_size, 3);
        assert_eq!(r.stats().approximate_keys, -1);
        assert_eq!(r.source(), astersql_pd_core::RegionSource::Heartbeat);
    }
}
#[test]
fn core_task_61_heartbeat_fields() {
    let peer = metapb::Peer {
        id: 7,
        store_id: 8,
        role: 1,
        is_witness: true,
    };
    let r = region_from_heartbeat(
        pdpb::RegionHeartbeatRequest {
            region: Some(metapb::Region {
                id: 42,
                peers: vec![peer],
                ..Default::default()
            }),
            leader: Some(peer),
            term: 19,
            cpu_usage: 77,
            pending_peers: vec![metapb::Peer { id: 9, ..peer }, peer],
            down_peers: vec![pdpb::PeerStats {
                peer: Some(peer),
                down_seconds: 30,
            }],
            bytes_read: 1 << 50,
            keys_read: 7,
            bytes_written: 20,
            keys_written: 10,
            query_stats: Some(pdpb::QueryStats {
                get: 2,
                put: 3,
                ..Default::default()
            }),
            interval: Some(pdpb::TimeInterval {
                start_timestamp: 10,
                end_timestamp: 15,
            }),
            cpu_stats: Some(pdpb::CpuStats {
                unified_read: 13,
                scheduler: 14,
            }),
            bucket_meta: Some(metapb::BucketMeta {
                version: 5,
                keys: vec![vec![0], vec![255]],
            }),
            replication_status: Some(
                astersql_pd_proto::replication_modepb::RegionReplicationStatus {
                    state: 2,
                    state_id: 99,
                },
            ),
            ..Default::default()
        },
        10,
    )
    .unwrap();
    assert_eq!(r.term(), 19);
    assert_eq!(r.response_cpu_usage(), 77);
    assert!(r.has_replication_status());
    assert_eq!(r.learners().len(), 1);
    assert_eq!(r.witnesses().len(), 1);
    assert_eq!(r.leader().unwrap().id, 7);
    assert_eq!(r.pending_peers()[0].id, 7);
    assert_eq!(r.down_peers()[0].down_seconds, 30);
    assert_eq!(r.buckets().unwrap().region_id, 42);
    assert_eq!(r.buckets().unwrap().version, 5);
    assert_eq!(r.buckets().unwrap().keys, vec![vec![0], vec![255]]);
    assert_eq!(r.replication_status(), (2, 99));
    assert_eq!(r.loads(), [0., 0., 2., 20., 10., 3., 13., 0.]);
    assert_eq!(r.write_rate(), (4., 2.));
}
#[test]
fn core_task_61_missing_fields() {
    assert!(region_from_heartbeat(Default::default(), 0).is_err());
    assert!(
        region_from_heartbeat(
            pdpb::RegionHeartbeatRequest {
                region: Some(Default::default()),
                down_peers: vec![pdpb::PeerStats::default()],
                ..Default::default()
            },
            0
        )
        .is_err()
    );
    let r = region_from_heartbeat(
        pdpb::RegionHeartbeatRequest {
            region: Some(Default::default()),
            keys_written: 1 << 50,
            bytes_written: 3,
            bytes_read: (1 << 50) - 1,
            ..Default::default()
        },
        0,
    )
    .unwrap();
    assert_eq!(r.stats().written_bytes, 0);
    assert_eq!(r.stats().read_bytes, (1 << 50) - 1);
    assert!(r.leader().is_none());
    assert!(r.buckets().is_none());
    assert_eq!(r.loads()[2], 0.);
}
