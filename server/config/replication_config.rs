// Copyright 2026 AsterSQL.
//! Compatibility exports of the canonical replication configuration.
pub use astersql_pd_replication::{
    DRAutoSyncReplicationConfig, ReplicationModeConfig, decode_toml_replication_mode,
    normalize_replication_mode,
};
