// Copyright 2026 AsterSQL.
//! Lossless known-field conversions at the protobuf boundary. Values are not
//! validated or normalized; optional messages retain their presence.
use astersql_pd_core as core;
use astersql_pd_proto::{apipb, metapb, pdpb};

trait Convert<T> {
    fn convert(self) -> T;
}

/// Convert optional store metadata, preserving absence and unknown enums.
pub fn store_from_proto(value: Option<metapb::Store>) -> Option<core::StoreMeta> {
    value.map(Convert::convert)
}
/// Convert owned metadata back to the protocol representation.
pub fn store_to_proto(value: core::StoreMeta) -> metapb::Store {
    value.convert()
}
/// Convert an optional raw heartbeat report without applying smoothing.
pub fn store_stats_from_proto(value: Option<pdpb::StoreStats>) -> Option<core::StoreStats> {
    value.map(Convert::convert)
}
/// Convert an owned raw report back to the protocol representation.
pub fn store_stats_to_proto(value: core::StoreStats) -> pdpb::StoreStats {
    value.convert()
}

impl Convert<core::StoreStats> for pdpb::StoreStats {
    fn convert(self) -> core::StoreStats {
        let value = self;
        core::StoreStats {
            store_id: value.store_id,
            capacity: value.capacity,
            available: value.available,
            region_count: value.region_count,
            sending_snap_count: value.sending_snap_count,
            receiving_snap_count: value.receiving_snap_count,
            start_time: value.start_time,
            applying_snap_count: value.applying_snap_count,
            is_busy: value.is_busy,
            used_size: value.used_size,
            bytes_written: value.bytes_written,
            keys_written: value.keys_written,
            bytes_read: value.bytes_read,
            keys_read: value.keys_read,
            interval: value.interval.map(Convert::convert),
            cpu_usages: value.cpu_usages.into_iter().map(Convert::convert).collect(),
            read_io_rates: value
                .read_io_rates
                .into_iter()
                .map(Convert::convert)
                .collect(),
            write_io_rates: value
                .write_io_rates
                .into_iter()
                .map(Convert::convert)
                .collect(),
            op_latencies: value
                .op_latencies
                .into_iter()
                .map(Convert::convert)
                .collect(),
            peer_stats: value.peer_stats.into_iter().map(Convert::convert).collect(),
            query_stats: value.query_stats.map(Convert::convert),
            slow_score: value.slow_score,
            damaged_regions_id: value.damaged_regions_id,
            is_apply_busy: value.is_apply_busy,
            snapshot_stats: value
                .snapshot_stats
                .into_iter()
                .map(Convert::convert)
                .collect(),
            slow_trend: value.slow_trend.map(Convert::convert),
            is_grpc_paused: value.is_grpc_paused,
            total_memory: value.total_memory,
            used_memory: value.used_memory,
            network_slow_scores: value.network_slow_scores,
            dfs: value.dfs.into_iter().map(Convert::convert).collect(),
            is_stopping: value.is_stopping,
        }
    }
}

impl Convert<pdpb::StoreStats> for core::StoreStats {
    fn convert(self) -> pdpb::StoreStats {
        let value = self;
        pdpb::StoreStats {
            store_id: value.store_id,
            capacity: value.capacity,
            available: value.available,
            region_count: value.region_count,
            sending_snap_count: value.sending_snap_count,
            receiving_snap_count: value.receiving_snap_count,
            start_time: value.start_time,
            applying_snap_count: value.applying_snap_count,
            is_busy: value.is_busy,
            used_size: value.used_size,
            bytes_written: value.bytes_written,
            keys_written: value.keys_written,
            bytes_read: value.bytes_read,
            keys_read: value.keys_read,
            interval: value.interval.map(Convert::convert),
            cpu_usages: value.cpu_usages.into_iter().map(Convert::convert).collect(),
            read_io_rates: value
                .read_io_rates
                .into_iter()
                .map(Convert::convert)
                .collect(),
            write_io_rates: value
                .write_io_rates
                .into_iter()
                .map(Convert::convert)
                .collect(),
            op_latencies: value
                .op_latencies
                .into_iter()
                .map(Convert::convert)
                .collect(),
            peer_stats: value.peer_stats.into_iter().map(Convert::convert).collect(),
            query_stats: value.query_stats.map(Convert::convert),
            slow_score: value.slow_score,
            damaged_regions_id: value.damaged_regions_id,
            is_apply_busy: value.is_apply_busy,
            snapshot_stats: value
                .snapshot_stats
                .into_iter()
                .map(Convert::convert)
                .collect(),
            slow_trend: value.slow_trend.map(Convert::convert),
            is_grpc_paused: value.is_grpc_paused,
            total_memory: value.total_memory,
            used_memory: value.used_memory,
            network_slow_scores: value.network_slow_scores,
            dfs: value.dfs.into_iter().map(Convert::convert).collect(),
            is_stopping: value.is_stopping,
        }
    }
}

impl Convert<core::TimeInterval> for pdpb::TimeInterval {
    fn convert(self) -> core::TimeInterval {
        let value = self;
        core::TimeInterval {
            start_timestamp: value.start_timestamp,
            end_timestamp: value.end_timestamp,
        }
    }
}

impl Convert<pdpb::TimeInterval> for core::TimeInterval {
    fn convert(self) -> pdpb::TimeInterval {
        let value = self;
        pdpb::TimeInterval {
            start_timestamp: value.start_timestamp,
            end_timestamp: value.end_timestamp,
        }
    }
}

impl Convert<core::RecordPair> for pdpb::RecordPair {
    fn convert(self) -> core::RecordPair {
        let value = self;
        core::RecordPair {
            key: value.key,
            value: value.value,
        }
    }
}

impl Convert<pdpb::RecordPair> for core::RecordPair {
    fn convert(self) -> pdpb::RecordPair {
        let value = self;
        pdpb::RecordPair {
            key: value.key,
            value: value.value,
        }
    }
}

impl Convert<core::PeerStat> for pdpb::PeerStat {
    fn convert(self) -> core::PeerStat {
        let value = self;
        core::PeerStat {
            region_id: value.region_id,
            read_keys: value.read_keys,
            read_bytes: value.read_bytes,
            query_stats: value.query_stats.map(Convert::convert),
            written_keys: value.written_keys,
            written_bytes: value.written_bytes,
            cpu_stats: value.cpu_stats.map(Convert::convert),
        }
    }
}

impl Convert<pdpb::PeerStat> for core::PeerStat {
    fn convert(self) -> pdpb::PeerStat {
        let value = self;
        pdpb::PeerStat {
            region_id: value.region_id,
            read_keys: value.read_keys,
            read_bytes: value.read_bytes,
            query_stats: value.query_stats.map(Convert::convert),
            written_keys: value.written_keys,
            written_bytes: value.written_bytes,
            cpu_stats: value.cpu_stats.map(Convert::convert),
        }
    }
}

impl Convert<core::QueryStats> for pdpb::QueryStats {
    fn convert(self) -> core::QueryStats {
        let value = self;
        core::QueryStats {
            gc: value.gc,
            get: value.get,
            scan: value.scan,
            coprocessor: value.coprocessor,
            delete: value.delete,
            delete_range: value.delete_range,
            put: value.put,
            prewrite: value.prewrite,
            acquire_pessimistic_lock: value.acquire_pessimistic_lock,
            commit: value.commit,
            rollback: value.rollback,
        }
    }
}

impl Convert<pdpb::QueryStats> for core::QueryStats {
    fn convert(self) -> pdpb::QueryStats {
        let value = self;
        pdpb::QueryStats {
            gc: value.gc,
            get: value.get,
            scan: value.scan,
            coprocessor: value.coprocessor,
            delete: value.delete,
            delete_range: value.delete_range,
            put: value.put,
            prewrite: value.prewrite,
            acquire_pessimistic_lock: value.acquire_pessimistic_lock,
            commit: value.commit,
            rollback: value.rollback,
        }
    }
}

impl Convert<core::CpuStats> for pdpb::CpuStats {
    fn convert(self) -> core::CpuStats {
        let value = self;
        core::CpuStats {
            unified_read: value.unified_read,
            scheduler: value.scheduler,
        }
    }
}

impl Convert<pdpb::CpuStats> for core::CpuStats {
    fn convert(self) -> pdpb::CpuStats {
        let value = self;
        pdpb::CpuStats {
            unified_read: value.unified_read,
            scheduler: value.scheduler,
        }
    }
}

impl Convert<core::DfsStatScope> for pdpb::DfsStatScope {
    fn convert(self) -> core::DfsStatScope {
        let value = self;
        core::DfsStatScope {
            is_global: value.is_global,
            keyspace_id: value.keyspace_id,
            component: value.component,
            keyspace_identities: value
                .keyspace_identities
                .into_iter()
                .map(Convert::convert)
                .collect(),
        }
    }
}

impl Convert<pdpb::DfsStatScope> for core::DfsStatScope {
    fn convert(self) -> pdpb::DfsStatScope {
        let value = self;
        pdpb::DfsStatScope {
            is_global: value.is_global,
            keyspace_id: value.keyspace_id,
            component: value.component,
            keyspace_identities: value
                .keyspace_identities
                .into_iter()
                .map(Convert::convert)
                .collect(),
        }
    }
}

impl Convert<core::DfsStatItem> for pdpb::DfsStatItem {
    fn convert(self) -> core::DfsStatItem {
        let value = self;
        core::DfsStatItem {
            scope: value.scope.map(Convert::convert),
            written_bytes: value.written_bytes,
            write_requests: value.write_requests,
        }
    }
}

impl Convert<pdpb::DfsStatItem> for core::DfsStatItem {
    fn convert(self) -> pdpb::DfsStatItem {
        let value = self;
        pdpb::DfsStatItem {
            scope: value.scope.map(Convert::convert),
            written_bytes: value.written_bytes,
            write_requests: value.write_requests,
        }
    }
}

impl Convert<core::SlowTrend> for pdpb::SlowTrend {
    fn convert(self) -> core::SlowTrend {
        let value = self;
        core::SlowTrend {
            cause_value: value.cause_value,
            cause_rate: value.cause_rate,
            result_value: value.result_value,
            result_rate: value.result_rate,
        }
    }
}

impl Convert<pdpb::SlowTrend> for core::SlowTrend {
    fn convert(self) -> pdpb::SlowTrend {
        let value = self;
        pdpb::SlowTrend {
            cause_value: value.cause_value,
            cause_rate: value.cause_rate,
            result_value: value.result_value,
            result_rate: value.result_rate,
        }
    }
}

impl Convert<core::SnapshotStat> for pdpb::SnapshotStat {
    fn convert(self) -> core::SnapshotStat {
        let value = self;
        core::SnapshotStat {
            region_id: value.region_id,
            generate_duration_sec: value.generate_duration_sec,
            send_duration_sec: value.send_duration_sec,
            total_duration_sec: value.total_duration_sec,
            transport_size: value.transport_size,
        }
    }
}

impl Convert<pdpb::SnapshotStat> for core::SnapshotStat {
    fn convert(self) -> pdpb::SnapshotStat {
        let value = self;
        pdpb::SnapshotStat {
            region_id: value.region_id,
            generate_duration_sec: value.generate_duration_sec,
            send_duration_sec: value.send_duration_sec,
            total_duration_sec: value.total_duration_sec,
            transport_size: value.transport_size,
        }
    }
}

impl Convert<core::StoreMeta> for metapb::Store {
    fn convert(self) -> core::StoreMeta {
        let value = self;
        core::StoreMeta {
            id: value.id,
            address: value.address,
            state: match value.state {
                0 => core::StoreState::Up,
                1 => core::StoreState::Offline,
                2 => core::StoreState::Tombstone,
                n => core::StoreState::Unknown(n),
            },
            labels: value.labels.into_iter().map(Convert::convert).collect(),
            version: value.version,
            peer_address: value.peer_address,
            status_address: value.status_address,
            git_hash: value.git_hash,
            start_timestamp: value.start_timestamp,
            deploy_path: value.deploy_path,
            last_heartbeat: value.last_heartbeat,
            physically_destroyed: value.physically_destroyed,
            node_state: match value.node_state {
                0 => core::NodeState::Preparing,
                1 => core::NodeState::Serving,
                2 => core::NodeState::Removing,
                3 => core::NodeState::Removed,
                n => core::NodeState::Unknown(n),
            },
            txn_protocol_version_range: value.txn_protocol_version_range.map(Convert::convert),
        }
    }
}

impl Convert<metapb::Store> for core::StoreMeta {
    fn convert(self) -> metapb::Store {
        let value = self;
        metapb::Store {
            id: value.id,
            address: value.address,
            state: match value.state {
                core::StoreState::Up => 0,
                core::StoreState::Offline => 1,
                core::StoreState::Tombstone => 2,
                core::StoreState::Unknown(n) => n,
            },
            labels: value.labels.into_iter().map(Convert::convert).collect(),
            version: value.version,
            peer_address: value.peer_address,
            status_address: value.status_address,
            git_hash: value.git_hash,
            start_timestamp: value.start_timestamp,
            deploy_path: value.deploy_path,
            last_heartbeat: value.last_heartbeat,
            physically_destroyed: value.physically_destroyed,
            node_state: match value.node_state {
                core::NodeState::Preparing => 0,
                core::NodeState::Serving => 1,
                core::NodeState::Removing => 2,
                core::NodeState::Removed => 3,
                core::NodeState::Unknown(n) => n,
            },
            txn_protocol_version_range: value.txn_protocol_version_range.map(Convert::convert),
        }
    }
}

impl Convert<core::StoreLabel> for metapb::StoreLabel {
    fn convert(self) -> core::StoreLabel {
        let value = self;
        core::StoreLabel {
            key: value.key,
            value: value.value,
        }
    }
}

impl Convert<metapb::StoreLabel> for core::StoreLabel {
    fn convert(self) -> metapb::StoreLabel {
        let value = self;
        metapb::StoreLabel {
            key: value.key,
            value: value.value,
        }
    }
}

impl Convert<core::TxnProtocolVersionRange> for metapb::TxnProtocolVersionRange {
    fn convert(self) -> core::TxnProtocolVersionRange {
        let value = self;
        core::TxnProtocolVersionRange {
            min: value.min,
            max: value.max,
        }
    }
}

impl Convert<metapb::TxnProtocolVersionRange> for core::TxnProtocolVersionRange {
    fn convert(self) -> metapb::TxnProtocolVersionRange {
        let value = self;
        metapb::TxnProtocolVersionRange {
            min: value.min,
            max: value.max,
        }
    }
}

impl Convert<core::KeyspaceIdentity> for apipb::KeyspaceIdentity {
    fn convert(self) -> core::KeyspaceIdentity {
        let value = self;
        core::KeyspaceIdentity {
            namespace_id: value.namespace_id,
            keyspace_id: value.keyspace_id,
        }
    }
}

impl Convert<apipb::KeyspaceIdentity> for core::KeyspaceIdentity {
    fn convert(self) -> apipb::KeyspaceIdentity {
        let value = self;
        apipb::KeyspaceIdentity {
            namespace_id: value.namespace_id,
            keyspace_id: value.keyspace_id,
        }
    }
}
