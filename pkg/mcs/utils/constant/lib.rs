// Copyright 2026 AsterSQL.

use std::time::Duration;

/// Interval between retries.
pub const RETRY_INTERVAL: Duration = Duration::from_millis(500);

/// Network name used for TCP listeners and connections.
pub const TCP_NETWORK_STR: &str = "tcp";

/// Maximum time to wait for a gRPC server to stop gracefully.
pub const DEFAULT_GRPC_GRACEFUL_STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum time to wait for an HTTP server to shut down gracefully.
pub const DEFAULT_HTTP_GRACEFUL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// Default log output format.
pub const DEFAULT_LOG_FORMAT: &str = "text";

/// Default log level.
pub const DEFAULT_LOG_LEVEL: &str = "info";

/// Whether verbose error output is disabled by default.
pub const DEFAULT_DISABLE_ERROR_VERBOSE: bool = true;

/// Default leadership lease in seconds.
pub const DEFAULT_LEASE: i64 = 5;

/// Multiplier used to derive the transfer-primary flag TTL from the lease.
pub const TRANSFER_PRIMARY_LEASE_MULTIPLIER: i64 = 3;

/// Interval between primary checks.
pub const PRIMARY_TICK_INTERVAL: Duration = Duration::from_millis(50);

/// Interval between leader checks.
pub const LEADER_TICK_INTERVAL: Duration = Duration::from_millis(50);

/// Root path for microservice data in etcd.
pub const MICROSERVICE_ROOT_PATH: &str = "/ms";

/// Service name of the PD server.
pub const PD_SERVICE_NAME: &str = "pd";

/// Service name of the TSO server.
pub const TSO_SERVICE_NAME: &str = "tso";

/// Service name of the resource manager server.
pub const RESOURCE_MANAGER_SERVICE_NAME: &str = "resource_manager";

/// Service name of the scheduling server.
pub const SCHEDULING_SERVICE_NAME: &str = "scheduling";

/// Service name of the router server.
pub const ROUTER_SERVICE_NAME: &str = "router";

/// Maximum number of keyspace groups supported by the storage layout.
pub const MAX_KEYSPACE_GROUP_COUNT: u32 = 100_000;

/// Practical maximum number of keyspace groups in use.
pub const MAX_KEYSPACE_GROUP_COUNT_IN_USE: u32 = 4_096;

/// Default number of replicas assigned to a keyspace group.
pub const DEFAULT_KEYSPACE_GROUP_REPLICA_COUNT: usize = 2;

/// Default weighted-election priority for a keyspace group replica.
pub const DEFAULT_KEYSPACE_GROUP_REPLICA_PRIORITY: i32 = 0;

#[cfg(test)]
mod constant_test;
