// Copyright 2026 AsterSQL.

//! Stable domain error definitions from Go errno.go at
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

/// ErrOperatorNotFound preserves the Go error code and message template.
pub const ERR_OPERATOR_NOT_FOUND: ErrorTemplate =
    ErrorTemplate::new("PD:operator:ErrOperatorNotFound", "operator not found");

/// ErrAddOperator preserves the Go error code and message template.
pub const ERR_ADD_OPERATOR: ErrorTemplate = ErrorTemplate::new(
    "PD:operator:ErrAddOperator",
    "failed to add operator, maybe already have one",
);

/// ErrRegionInvalidID preserves the Go error code and message template.
pub const ERR_REGION_INVALID_ID: ErrorTemplate =
    ErrorTemplate::new("PD:region:ErrRegionInvalidID", "invalid region id");

/// ErrRegionNotAdjacent preserves the Go error code and message template.
pub const ERR_REGION_NOT_ADJACENT: ErrorTemplate = ErrorTemplate::new(
    "PD:region:ErrRegionNotAdjacent",
    "two regions are not adjacent",
);

/// ErrRegionNotFound preserves the Go error code and message template.
pub const ERR_REGION_NOT_FOUND: ErrorTemplate =
    ErrorTemplate::new("PD:region:ErrRegionNotFound", "region %v not found");

/// ErrRegionAbnormalPeer preserves the Go error code and message template.
pub const ERR_REGION_ABNORMAL_PEER: ErrorTemplate = ErrorTemplate::new(
    "PD:region:ErrRegionAbnormalPeer",
    "region %v has abnormal peer",
);

/// ErrPluginNotFound preserves the Go error code and message template.
pub const ERR_PLUGIN_NOT_FOUND: ErrorTemplate =
    ErrorTemplate::new("PD:operator:ErrPluginNotFound", "plugin is not found: %s");

/// ErrUnexpectedOperatorStatus preserves the Go error code and message template.
pub const ERR_UNEXPECTED_OPERATOR_STATUS: ErrorTemplate = ErrorTemplate::new(
    "PD:schedule:ErrUnexpectedOperatorStatus",
    "operator with unexpected status",
);

/// ErrUnknownOperatorStep preserves the Go error code and message template.
pub const ERR_UNKNOWN_OPERATOR_STEP: ErrorTemplate = ErrorTemplate::new(
    "PD:schedule:ErrUnknownOperatorStep",
    "unknown operator step found",
);

/// ErrMergeOperator preserves the Go error code and message template.
pub const ERR_MERGE_OPERATOR: ErrorTemplate =
    ErrorTemplate::new("PD:schedule:ErrMergeOperator", "merge operator error, %s");

/// ErrCreateOperator preserves the Go error code and message template.
pub const ERR_CREATE_OPERATOR: ErrorTemplate = ErrorTemplate::new(
    "PD:schedule:ErrCreateOperator",
    "unable to create operator, %s",
);

/// ErrCheckerNotFound preserves the Go error code and message template.
pub const ERR_CHECKER_NOT_FOUND: ErrorTemplate =
    ErrorTemplate::new("PD:checker:ErrCheckerNotFound", "checker not found");

/// ErrCheckerMergeAgain preserves the Go error code and message template.
pub const ERR_CHECKER_MERGE_AGAIN: ErrorTemplate = ErrorTemplate::new(
    "PD:checker:ErrCheckerMergeAgain",
    "region will be merged again, %s",
);

/// ErrNoStoreToAdd preserves the Go error code and message template.
pub const ERR_NO_STORE_TO_ADD: ErrorTemplate =
    ErrorTemplate::new("PD:checker:ErrNoStoreToAdd", "no store to add peer");

/// ErrNoStoreToReplace preserves the Go error code and message template.
pub const ERR_NO_STORE_TO_REPLACE: ErrorTemplate =
    ErrorTemplate::new("PD:checker:ErrNoStoreToReplace", "no store to replace peer");

/// ErrPeerCannotBeLeader preserves the Go error code and message template.
pub const ERR_PEER_CANNOT_BE_LEADER: ErrorTemplate =
    ErrorTemplate::new("PD:checker:ErrPeerCannotBeLeader", "peer cannot be leader");

/// ErrPeerCannotBeWitness preserves the Go error code and message template.
pub const ERR_PEER_CANNOT_BE_WITNESS: ErrorTemplate = ErrorTemplate::new(
    "PD:checker:ErrPeerCannotBeWitness",
    "peer cannot be witness",
);

/// ErrNoNewLeader preserves the Go error code and message template.
pub const ERR_NO_NEW_LEADER: ErrorTemplate =
    ErrorTemplate::new("PD:checker:ErrNoNewLeader", "no new leader");

/// ErrRegionNoLeader preserves the Go error code and message template.
pub const ERR_REGION_NO_LEADER: ErrorTemplate =
    ErrorTemplate::new("PD:checker:ErrRegionNoLeader", "region no leader");

/// ErrKeyspaceNotFound preserves the Go error code and message template.
pub const ERR_KEYSPACE_NOT_FOUND: ErrorTemplate =
    ErrorTemplate::new("PD:keyspace:ErrKeyspaceNotFound", "keyspace does not exist");

/// ErrRegionSplitTimeout preserves the Go error code and message template.
pub const ERR_REGION_SPLIT_TIMEOUT: ErrorTemplate =
    ErrorTemplate::new("PD:keyspace:ErrRegionSplitTimeout", "region split timeout");

/// ErrRegionSplitFailed preserves the Go error code and message template.
pub const ERR_REGION_SPLIT_FAILED: ErrorTemplate =
    ErrorTemplate::new("PD:keyspace:ErrRegionSplitFailed", "region split failed");

/// ErrKeyspaceExists preserves the Go error code and message template.
pub const ERR_KEYSPACE_EXISTS: ErrorTemplate =
    ErrorTemplate::new("PD:keyspace:ErrKeyspaceExists", "keyspace already exists");

/// ErrKeyspaceGroupExists preserves the Go error code and message template.
pub const ERR_KEYSPACE_GROUP_EXISTS: ErrorTemplate = ErrorTemplate::new(
    "PD:keyspace:ErrKeyspaceGroupExists",
    "keyspace group already exists",
);

/// ErrKeyspaceNotInKeyspaceGroup preserves the Go error code and message template.
pub const ERR_KEYSPACE_NOT_IN_KEYSPACE_GROUP: ErrorTemplate = ErrorTemplate::new(
    "PD:keyspace:ErrKeyspaceNotInKeyspaceGroup",
    "keyspace is not in this keyspace group",
);

/// ErrKeyspaceNotInAnyKeyspaceGroup preserves the Go error code and message template.
pub const ERR_KEYSPACE_NOT_IN_ANY_KEYSPACE_GROUP: ErrorTemplate = ErrorTemplate::new(
    "PD:keyspace:ErrKeyspaceNotInAnyKeyspaceGroup",
    "keyspace is not in any keyspace group",
);

/// ErrNodeNotInKeyspaceGroup preserves the Go error code and message template.
pub const ERR_NODE_NOT_IN_KEYSPACE_GROUP: ErrorTemplate = ErrorTemplate::new(
    "PD:keyspace:ErrNodeNotInKeyspaceGroup",
    "the tso node is not in this keyspace group",
);

/// ErrKeyspaceGroupNotEnoughReplicas preserves the Go error code and message template.
pub const ERR_KEYSPACE_GROUP_NOT_ENOUGH_REPLICAS: ErrorTemplate = ErrorTemplate::new(
    "PD:keyspace:ErrKeyspaceGroupNotEnoughReplicas",
    "not enough replicas in the keyspace group",
);

/// ErrKeyspaceGroupWithEmptyKeyspace preserves the Go error code and message template.
pub const ERR_KEYSPACE_GROUP_WITH_EMPTY_KEYSPACE: ErrorTemplate = ErrorTemplate::new(
    "PD:keyspace:ErrKeyspaceGroupWithEmptyKeyspace",
    "keyspace group with empty keyspace",
);

/// ErrModifyDefaultKeyspaceGroup preserves the Go error code and message template.
pub const ERR_MODIFY_DEFAULT_KEYSPACE_GROUP: ErrorTemplate = ErrorTemplate::new(
    "PD:keyspace:ErrModifyDefaultKeyspaceGroup",
    "default keyspace group cannot be modified",
);

/// ErrNoAvailableNode preserves the Go error code and message template.
pub const ERR_NO_AVAILABLE_NODE: ErrorTemplate =
    ErrorTemplate::new("PD:keyspace:ErrNoAvailableNode", "no available node");

/// ErrExceedMaxEtcdTxnOps preserves the Go error code and message template.
pub const ERR_EXCEED_MAX_ETCD_TXN_OPS: ErrorTemplate = ErrorTemplate::new(
    "PD:keyspace:ErrExceedMaxEtcdTxnOps",
    "exceed max etcd txn operations",
);

/// ErrModifyDefaultKeyspace preserves the Go error code and message template.
pub const ERR_MODIFY_DEFAULT_KEYSPACE: ErrorTemplate = ErrorTemplate::new(
    "PD:keyspace:ErrModifyDefaultKeyspace",
    "cannot modify default keyspace's state",
);

/// ErrModifyReservedKeyspace preserves the Go error code and message template.
pub const ERR_MODIFY_RESERVED_KEYSPACE: ErrorTemplate = ErrorTemplate::new(
    "PD:keyspace:ErrModifyReservedKeyspace",
    "cannot modify reserved keyspace's state",
);

/// ErrIllegalOperation preserves the Go error code and message template.
pub const ERR_ILLEGAL_OPERATION: ErrorTemplate =
    ErrorTemplate::new("PD:keyspace:ErrIllegalOperation", "unknown operation");

/// ErrUnsupportedOperationInKeyspace preserves the Go error code and message template.
pub const ERR_UNSUPPORTED_OPERATION_IN_KEYSPACE: ErrorTemplate = ErrorTemplate::new(
    "PD:keyspace:ErrUnsupportedOperationInKeyspace",
    "it's a unsupported operation",
);

/// ErrKeyspaceConfigPreconditionFailed preserves the Go error code and message template.
pub const ERR_KEYSPACE_CONFIG_PRECONDITION_FAILED: ErrorTemplate = ErrorTemplate::new(
    "PD:keyspace:ErrKeyspaceConfigPreconditionFailed",
    "keyspace config precondition failed, %s",
);

/// ErrKeyspaceGroupPrimaryNotFound preserves the Go error code and message template.
pub const ERR_KEYSPACE_GROUP_PRIMARY_NOT_FOUND: ErrorTemplate = ErrorTemplate::new(
    "PD:keyspace:ErrKeyspaceGroupPrimaryNotFound",
    "primary of keyspace group does not exist",
);

/// ErrKeyspaceGroupNotExists preserves the Go error code and message template.
pub const ERR_KEYSPACE_GROUP_NOT_EXISTS: ErrorTemplate = ErrorTemplate::new(
    "PD:keyspace:ErrKeyspaceGroupNotExists",
    "keyspace group %v does not exist",
);

/// ErrKeyspaceGroupInSplit preserves the Go error code and message template.
pub const ERR_KEYSPACE_GROUP_IN_SPLIT: ErrorTemplate = ErrorTemplate::new(
    "PD:keyspace:ErrKeyspaceGroupInSplit",
    "keyspace group %v is in split state",
);

/// ErrKeyspaceGroupNotInSplit preserves the Go error code and message template.
pub const ERR_KEYSPACE_GROUP_NOT_IN_SPLIT: ErrorTemplate = ErrorTemplate::new(
    "PD:keyspace:ErrKeyspaceGroupNotInSplit",
    "keyspace group %v is not in split state",
);

/// ErrKeyspaceGroupInMerging preserves the Go error code and message template.
pub const ERR_KEYSPACE_GROUP_IN_MERGING: ErrorTemplate = ErrorTemplate::new(
    "PD:keyspace:ErrKeyspaceGroupInMerging",
    "keyspace group %v is in merging state",
);

/// ErrKeyspaceGroupNotInMerging preserves the Go error code and message template.
pub const ERR_KEYSPACE_GROUP_NOT_IN_MERGING: ErrorTemplate = ErrorTemplate::new(
    "PD:keyspace:ErrKeyspaceGroupNotInMerging",
    "keyspace group %v is not in merging state",
);

/// ErrKeyspaceGroupModRevisionStale preserves the Go error code and message template.
pub const ERR_KEYSPACE_GROUP_MOD_REVISION_STALE: ErrorTemplate = ErrorTemplate::new(
    "PD:keyspace:ErrKeyspaceGroupModRevisionStale",
    "keyspace group mod revision is stale",
);

/// ErrUnknownMetaServiceGroup preserves the Go error code and message template.
pub const ERR_UNKNOWN_META_SERVICE_GROUP: ErrorTemplate = ErrorTemplate::new(
    "PD:keyspace:ErrUnknownMetaServiceGroup",
    "unknown meta-service group",
);

/// ErrInvalidAssignmentCount preserves the Go error code and message template.
pub const ERR_INVALID_ASSIGNMENT_COUNT: ErrorTemplate = ErrorTemplate::new(
    "PD:keyspace:ErrInvalidAssignmentCount",
    "assignment count must be non-negative",
);

/// ErrMetaServiceGroupDisabled preserves the Go error code and message template.
pub const ERR_META_SERVICE_GROUP_DISABLED: ErrorTemplate = ErrorTemplate::new(
    "PD:keyspace:ErrMetaServiceGroupDisabled",
    "meta-service group is disabled",
);

/// ErrGroupHasAssignedKeyspaces preserves the Go error code and message template.
pub const ERR_GROUP_HAS_ASSIGNED_KEYSPACES: ErrorTemplate = ErrorTemplate::new(
    "PD:keyspace:ErrGroupHasAssignedKeyspaces",
    "cannot delete meta-service group with assigned keyspaces",
);

/// ErrMetaServiceGroupUnhealthy preserves the Go error code and message template.
pub const ERR_META_SERVICE_GROUP_UNHEALTHY: ErrorTemplate = ErrorTemplate::new(
    "PD:keyspace:ErrMetaServiceGroupUnhealthy",
    "meta-service group etcd server is unhealthy",
);

/// ErrRegionRuleContent preserves the Go error code and message template.
pub const ERR_REGION_RULE_CONTENT: ErrorTemplate = ErrorTemplate::new(
    "PD:region:ErrRegionRuleContent",
    "invalid region rule content, %s",
);

/// ErrRegionRuleNotFound preserves the Go error code and message template.
pub const ERR_REGION_RULE_NOT_FOUND: ErrorTemplate = ErrorTemplate::new(
    "PD:region:ErrRegionRuleNotFound",
    "region label rule not found for id %s",
);

/// Operator, schedule, checker, region and keyspace templates in Go declaration order.
pub const SCHEDULING_ERRORS: &[ErrorTemplate] = &[
    ERR_OPERATOR_NOT_FOUND,
    ERR_ADD_OPERATOR,
    ERR_REGION_INVALID_ID,
    ERR_REGION_NOT_ADJACENT,
    ERR_REGION_NOT_FOUND,
    ERR_REGION_ABNORMAL_PEER,
    ERR_PLUGIN_NOT_FOUND,
    ERR_UNEXPECTED_OPERATOR_STATUS,
    ERR_UNKNOWN_OPERATOR_STEP,
    ERR_MERGE_OPERATOR,
    ERR_CREATE_OPERATOR,
    ERR_CHECKER_NOT_FOUND,
    ERR_CHECKER_MERGE_AGAIN,
    ERR_NO_STORE_TO_ADD,
    ERR_NO_STORE_TO_REPLACE,
    ERR_PEER_CANNOT_BE_LEADER,
    ERR_PEER_CANNOT_BE_WITNESS,
    ERR_NO_NEW_LEADER,
    ERR_REGION_NO_LEADER,
    ERR_KEYSPACE_NOT_FOUND,
    ERR_REGION_SPLIT_TIMEOUT,
    ERR_REGION_SPLIT_FAILED,
    ERR_KEYSPACE_EXISTS,
    ERR_KEYSPACE_GROUP_EXISTS,
    ERR_KEYSPACE_NOT_IN_KEYSPACE_GROUP,
    ERR_KEYSPACE_NOT_IN_ANY_KEYSPACE_GROUP,
    ERR_NODE_NOT_IN_KEYSPACE_GROUP,
    ERR_KEYSPACE_GROUP_NOT_ENOUGH_REPLICAS,
    ERR_KEYSPACE_GROUP_WITH_EMPTY_KEYSPACE,
    ERR_MODIFY_DEFAULT_KEYSPACE_GROUP,
    ERR_NO_AVAILABLE_NODE,
    ERR_EXCEED_MAX_ETCD_TXN_OPS,
    ERR_MODIFY_DEFAULT_KEYSPACE,
    ERR_MODIFY_RESERVED_KEYSPACE,
    ERR_ILLEGAL_OPERATION,
    ERR_UNSUPPORTED_OPERATION_IN_KEYSPACE,
    ERR_KEYSPACE_CONFIG_PRECONDITION_FAILED,
    ERR_KEYSPACE_GROUP_PRIMARY_NOT_FOUND,
    ERR_KEYSPACE_GROUP_NOT_EXISTS,
    ERR_KEYSPACE_GROUP_IN_SPLIT,
    ERR_KEYSPACE_GROUP_NOT_IN_SPLIT,
    ERR_KEYSPACE_GROUP_IN_MERGING,
    ERR_KEYSPACE_GROUP_NOT_IN_MERGING,
    ERR_KEYSPACE_GROUP_MOD_REVISION_STALE,
    ERR_UNKNOWN_META_SERVICE_GROUP,
    ERR_INVALID_ASSIGNMENT_COUNT,
    ERR_META_SERVICE_GROUP_DISABLED,
    ERR_GROUP_HAS_ASSIGNED_KEYSPACES,
    ERR_META_SERVICE_GROUP_UNHEALTHY,
    ERR_REGION_RULE_CONTENT,
    ERR_REGION_RULE_NOT_FOUND,
];

/// ErrServiceRegistered preserves the Go error code and message template.
pub const ERR_SERVICE_REGISTERED: ErrorTemplate = ErrorTemplate::new(
    "PD:server:ErrServiceRegistered",
    "service with path [%s] already registered",
);

/// ErrAPIInformationInvalid preserves the Go error code and message template.
pub const ERR_API_INFORMATION_INVALID: ErrorTemplate = ErrorTemplate::new(
    "PD:server:ErrAPIInformationInvalid",
    "invalid api information, group %s version %s",
);

/// ErrClientURLEmpty preserves the Go error code and message template.
pub const ERR_CLIENT_URL_EMPTY: ErrorTemplate =
    ErrorTemplate::new("PD:server:ErrClientEmpty", "client url empty");

/// ErrLeaderNil preserves the Go error code and message template.
pub const ERR_LEADER_NIL: ErrorTemplate =
    ErrorTemplate::new("PD:server:ErrLeaderNil", "leader is nil");

/// ErrCancelStartEtcd preserves the Go error code and message template.
pub const ERR_CANCEL_START_ETCD: ErrorTemplate =
    ErrorTemplate::new("PD:server:ErrCancelStartEtcd", "etcd start canceled");

/// ErrConfigItem preserves the Go error code and message template.
pub const ERR_CONFIG_ITEM: ErrorTemplate = ErrorTemplate::new(
    "PD:server:ErrConfiguration",
    "cannot set invalid configuration",
);

/// ErrServerNotStarted preserves the Go error code and message template.
pub const ERR_SERVER_NOT_STARTED: ErrorTemplate =
    ErrorTemplate::new("PD:server:ErrServerNotStarted", "server not started");

/// ErrRateLimitExceeded preserves the Go error code and message template.
pub const ERR_RATE_LIMIT_EXCEEDED: ErrorTemplate =
    ErrorTemplate::new("PD:server:ErrRateLimitExceeded", "rate limit exceeded");

/// ErrLeaderFrequentlyChange preserves the Go error code and message template.
pub const ERR_LEADER_FREQUENTLY_CHANGE: ErrorTemplate = ErrorTemplate::new(
    "PD:server:ErrLeaderFrequentlyChange",
    "leader %s frequently changed, leader-key is [%s]",
);

/// ErrBytesToUint64 preserves the Go error code and message template.
pub const ERR_BYTES_TO_UINT64: ErrorTemplate = ErrorTemplate::new(
    "PD:typeutil:ErrBytesToUint64",
    "invalid data, must 8 bytes, but %d",
);

/// ErrMaxWaitingTasksExceeded preserves the Go error code and message template.
pub const ERR_MAX_WAITING_TASKS_EXCEEDED: ErrorTemplate = ErrorTemplate::new(
    "PD:ratelimit:ErrMaxWaitingTasksExceeded",
    "max waiting tasks exceeded",
);

/// Server, API information, type conversion, and rate limit errors.
pub const SERVICE_ERRORS: &[ErrorTemplate] = &[
    ERR_SERVICE_REGISTERED,
    ERR_API_INFORMATION_INVALID,
    ERR_CLIENT_URL_EMPTY,
    ERR_LEADER_NIL,
    ERR_CANCEL_START_ETCD,
    ERR_CONFIG_ITEM,
    ERR_SERVER_NOT_STARTED,
    ERR_RATE_LIMIT_EXCEEDED,
    ERR_LEADER_FREQUENTLY_CHANGE,
    ERR_BYTES_TO_UINT64,
    ERR_MAX_WAITING_TASKS_EXCEEDED,
];

/// ErrURLParse preserves the Go error code and message template.
pub const ERR_URL_PARSE: ErrorTemplate =
    ErrorTemplate::new("PD:url:ErrURLParse", "parse url error");

/// ErrQueryUnescape preserves the Go error code and message template.
pub const ERR_QUERY_UNESCAPE: ErrorTemplate = ErrorTemplate::new(
    "PD:url:ErrQueryUnescape",
    "inverse transformation of QueryEscape error",
);

/// ErrGRPCDial preserves the Go error code and message template.
pub const ERR_GRPC_DIAL: ErrorTemplate = ErrorTemplate::new("PD:grpc:ErrGRPCDial", "dial error");

/// ErrCloseGRPCConn preserves the Go error code and message template.
pub const ERR_CLOSE_GRPC_CONN: ErrorTemplate =
    ErrorTemplate::new("PD:grpc:ErrCloseGRPCConn", "close gRPC connection failed");

/// ErrGRPCSend preserves the Go error code and message template.
pub const ERR_GRPC_SEND: ErrorTemplate =
    ErrorTemplate::new("PD:grpc:ErrGRPCSend", "send request error");

/// ErrGRPCRecv preserves the Go error code and message template.
pub const ERR_GRPC_RECV: ErrorTemplate =
    ErrorTemplate::new("PD:grpc:ErrGRPCRecv", "receive response error");

/// ErrGRPCCloseSend preserves the Go error code and message template.
pub const ERR_GRPC_CLOSE_SEND: ErrorTemplate =
    ErrorTemplate::new("PD:grpc:ErrGRPCCloseSend", "close send error");

/// ErrGRPCCreateStream preserves the Go error code and message template.
pub const ERR_GRPC_CREATE_STREAM: ErrorTemplate =
    ErrorTemplate::new("PD:grpc:ErrGRPCCreateStream", "create stream error");

/// ErrProtoUnmarshal preserves the Go error code and message template.
pub const ERR_PROTO_UNMARSHAL: ErrorTemplate =
    ErrorTemplate::new("PD:proto:ErrProtoUnmarshal", "failed to unmarshal proto");

/// ErrProtoMarshal preserves the Go error code and message template.
pub const ERR_PROTO_MARSHAL: ErrorTemplate =
    ErrorTemplate::new("PD:proto:ErrProtoMarshal", "failed to marshal proto");

/// ErrNewEtcdClient preserves the Go error code and message template.
pub const ERR_NEW_ETCD_CLIENT: ErrorTemplate =
    ErrorTemplate::new("PD:etcd:ErrNewEtcdClient", "new etcd client failed");

/// ErrStartEtcd preserves the Go error code and message template.
pub const ERR_START_ETCD: ErrorTemplate =
    ErrorTemplate::new("PD:etcd:ErrStartEtcd", "start etcd failed");

/// ErrEtcdURLMap preserves the Go error code and message template.
pub const ERR_ETCD_URL_MAP: ErrorTemplate =
    ErrorTemplate::new("PD:etcd:ErrEtcdURLMap", "etcd url map error");

/// ErrEtcdGrantLease preserves the Go error code and message template.
pub const ERR_ETCD_GRANT_LEASE: ErrorTemplate =
    ErrorTemplate::new("PD:etcd:ErrEtcdGrantLease", "etcd lease failed");

/// ErrEtcdTxnInternal preserves the Go error code and message template.
pub const ERR_ETCD_TXN_INTERNAL: ErrorTemplate = ErrorTemplate::new(
    "PD:etcd:ErrEtcdTxnInternal",
    "internal etcd transaction error occurred",
);

/// ErrEtcdTxnConflict preserves the Go error code and message template.
pub const ERR_ETCD_TXN_CONFLICT: ErrorTemplate = ErrorTemplate::new(
    "PD:etcd:ErrEtcdTxnConflict",
    "etcd transaction failed, conflicted and rolled back",
);

/// ErrEtcdTxnResponse preserves the Go error code and message template.
pub const ERR_ETCD_TXN_RESPONSE: ErrorTemplate = ErrorTemplate::new(
    "PD:etcd:ErrEtcdTxnResponse",
    "etcd transaction returned unexpected response: %v",
);

/// ErrEtcdKVPut preserves the Go error code and message template.
pub const ERR_ETCD_KV_PUT: ErrorTemplate =
    ErrorTemplate::new("PD:etcd:ErrEtcdKVPut", "etcd KV put failed");

/// ErrEtcdKVDelete preserves the Go error code and message template.
pub const ERR_ETCD_KV_DELETE: ErrorTemplate =
    ErrorTemplate::new("PD:etcd:ErrEtcdKVDelete", "etcd KV delete failed");

/// ErrEtcdKVGet preserves the Go error code and message template.
pub const ERR_ETCD_KV_GET: ErrorTemplate =
    ErrorTemplate::new("PD:etcd:ErrEtcdKVGet", "etcd KV get failed");

/// ErrEtcdKVGetResponse preserves the Go error code and message template.
pub const ERR_ETCD_KV_GET_RESPONSE: ErrorTemplate = ErrorTemplate::new(
    "PD:etcd:ErrEtcdKVGetResponse",
    "etcd invalid get value response %v, must only one",
);

/// ErrEtcdGetCluster preserves the Go error code and message template.
pub const ERR_ETCD_GET_CLUSTER: ErrorTemplate = ErrorTemplate::new(
    "PD:etcd:ErrEtcdGetCluster",
    "etcd get cluster from remote peer failed",
);

/// ErrEtcdMoveLeader preserves the Go error code and message template.
pub const ERR_ETCD_MOVE_LEADER: ErrorTemplate =
    ErrorTemplate::new("PD:etcd:ErrEtcdMoveLeader", "etcd move leader error");

/// ErrEtcdWatcherCancel preserves the Go error code and message template.
pub const ERR_ETCD_WATCHER_CANCEL: ErrorTemplate =
    ErrorTemplate::new("PD:etcd:ErrEtcdWatcherCancel", "watcher canceled");

/// ErrCloseEtcdClient preserves the Go error code and message template.
pub const ERR_CLOSE_ETCD_CLIENT: ErrorTemplate =
    ErrorTemplate::new("PD:etcd:ErrCloseEtcdClient", "close etcd client failed");

/// ErrEtcdMemberList preserves the Go error code and message template.
pub const ERR_ETCD_MEMBER_LIST: ErrorTemplate =
    ErrorTemplate::new("PD:etcd:ErrEtcdMemberList", "etcd member list failed");

/// ErrEtcdMemberRemove preserves the Go error code and message template.
pub const ERR_ETCD_MEMBER_REMOVE: ErrorTemplate =
    ErrorTemplate::new("PD:etcd:ErrEtcdMemberRemove", "etcd remove member failed");

/// ErrDashboardStart preserves the Go error code and message template.
pub const ERR_DASHBOARD_START: ErrorTemplate =
    ErrorTemplate::new("PD:dashboard:ErrDashboardStart", "start dashboard failed");

/// ErrDashboardStop preserves the Go error code and message template.
pub const ERR_DASHBOARD_STOP: ErrorTemplate =
    ErrorTemplate::new("PD:dashboard:ErrDashboardStop", "stop dashboard failed");

/// ErrStrconvParseBool preserves the Go error code and message template.
pub const ERR_STRCONV_PARSE_BOOL: ErrorTemplate =
    ErrorTemplate::new("PD:strconv:ErrStrconvParseBool", "parse bool error");

/// ErrStrconvParseInt preserves the Go error code and message template.
pub const ERR_STRCONV_PARSE_INT: ErrorTemplate =
    ErrorTemplate::new("PD:strconv:ErrStrconvParseInt", "parse int error");

/// ErrStrconvParseUint preserves the Go error code and message template.
pub const ERR_STRCONV_PARSE_UINT: ErrorTemplate =
    ErrorTemplate::new("PD:strconv:ErrStrconvParseUint", "parse uint error");

/// ErrStrconvParseFloat preserves the Go error code and message template.
pub const ERR_STRCONV_PARSE_FLOAT: ErrorTemplate =
    ErrorTemplate::new("PD:strconv:ErrStrconvParseFloat", "parse float error");

/// ErrPrometheusPushMetrics preserves the Go error code and message template.
pub const ERR_PROMETHEUS_PUSH_METRICS: ErrorTemplate = ErrorTemplate::new(
    "PD:prometheus:ErrPrometheusPushMetrics",
    "push metrics to gateway failed",
);

/// ErrPrometheusCreateClient preserves the Go error code and message template.
pub const ERR_PROMETHEUS_CREATE_CLIENT: ErrorTemplate = ErrorTemplate::new(
    "PD:prometheus:ErrPrometheusCreateClient",
    "create client error",
);

/// ErrPrometheusQuery preserves the Go error code and message template.
pub const ERR_PROMETHEUS_QUERY: ErrorTemplate =
    ErrorTemplate::new("PD:prometheus:ErrPrometheusQuery", "query error");

/// ErrSendRequest preserves the Go error code and message template.
pub const ERR_SEND_REQUEST: ErrorTemplate =
    ErrorTemplate::new("PD:http:ErrSendRequest", "send HTTP request failed");

/// ErrWriteHTTPBody preserves the Go error code and message template.
pub const ERR_WRITE_HTTP_BODY: ErrorTemplate =
    ErrorTemplate::new("PD:http:ErrWriteHTTPBody", "write HTTP body failed");

/// ErrNewHTTPRequest preserves the Go error code and message template.
pub const ERR_NEW_HTTP_REQUEST: ErrorTemplate =
    ErrorTemplate::new("PD:http:ErrNewHTTPRequest", "new HTTP request failed");

/// ErrIORead preserves the Go error code and message template.
pub const ERR_IO_READ: ErrorTemplate = ErrorTemplate::new("PD:ioutil:ErrIORead", "IO read error");

/// ErrOSOpen preserves the Go error code and message template.
pub const ERR_OS_OPEN: ErrorTemplate = ErrorTemplate::new("PD:os:ErrOSOpen", "open error");

/// ErrReadDirName preserves the Go error code and message template.
pub const ERR_READ_DIR_NAME: ErrorTemplate =
    ErrorTemplate::new("PD:dir:ErrReadDirName", "read dir name error");

/// ErrNetstatTCPSocks preserves the Go error code and message template.
pub const ERR_NETSTAT_TCP_SOCKS: ErrorTemplate =
    ErrorTemplate::new("PD:netstat:ErrNetstatTCPSocks", "TCP socks error");

/// ErrHexDecodingString preserves the Go error code and message template.
pub const ERR_HEX_DECODING_STRING: ErrorTemplate =
    ErrorTemplate::new("PD:hex:ErrHexDecodingString", "decode string %s error");

/// ErrFilePathAbs preserves the Go error code and message template.
pub const ERR_FILE_PATH_ABS: ErrorTemplate = ErrorTemplate::new(
    "PD:filepath:ErrFilePathAbs",
    "failed to convert a path to absolute path",
);

/// ErrLoadPlugin preserves the Go error code and message template.
pub const ERR_LOAD_PLUGIN: ErrorTemplate =
    ErrorTemplate::new("PD:plugin:ErrLoadPlugin", "failed to load plugin");

/// ErrLookupPluginFunc preserves the Go error code and message template.
pub const ERR_LOOKUP_PLUGIN_FUNC: ErrorTemplate = ErrorTemplate::new(
    "PD:plugin:ErrLookupPluginFunc",
    "failed to lookup plugin function",
);

/// ErrJSONMarshal preserves the Go error code and message template.
pub const ERR_JSON_MARSHAL: ErrorTemplate =
    ErrorTemplate::new("PD:json:ErrJSONMarshal", "failed to marshal json");

/// ErrJSONUnmarshal preserves the Go error code and message template.
pub const ERR_JSON_UNMARSHAL: ErrorTemplate =
    ErrorTemplate::new("PD:json:ErrJSONUnmarshal", "failed to unmarshal json");

/// ErrLevelDBClose preserves the Go error code and message template.
pub const ERR_LEVEL_DB_CLOSE: ErrorTemplate =
    ErrorTemplate::new("PD:leveldb:ErrLevelDBClose", "close leveldb error");

/// ErrLevelDBWrite preserves the Go error code and message template.
pub const ERR_LEVEL_DB_WRITE: ErrorTemplate =
    ErrorTemplate::new("PD:leveldb:ErrLevelDBWrite", "leveldb write error");

/// ErrLevelDBOpen preserves the Go error code and message template.
pub const ERR_LEVEL_DB_OPEN: ErrorTemplate =
    ErrorTemplate::new("PD:leveldb:ErrLevelDBOpen", "leveldb open file error");

/// ErrSemverNewVersion preserves the Go error code and message template.
pub const ERR_SEMVER_NEW_VERSION: ErrorTemplate =
    ErrorTemplate::new("PD:semver:ErrSemverNewVersion", "new version error");

/// ErrInitLogger preserves the Go error code and message template.
pub const ERR_INIT_LOGGER: ErrorTemplate =
    ErrorTemplate::new("PD:log:ErrInitLogger", "init logger error");

/// ErrEncryptionInvalidMethod preserves the Go error code and message template.
pub const ERR_ENCRYPTION_INVALID_METHOD: ErrorTemplate = ErrorTemplate::new(
    "PD:encryption:ErrEncryptionInvalidMethod",
    "invalid encryption method",
);

/// ErrEncryptionInvalidConfig preserves the Go error code and message template.
pub const ERR_ENCRYPTION_INVALID_CONFIG: ErrorTemplate =
    ErrorTemplate::new("PD:encryption:ErrEncryptionInvalidConfig", "invalid config");

/// ErrEncryptionGenerateIV preserves the Go error code and message template.
pub const ERR_ENCRYPTION_GENERATE_IV: ErrorTemplate = ErrorTemplate::new(
    "PD:encryption:ErrEncryptionGenerateIV",
    "fail to generate iv",
);

/// ErrEncryptionGCMEncrypt preserves the Go error code and message template.
pub const ERR_ENCRYPTION_GCM_ENCRYPT: ErrorTemplate = ErrorTemplate::new(
    "PD:encryption:ErrEncryptionGCMEncrypt",
    "GCM encryption fail",
);

/// ErrEncryptionGCMDecrypt preserves the Go error code and message template.
pub const ERR_ENCRYPTION_GCM_DECRYPT: ErrorTemplate = ErrorTemplate::new(
    "PD:encryption:ErrEncryptionGCMDecrypt",
    "GCM decryption fail",
);

/// ErrEncryptionCTREncrypt preserves the Go error code and message template.
pub const ERR_ENCRYPTION_CTR_ENCRYPT: ErrorTemplate = ErrorTemplate::new(
    "PD:encryption:ErrEncryptionCTREncrypt",
    "CTR encryption fail",
);

/// ErrEncryptionCTRDecrypt preserves the Go error code and message template.
pub const ERR_ENCRYPTION_CTR_DECRYPT: ErrorTemplate = ErrorTemplate::new(
    "PD:encryption:ErrEncryptionCTRDecrypt",
    "CTR decryption fail",
);

/// ErrEncryptionEncryptRegion preserves the Go error code and message template.
pub const ERR_ENCRYPTION_ENCRYPT_REGION: ErrorTemplate = ErrorTemplate::new(
    "PD:encryption:ErrEncryptionEncryptRegion",
    "encrypt region fail",
);

/// ErrEncryptionDecryptRegion preserves the Go error code and message template.
pub const ERR_ENCRYPTION_DECRYPT_REGION: ErrorTemplate = ErrorTemplate::new(
    "PD:encryption:ErrEncryptionDecryptRegion",
    "decrypt region fail",
);

/// ErrEncryptionNewDataKey preserves the Go error code and message template.
pub const ERR_ENCRYPTION_NEW_DATA_KEY: ErrorTemplate = ErrorTemplate::new(
    "PD:encryption:ErrEncryptionNewDataKey",
    "fail to generate data key",
);

/// ErrEncryptionNewMasterKey preserves the Go error code and message template.
pub const ERR_ENCRYPTION_NEW_MASTER_KEY: ErrorTemplate = ErrorTemplate::new(
    "PD:encryption:ErrEncryptionNewMasterKey",
    "fail to get master key",
);

/// ErrEncryptionCurrentKeyNotFound preserves the Go error code and message template.
pub const ERR_ENCRYPTION_CURRENT_KEY_NOT_FOUND: ErrorTemplate = ErrorTemplate::new(
    "PD:encryption:ErrEncryptionCurrentKeyNotFound",
    "current data key not found",
);

/// ErrEncryptionKeyNotFound preserves the Go error code and message template.
pub const ERR_ENCRYPTION_KEY_NOT_FOUND: ErrorTemplate = ErrorTemplate::new(
    "PD:encryption:ErrEncryptionKeyNotFound",
    "data key not found",
);

/// ErrEncryptionKeysWatcher preserves the Go error code and message template.
pub const ERR_ENCRYPTION_KEYS_WATCHER: ErrorTemplate = ErrorTemplate::new(
    "PD:encryption:ErrEncryptionKeysWatcher",
    "data key watcher error",
);

/// ErrEncryptionLoadKeys preserves the Go error code and message template.
pub const ERR_ENCRYPTION_LOAD_KEYS: ErrorTemplate = ErrorTemplate::new(
    "PD:encryption:ErrEncryptionLoadKeys",
    "load data keys error",
);

/// ErrEncryptionRotateDataKey preserves the Go error code and message template.
pub const ERR_ENCRYPTION_ROTATE_DATA_KEY: ErrorTemplate = ErrorTemplate::new(
    "PD:encryption:ErrEncryptionRotateDataKey",
    "failed to rotate data key",
);

/// ErrEncryptionSaveDataKeys preserves the Go error code and message template.
pub const ERR_ENCRYPTION_SAVE_DATA_KEYS: ErrorTemplate = ErrorTemplate::new(
    "PD:encryption:ErrEncryptionSaveDataKeys",
    "failed to save data keys",
);

/// ErrEncryptionKMS preserves the Go error code and message template.
pub const ERR_ENCRYPTION_KMS: ErrorTemplate =
    ErrorTemplate::new("PD:ErrEncryptionKMS", "KMS error");

/// ErrCryptoX509KeyPair preserves the Go error code and message template.
pub const ERR_CRYPTO_X509_KEY_PAIR: ErrorTemplate =
    ErrorTemplate::new("PD:crypto:ErrCryptoX509KeyPair", "x509 keypair error");

/// ErrCryptoAppendCertsFromPEM preserves the Go error code and message template.
pub const ERR_CRYPTO_APPEND_CERTS_FROM_PEM: ErrorTemplate = ErrorTemplate::new(
    "PD:crypto:ErrCryptoAppendCertsFromPEM",
    "cert pool append certs error",
);

/// ErrBindJSON preserves the Go error code and message template.
pub const ERR_BIND_JSON: ErrorTemplate =
    ErrorTemplate::new("PD:gin:ErrBindJSON", "bind JSON error");

/// Storage, system and third-party wrappers in Go declaration order.
pub const INFRASTRUCTURE_ERRORS: &[ErrorTemplate] = &[
    ERR_URL_PARSE,
    ERR_QUERY_UNESCAPE,
    ERR_GRPC_DIAL,
    ERR_CLOSE_GRPC_CONN,
    ERR_GRPC_SEND,
    ERR_GRPC_RECV,
    ERR_GRPC_CLOSE_SEND,
    ERR_GRPC_CREATE_STREAM,
    ERR_PROTO_UNMARSHAL,
    ERR_PROTO_MARSHAL,
    ERR_NEW_ETCD_CLIENT,
    ERR_START_ETCD,
    ERR_ETCD_URL_MAP,
    ERR_ETCD_GRANT_LEASE,
    ERR_ETCD_TXN_INTERNAL,
    ERR_ETCD_TXN_CONFLICT,
    ERR_ETCD_TXN_RESPONSE,
    ERR_ETCD_KV_PUT,
    ERR_ETCD_KV_DELETE,
    ERR_ETCD_KV_GET,
    ERR_ETCD_KV_GET_RESPONSE,
    ERR_ETCD_GET_CLUSTER,
    ERR_ETCD_MOVE_LEADER,
    ERR_ETCD_WATCHER_CANCEL,
    ERR_CLOSE_ETCD_CLIENT,
    ERR_ETCD_MEMBER_LIST,
    ERR_ETCD_MEMBER_REMOVE,
    ERR_DASHBOARD_START,
    ERR_DASHBOARD_STOP,
    ERR_STRCONV_PARSE_BOOL,
    ERR_STRCONV_PARSE_INT,
    ERR_STRCONV_PARSE_UINT,
    ERR_STRCONV_PARSE_FLOAT,
    ERR_PROMETHEUS_PUSH_METRICS,
    ERR_PROMETHEUS_CREATE_CLIENT,
    ERR_PROMETHEUS_QUERY,
    ERR_SEND_REQUEST,
    ERR_WRITE_HTTP_BODY,
    ERR_NEW_HTTP_REQUEST,
    ERR_IO_READ,
    ERR_OS_OPEN,
    ERR_READ_DIR_NAME,
    ERR_NETSTAT_TCP_SOCKS,
    ERR_HEX_DECODING_STRING,
    ERR_FILE_PATH_ABS,
    ERR_LOAD_PLUGIN,
    ERR_LOOKUP_PLUGIN_FUNC,
    ERR_JSON_MARSHAL,
    ERR_JSON_UNMARSHAL,
    ERR_LEVEL_DB_CLOSE,
    ERR_LEVEL_DB_WRITE,
    ERR_LEVEL_DB_OPEN,
    ERR_SEMVER_NEW_VERSION,
    ERR_INIT_LOGGER,
    ERR_ENCRYPTION_INVALID_METHOD,
    ERR_ENCRYPTION_INVALID_CONFIG,
    ERR_ENCRYPTION_GENERATE_IV,
    ERR_ENCRYPTION_GCM_ENCRYPT,
    ERR_ENCRYPTION_GCM_DECRYPT,
    ERR_ENCRYPTION_CTR_ENCRYPT,
    ERR_ENCRYPTION_CTR_DECRYPT,
    ERR_ENCRYPTION_ENCRYPT_REGION,
    ERR_ENCRYPTION_DECRYPT_REGION,
    ERR_ENCRYPTION_NEW_DATA_KEY,
    ERR_ENCRYPTION_NEW_MASTER_KEY,
    ERR_ENCRYPTION_CURRENT_KEY_NOT_FOUND,
    ERR_ENCRYPTION_KEY_NOT_FOUND,
    ERR_ENCRYPTION_KEYS_WATCHER,
    ERR_ENCRYPTION_LOAD_KEYS,
    ERR_ENCRYPTION_ROTATE_DATA_KEY,
    ERR_ENCRYPTION_SAVE_DATA_KEYS,
    ERR_ENCRYPTION_KMS,
    ERR_CRYPTO_X509_KEY_PAIR,
    ERR_CRYPTO_APPEND_CERTS_FROM_PEM,
    ERR_BIND_JSON,
];
