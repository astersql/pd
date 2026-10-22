// Copyright 2026 AsterSQL.
use super::core_store_adapter::*;
use astersql_pd_core as core;
use astersql_pd_proto::{apipb, metapb, pdpb};
#[test]
fn core_task_60_defaults_and_presence() {
    assert_eq!(store_from_proto(None), None);
    assert_eq!(store_stats_from_proto(None), None);
    let store = store_from_proto(Some(metapb::Store::default())).unwrap();
    assert_eq!(store, core::StoreMeta::default());
    assert_eq!(store_to_proto(store), metapb::Store::default());
    let stats = store_stats_from_proto(Some(pdpb::StoreStats::default())).unwrap();
    assert_eq!(stats, core::StoreStats::default());
    assert_eq!(store_stats_to_proto(stats), pdpb::StoreStats::default());
}
#[test]
fn core_task_60_unknown_enums_and_signed_extremes() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../core/rust-fixtures/core_task_60.json")).unwrap();
    for case in fixture["states"].as_array().unwrap() {
        let state = case[0].as_i64().unwrap() as i32;
        let node_state = case[1].as_i64().unwrap() as i32;
        let proto = metapb::Store {
            state,
            node_state,
            start_timestamp: i64::MIN,
            last_heartbeat: i64::MAX,
            txn_protocol_version_range: Some(metapb::TxnProtocolVersionRange { min: 9, max: 0 }),
            ..Default::default()
        };
        let domain = store_from_proto(Some(proto.clone())).unwrap();
        assert_eq!(
            domain.state,
            match state {
                0 => core::StoreState::Up,
                1 => core::StoreState::Offline,
                2 => core::StoreState::Tombstone,
                n => core::StoreState::Unknown(n),
            }
        );
        assert_eq!(
            domain.node_state,
            match node_state {
                0 => core::NodeState::Preparing,
                1 => core::NodeState::Serving,
                2 => core::NodeState::Removing,
                3 => core::NodeState::Removed,
                n => core::NodeState::Unknown(n),
            }
        );
        assert_eq!(store_to_proto(domain), proto);
    }
}

#[test]
fn core_task_60_full_store() {
    let proto = metapb::Store {
        id: 1,
        address: "field-2".into(),
        state: -73,
        labels: vec![metapb::StoreLabel {
            key: "field-1".into(),
            value: "field-2".into(),
        }],
        version: "field-5".into(),
        peer_address: "field-6".into(),
        status_address: "field-7".into(),
        git_hash: "field-8".into(),
        start_timestamp: 9,
        deploy_path: "field-10".into(),
        last_heartbeat: 11,
        physically_destroyed: true,
        node_state: -73,
        txn_protocol_version_range: Some(metapb::TxnProtocolVersionRange { min: 1, max: 2 }),
    };
    let domain = store_from_proto(Some(proto.clone())).unwrap();
    assert_eq!(
        domain,
        core::StoreMeta {
            id: 1,
            address: "field-2".into(),
            state: core::StoreState::Unknown(-73),
            labels: vec![core::StoreLabel {
                key: "field-1".into(),
                value: "field-2".into(),
            }],
            version: "field-5".into(),
            peer_address: "field-6".into(),
            status_address: "field-7".into(),
            git_hash: "field-8".into(),
            start_timestamp: 9,
            deploy_path: "field-10".into(),
            last_heartbeat: 11,
            physically_destroyed: true,
            node_state: core::NodeState::Unknown(-73),
            txn_protocol_version_range: Some(core::TxnProtocolVersionRange { min: 1, max: 2 }),
        }
    );
    assert_eq!(domain.id, proto.id);
    assert_eq!(domain.address, proto.address);
    assert_eq!(domain.version, proto.version);
    assert_eq!(domain.peer_address, proto.peer_address);
    assert_eq!(domain.status_address, proto.status_address);
    assert_eq!(domain.git_hash, proto.git_hash);
    assert_eq!(domain.start_timestamp, proto.start_timestamp);
    assert_eq!(domain.deploy_path, proto.deploy_path);
    assert_eq!(domain.last_heartbeat, proto.last_heartbeat);
    assert_eq!(domain.physically_destroyed, proto.physically_destroyed);
    assert_eq!(store_to_proto(domain), proto);
}

#[test]
fn core_task_60_full_store_stats() {
    let proto = pdpb::StoreStats {
        store_id: 1,
        capacity: 2,
        available: 3,
        region_count: 4,
        sending_snap_count: 5,
        receiving_snap_count: 6,
        start_time: 7,
        applying_snap_count: 8,
        is_busy: true,
        used_size: 10,
        bytes_written: 11,
        keys_written: 12,
        bytes_read: 13,
        keys_read: 14,
        interval: Some(pdpb::TimeInterval {
            start_timestamp: 1,
            end_timestamp: 2,
        }),
        cpu_usages: vec![pdpb::RecordPair {
            key: "field-1".into(),
            value: 2,
        }],
        read_io_rates: vec![pdpb::RecordPair {
            key: "field-1".into(),
            value: 2,
        }],
        write_io_rates: vec![pdpb::RecordPair {
            key: "field-1".into(),
            value: 2,
        }],
        op_latencies: vec![pdpb::RecordPair {
            key: "field-1".into(),
            value: 2,
        }],
        peer_stats: vec![pdpb::PeerStat {
            region_id: 1,
            read_keys: 2,
            read_bytes: 3,
            query_stats: Some(pdpb::QueryStats {
                gc: 1,
                get: 2,
                scan: 3,
                coprocessor: 4,
                delete: 5,
                delete_range: 6,
                put: 7,
                prewrite: 8,
                acquire_pessimistic_lock: 9,
                commit: 10,
                rollback: 11,
            }),
            written_keys: 5,
            written_bytes: 6,
            cpu_stats: Some(pdpb::CpuStats {
                unified_read: 1,
                scheduler: 2,
            }),
        }],
        query_stats: Some(pdpb::QueryStats {
            gc: 1,
            get: 2,
            scan: 3,
            coprocessor: 4,
            delete: 5,
            delete_range: 6,
            put: 7,
            prewrite: 8,
            acquire_pessimistic_lock: 9,
            commit: 10,
            rollback: 11,
        }),
        slow_score: 22,
        damaged_regions_id: vec![23],
        is_apply_busy: true,
        snapshot_stats: vec![pdpb::SnapshotStat {
            region_id: 1,
            generate_duration_sec: 2,
            send_duration_sec: 3,
            total_duration_sec: 4,
            transport_size: 5,
        }],
        slow_trend: Some(pdpb::SlowTrend {
            cause_value: 1.25,
            cause_rate: 2.25,
            result_value: 3.25,
            result_rate: 4.25,
        }),
        is_grpc_paused: true,
        total_memory: 28,
        used_memory: 29,
        network_slow_scores: [(19, 30)].into(),
        dfs: vec![pdpb::DfsStatItem {
            scope: Some(pdpb::DfsStatScope {
                is_global: true,
                keyspace_id: 2,
                component: "field-3".into(),
                keyspace_identities: vec![apipb::KeyspaceIdentity {
                    namespace_id: 1,
                    keyspace_id: 2,
                }],
            }),
            written_bytes: 2,
            write_requests: 3,
        }],
        is_stopping: true,
    };
    let domain = store_stats_from_proto(Some(proto.clone())).unwrap();
    assert_eq!(
        domain,
        core::StoreStats {
            store_id: 1,
            capacity: 2,
            available: 3,
            region_count: 4,
            sending_snap_count: 5,
            receiving_snap_count: 6,
            start_time: 7,
            applying_snap_count: 8,
            is_busy: true,
            used_size: 10,
            bytes_written: 11,
            keys_written: 12,
            bytes_read: 13,
            keys_read: 14,
            interval: Some(core::TimeInterval {
                start_timestamp: 1,
                end_timestamp: 2,
            }),
            cpu_usages: vec![core::RecordPair {
                key: "field-1".into(),
                value: 2,
            }],
            read_io_rates: vec![core::RecordPair {
                key: "field-1".into(),
                value: 2,
            }],
            write_io_rates: vec![core::RecordPair {
                key: "field-1".into(),
                value: 2,
            }],
            op_latencies: vec![core::RecordPair {
                key: "field-1".into(),
                value: 2,
            }],
            peer_stats: vec![core::PeerStat {
                region_id: 1,
                read_keys: 2,
                read_bytes: 3,
                query_stats: Some(core::QueryStats {
                    gc: 1,
                    get: 2,
                    scan: 3,
                    coprocessor: 4,
                    delete: 5,
                    delete_range: 6,
                    put: 7,
                    prewrite: 8,
                    acquire_pessimistic_lock: 9,
                    commit: 10,
                    rollback: 11,
                }),
                written_keys: 5,
                written_bytes: 6,
                cpu_stats: Some(core::CpuStats {
                    unified_read: 1,
                    scheduler: 2,
                }),
            }],
            query_stats: Some(core::QueryStats {
                gc: 1,
                get: 2,
                scan: 3,
                coprocessor: 4,
                delete: 5,
                delete_range: 6,
                put: 7,
                prewrite: 8,
                acquire_pessimistic_lock: 9,
                commit: 10,
                rollback: 11,
            }),
            slow_score: 22,
            damaged_regions_id: vec![23],
            is_apply_busy: true,
            snapshot_stats: vec![core::SnapshotStat {
                region_id: 1,
                generate_duration_sec: 2,
                send_duration_sec: 3,
                total_duration_sec: 4,
                transport_size: 5,
            }],
            slow_trend: Some(core::SlowTrend {
                cause_value: 1.25,
                cause_rate: 2.25,
                result_value: 3.25,
                result_rate: 4.25,
            }),
            is_grpc_paused: true,
            total_memory: 28,
            used_memory: 29,
            network_slow_scores: [(19, 30)].into(),
            dfs: vec![core::DfsStatItem {
                scope: Some(core::DfsStatScope {
                    is_global: true,
                    keyspace_id: 2,
                    component: "field-3".into(),
                    keyspace_identities: vec![core::KeyspaceIdentity {
                        namespace_id: 1,
                        keyspace_id: 2,
                    }],
                }),
                written_bytes: 2,
                write_requests: 3,
            }],
            is_stopping: true,
        }
    );
    assert_eq!(domain.store_id, proto.store_id);
    assert_eq!(domain.capacity, proto.capacity);
    assert_eq!(domain.available, proto.available);
    assert_eq!(domain.region_count, proto.region_count);
    assert_eq!(domain.sending_snap_count, proto.sending_snap_count);
    assert_eq!(domain.receiving_snap_count, proto.receiving_snap_count);
    assert_eq!(domain.start_time, proto.start_time);
    assert_eq!(domain.applying_snap_count, proto.applying_snap_count);
    assert_eq!(domain.is_busy, proto.is_busy);
    assert_eq!(domain.used_size, proto.used_size);
    assert_eq!(domain.bytes_written, proto.bytes_written);
    assert_eq!(domain.keys_written, proto.keys_written);
    assert_eq!(domain.bytes_read, proto.bytes_read);
    assert_eq!(domain.keys_read, proto.keys_read);
    assert_eq!(domain.slow_score, proto.slow_score);
    assert_eq!(domain.is_apply_busy, proto.is_apply_busy);
    assert_eq!(domain.is_grpc_paused, proto.is_grpc_paused);
    assert_eq!(domain.total_memory, proto.total_memory);
    assert_eq!(domain.used_memory, proto.used_memory);
    assert_eq!(domain.network_slow_scores, proto.network_slow_scores);
    assert_eq!(domain.is_stopping, proto.is_stopping);
    assert_eq!(store_stats_to_proto(domain), proto);
}

#[test]
fn core_task_60_nested_presence_order_and_unsigned_limits() {
    let proto = pdpb::StoreStats {
        store_id: u64::MAX,
        capacity: 0,
        available: u64::MAX,
        used_size: u64::MAX,
        region_count: u32::MAX,
        interval: Some(pdpb::TimeInterval::default()),
        query_stats: Some(pdpb::QueryStats::default()),
        peer_stats: vec![
            pdpb::PeerStat::default(),
            pdpb::PeerStat {
                region_id: u64::MAX,
                query_stats: Some(pdpb::QueryStats::default()),
                cpu_stats: Some(pdpb::CpuStats::default()),
                ..Default::default()
            },
        ],
        cpu_usages: vec![
            pdpb::RecordPair {
                key: "dup".into(),
                value: 9,
            },
            pdpb::RecordPair {
                key: "dup".into(),
                value: 0,
            },
        ],
        dfs: vec![
            pdpb::DfsStatItem::default(),
            pdpb::DfsStatItem {
                scope: Some(pdpb::DfsStatScope::default()),
                written_bytes: u64::MAX,
                write_requests: 0,
            },
        ],
        damaged_regions_id: vec![u64::MAX, 0, u64::MAX],
        slow_trend: Some(pdpb::SlowTrend {
            cause_value: f64::INFINITY,
            cause_rate: -0.0,
            result_value: f64::NEG_INFINITY,
            result_rate: 0.0,
        }),
        ..Default::default()
    };
    let domain = store_stats_from_proto(Some(proto.clone())).unwrap();
    assert_eq!(domain.available, u64::MAX);
    assert_eq!(domain.region_count, u32::MAX);
    assert_eq!(domain.interval, Some(core::TimeInterval::default()));
    assert_eq!(domain.query_stats, Some(core::QueryStats::default()));
    assert!(domain.peer_stats[0].cpu_stats.is_none());
    assert_eq!(
        domain.peer_stats[1].cpu_stats,
        Some(core::CpuStats::default())
    );
    assert!(domain.dfs[0].scope.is_none());
    assert_eq!(domain.dfs[1].scope, Some(core::DfsStatScope::default()));
    assert_eq!(
        domain
            .cpu_usages
            .iter()
            .map(|p| p.value)
            .collect::<Vec<_>>(),
        vec![9, 0]
    );
    assert_eq!(
        domain.slow_trend.as_ref().unwrap().cause_rate.to_bits(),
        (-0.0f64).to_bits()
    );
    assert_eq!(store_stats_to_proto(domain), proto);
    let meta = core::StoreMeta {
        txn_protocol_version_range: Some(core::TxnProtocolVersionRange::default()),
        ..Default::default()
    };
    assert_eq!(
        store_to_proto(meta).txn_protocol_version_range,
        Some(metapb::TxnProtocolVersionRange::default())
    );
}
