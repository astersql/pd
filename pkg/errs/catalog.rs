// Copyright 2026 AsterSQL.

//! Stable foundation error definitions from Go errno.go at
//! 42a0da684cf28f16a48c74c54575b711031f3c5d. Transport and logging adapters
//! intentionally remain outside this domain catalog.

use crate::ErrorTemplate;

/// Failed to get the source store.
pub const ERR_GET_SOURCE_STORE: ErrorTemplate = ErrorTemplate::new(
    "PD:common:ErrGetSourceStore",
    "failed to get the source store",
);

/// Failed to get the target store.
pub const ERR_GET_TARGET_STORE: ErrorTemplate = ErrorTemplate::new(
    "PD:common:ErrGetTargetStore",
    "failed to get the target store",
);

/// Incorrect system time.
pub const ERR_INCORRECT_SYSTEM_TIME: ErrorTemplate =
    ErrorTemplate::new("PD:common:ErrIncorrectSystemTime", "incorrect system time");

/// Invalid argument for %s: %v.
pub const ERR_INVALID_ARGUMENT: ErrorTemplate = ErrorTemplate::new(
    "PD:common:ErrInvalidArgument",
    "invalid argument for %s: %v",
);

/// Get allocator failed, %s.
pub const ERR_GET_ALLOCATOR: ErrorTemplate =
    ErrorTemplate::new("PD:tso:ErrGetAllocator", "get allocator failed, %s");

/// Reset user timestamp failed, %s.
pub const ERR_RESET_USER_TIMESTAMP: ErrorTemplate = ErrorTemplate::new(
    "PD:tso:ErrResetUserTimestamp",
    "reset user timestamp failed, %s",
);

/// Generate timestamp failed, %s.
pub const ERR_GENERATE_TIMESTAMP: ErrorTemplate = ErrorTemplate::new(
    "PD:tso:ErrGenerateTimestamp",
    "generate timestamp failed, %s",
);

/// Update timestamp failed, %s.
pub const ERR_UPDATE_TIMESTAMP: ErrorTemplate =
    ErrorTemplate::new("PD:tso:ErrUpdateTimestamp", "update timestamp failed, %s");

/// Save timestamp failed, %s.
pub const ERR_SAVE_TIMESTAMP: ErrorTemplate =
    ErrorTemplate::new("PD:tso:ErrSaveTimestamp", "save timestamp failed, %s");

/// Logic part overflow.
pub const ERR_LOGIC_OVERFLOW: ErrorTemplate =
    ErrorTemplate::new("PD:tso:ErrLogicOverflow", "logic part overflow");

/// Proxy tso timeout.
pub const ERR_PROXY_TSO_TIMEOUT: ErrorTemplate =
    ErrorTemplate::new("PD:tso:ErrProxyTSOTimeout", "proxy tso timeout");

/// The keyspace group id is invalid, %s.
pub const ERR_KEYSPACE_GROUP_ID_INVALID: ErrorTemplate = ErrorTemplate::new(
    "PD:tso:ErrKeyspaceGroupIDInvalid",
    "the keyspace group id is invalid, %s",
);

/// Load keyspace groups timeout.
pub const ERR_LOAD_KEYSPACE_GROUPS_TIMEOUT: ErrorTemplate = ErrorTemplate::new(
    "PD:tso:ErrLoadKeyspaceGroupsTimeout",
    "load keyspace groups timeout",
);

/// Load keyspace groups terminated.
pub const ERR_LOAD_KEYSPACE_GROUPS_TERMINATED: ErrorTemplate = ErrorTemplate::new(
    "PD:tso:ErrLoadKeyspaceGroupsTerminated",
    "load keyspace groups terminated",
);

/// Load keyspace groups retry exhausted, %s.
pub const ERR_LOAD_KEYSPACE_GROUPS_RETRY_EXHAUSTED: ErrorTemplate = ErrorTemplate::new(
    "PD:tso:ErrLoadKeyspaceGroupsRetryExhausted",
    "load keyspace groups retry exhausted, %s",
);

/// The keyspace group %d isn't initialized.
pub const ERR_KEYSPACE_GROUP_NOT_INITIALIZED: ErrorTemplate = ErrorTemplate::new(
    "PD:tso:ErrKeyspaceGroupNotInitialized",
    "the keyspace group %d isn't initialized",
);

/// The keyspace %d isn't assigned to any keyspace group.
pub const ERR_KEYSPACE_NOT_ASSIGNED: ErrorTemplate = ErrorTemplate::new(
    "PD:tso:ErrKeyspaceNotAssigned",
    "the keyspace %d isn't assigned to any keyspace group",
);

/// Get min ts failed, %s.
pub const ERR_GET_MIN_TS: ErrorTemplate =
    ErrorTemplate::new("PD:tso:ErrGetMinTS", "get min ts failed, %s");

/// The keyspace group %d is merging.
pub const ERR_KEYSPACE_GROUP_IS_MERGING: ErrorTemplate = ErrorTemplate::new(
    "PD:tso:ErrKeyspaceGroupIsMerging",
    "the keyspace group %d is merging",
);

/// Etcd leader not found.
pub const ERR_ETCD_LEADER_NOT_FOUND: ErrorTemplate =
    ErrorTemplate::new("PD:member:ErrEtcdLeaderNotFound", "etcd leader not found");

/// Marshal member failed.
pub const ERR_MARSHAL_MEMBER: ErrorTemplate =
    ErrorTemplate::new("PD:member:ErrMarshalMember", "marshal member failed");

/// Marshal participant failed.
pub const ERR_MARSHAL_PARTICIPANT: ErrorTemplate = ErrorTemplate::new(
    "PD:member:ErrMarshalParticipant",
    "marshal participant failed",
);

/// Check campaign failed.
pub const ERR_CHECK_CAMPAIGN: ErrorTemplate =
    ErrorTemplate::new("PD:member:ErrCheckCampaign", "check campaign failed");

/// Wrong range keys.
pub const ERR_WRONG_RANGE_KEYS: ErrorTemplate =
    ErrorTemplate::new("PD:core:ErrWrongRangeKeys", "wrong range keys");

/// Store %v not found.
pub const ERR_STORE_NOT_FOUND: ErrorTemplate =
    ErrorTemplate::new("PD:core:ErrStoreNotFound", "store %v not found");

/// Store %v has been removed.
pub const ERR_STORE_REMOVED: ErrorTemplate =
    ErrorTemplate::new("PD:core:ErrStoreRemoved", "store %v has been removed");

/// Store %v has been physically destroyed.
pub const ERR_STORE_DESTROYED: ErrorTemplate = ErrorTemplate::new(
    "PD:core:ErrStoreDestroyed",
    "store %v has been physically destroyed",
);

/// Store %v is unhealthy.
pub const ERR_STORE_UNHEALTHY: ErrorTemplate =
    ErrorTemplate::new("PD:core:ErrStoreUnhealthy", "store %v is unhealthy");

/// Store %v has been serving.
pub const ERR_STORE_SERVING: ErrorTemplate =
    ErrorTemplate::new("PD:core:ErrStoreServing", "store %v has been serving");

/// Store %v is evicted as a slow store by trend.
pub const ERR_SLOW_TREND_EVICTED: ErrorTemplate = ErrorTemplate::new(
    "PD:core:ErrSlowTrendEvicted",
    "store %v is evicted as a slow store by trend",
);

/// Can not remove store %v since the number of up stores would be %v while need %v.
pub const ERR_STORES_NOT_ENOUGH: ErrorTemplate = ErrorTemplate::new(
    "PD:core:ErrStoresNotEnough",
    "can not remove store %v since the number of up stores would be %v while need %v",
);

/// Can not remove store %d since there are no extra up store to store the leader.
pub const ERR_NO_STORE_FOR_REGION_LEADER: ErrorTemplate = ErrorTemplate::new(
    "PD:core:ErrNoStoreForRegionLeader",
    "can not remove store %d since there are no extra up store to store the leader",
);

/// Create TSO stream failed, %s.
pub const ERR_CLIENT_CREATE_TSO_STREAM: ErrorTemplate = ErrorTemplate::new(
    "PD:client:ErrClientCreateTSOStream",
    "create TSO stream failed, %s",
);

/// Get TSO failed.
pub const ERR_CLIENT_GET_TSO: ErrorTemplate =
    ErrorTemplate::new("PD:client:ErrClientGetTSO", "get TSO failed");

/// Get leader failed, %v.
pub const ERR_CLIENT_GET_LEADER: ErrorTemplate =
    ErrorTemplate::new("PD:client:ErrClientGetLeader", "get leader failed, %v");

/// Get member failed.
pub const ERR_CLIENT_GET_MEMBER: ErrorTemplate =
    ErrorTemplate::new("PD:client:ErrClientGetMember", "get member failed");

/// Get min TSO failed, %v.
pub const ERR_CLIENT_GET_MIN_TSO: ErrorTemplate =
    ErrorTemplate::new("PD:client:ErrClientGetMinTSO", "get min TSO failed, %v");

/// Common, TSO, member, core and client templates in Go declaration order.
pub const FOUNDATION_ERRORS: &[ErrorTemplate] = &[
    ERR_GET_SOURCE_STORE,
    ERR_GET_TARGET_STORE,
    ERR_INCORRECT_SYSTEM_TIME,
    ERR_INVALID_ARGUMENT,
    ERR_GET_ALLOCATOR,
    ERR_RESET_USER_TIMESTAMP,
    ERR_GENERATE_TIMESTAMP,
    ERR_UPDATE_TIMESTAMP,
    ERR_SAVE_TIMESTAMP,
    ERR_LOGIC_OVERFLOW,
    ERR_PROXY_TSO_TIMEOUT,
    ERR_KEYSPACE_GROUP_ID_INVALID,
    ERR_LOAD_KEYSPACE_GROUPS_TIMEOUT,
    ERR_LOAD_KEYSPACE_GROUPS_TERMINATED,
    ERR_LOAD_KEYSPACE_GROUPS_RETRY_EXHAUSTED,
    ERR_KEYSPACE_GROUP_NOT_INITIALIZED,
    ERR_KEYSPACE_NOT_ASSIGNED,
    ERR_GET_MIN_TS,
    ERR_KEYSPACE_GROUP_IS_MERGING,
    ERR_ETCD_LEADER_NOT_FOUND,
    ERR_MARSHAL_MEMBER,
    ERR_MARSHAL_PARTICIPANT,
    ERR_CHECK_CAMPAIGN,
    ERR_WRONG_RANGE_KEYS,
    ERR_STORE_NOT_FOUND,
    ERR_STORE_REMOVED,
    ERR_STORE_DESTROYED,
    ERR_STORE_UNHEALTHY,
    ERR_STORE_SERVING,
    ERR_SLOW_TREND_EVICTED,
    ERR_STORES_NOT_ENOUGH,
    ERR_NO_STORE_FOR_REGION_LEADER,
    ERR_CLIENT_CREATE_TSO_STREAM,
    ERR_CLIENT_GET_TSO,
    ERR_CLIENT_GET_LEADER,
    ERR_CLIENT_GET_MEMBER,
    ERR_CLIENT_GET_MIN_TSO,
];
