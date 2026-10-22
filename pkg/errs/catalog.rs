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
