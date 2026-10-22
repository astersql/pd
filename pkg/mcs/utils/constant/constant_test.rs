// Copyright 2026 AsterSQL.

use std::time::Duration;

use super::*;

#[test]
fn network_and_timing_defaults() {
    assert_eq!(RETRY_INTERVAL, Duration::from_millis(500));
    assert_eq!(TCP_NETWORK_STR, "tcp");
    assert_eq!(DEFAULT_GRPC_GRACEFUL_STOP_TIMEOUT, Duration::from_secs(5));
    assert_eq!(
        DEFAULT_HTTP_GRACEFUL_SHUTDOWN_TIMEOUT,
        Duration::from_secs(5)
    );
    assert_eq!(DEFAULT_LOG_FORMAT, "text");
    assert_eq!(DEFAULT_LOG_LEVEL, "info");
    assert!(DEFAULT_DISABLE_ERROR_VERBOSE);
    assert_eq!(DEFAULT_LEASE, 5);
    assert_eq!(TRANSFER_PRIMARY_LEASE_MULTIPLIER, 3);
    assert_eq!(PRIMARY_TICK_INTERVAL, Duration::from_millis(50));
    assert_eq!(LEADER_TICK_INTERVAL, Duration::from_millis(50));
}

#[test]
fn service_names_and_root() {
    assert_eq!(MICROSERVICE_ROOT_PATH, "/ms");
    assert_eq!(PD_SERVICE_NAME, "pd");
    assert_eq!(TSO_SERVICE_NAME, "tso");
    assert_eq!(RESOURCE_MANAGER_SERVICE_NAME, "resource_manager");
    assert_eq!(SCHEDULING_SERVICE_NAME, "scheduling");
    assert_eq!(ROUTER_SERVICE_NAME, "router");
}

#[test]
fn keyspace_group_limits() {
    let max_group_count: u32 = MAX_KEYSPACE_GROUP_COUNT;
    let max_group_count_in_use: u32 = MAX_KEYSPACE_GROUP_COUNT_IN_USE;

    assert_eq!(max_group_count, 100_000);
    assert_eq!(max_group_count_in_use, 4_096);
    assert!(max_group_count_in_use <= max_group_count);
    assert_eq!(DEFAULT_KEYSPACE_GROUP_REPLICA_COUNT, 2);
    assert_eq!(DEFAULT_KEYSPACE_GROUP_REPLICA_PRIORITY, 0);
}
