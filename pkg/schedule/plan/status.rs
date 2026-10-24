// Copyright 2026 AsterSQL.
use std::fmt;
/// Numeric diagnostic code; unknown and negative Go integer values are preserved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StatusCode(pub i64);
impl StatusCode {
    /// Go `StatusOK`.
    pub const OK: Self = Self(0);
    /// Go `StatusStoreScoreDisallowed`.
    pub const STORE_SCORE_DISALLOWED: Self = Self(100);
    /// Go `StatusStoreAlreadyHasPeer`.
    pub const STORE_ALREADY_HAS_PEER: Self = Self(101);
    /// Go `StatusStoreNotMatchRule`.
    pub const STORE_NOT_MATCH_RULE: Self = Self(102);
    /// Go `StatusStoreSnapshotThrottled`.
    pub const STORE_SNAPSHOT_THROTTLED: Self = Self(200);
    /// Go `StatusStorePendingPeerThrottled`.
    pub const STORE_PENDING_PEER_THROTTLED: Self = Self(201);
    /// Go `StatusStoreAddLimitThrottled`.
    pub const STORE_ADD_LIMIT_THROTTLED: Self = Self(202);
    /// Go `StatusStoreRemoveLimitThrottled`.
    pub const STORE_REMOVE_LIMIT_THROTTLED: Self = Self(203);
    /// Go `StatusStoreTransferLeaderInLimitThrottled`.
    pub const STORE_TRANSFER_LEADER_IN_LIMIT_THROTTLED: Self = Self(204);
    /// Go `StatusStoreRejectLeader`.
    pub const STORE_REJECT_LEADER: Self = Self(300);
    /// Go `StatusStoreNotMatchIsolation`.
    pub const STORE_NOT_MATCH_ISOLATION: Self = Self(301);
    /// Go `StatusStoreBusy`.
    pub const STORE_BUSY: Self = Self(400);
    /// Go `StatusStoreRemoved`.
    pub const STORE_REMOVED: Self = Self(401);
    /// Go `StatusStoreRemoving`.
    pub const STORE_REMOVING: Self = Self(402);
    /// Go `StatusStoreDown`.
    pub const STORE_DOWN: Self = Self(403);
    /// Go `StatusStoreDisconnected`.
    pub const STORE_DISCONNECTED: Self = Self(404);
    /// Go `StatusStoreLowSpace`.
    pub const STORE_LOW_SPACE: Self = Self(500);
    /// Go `StatusStoreNotExisted`.
    pub const STORE_NOT_EXISTED: Self = Self(501);
    /// Go `StatusRegionHot`.
    pub const REGION_HOT: Self = Self(1000);
    /// Go `StatusRegionUnhealthy`.
    pub const REGION_UNHEALTHY: Self = Self(1001);
    /// Go `StatusRegionEmpty`.
    pub const REGION_EMPTY: Self = Self(1002);
    /// Go `StatusRegionNotReplicated`.
    pub const REGION_NOT_REPLICATED: Self = Self(1003);
    /// Go `StatusRegionNotMatchRule`.
    pub const REGION_NOT_MATCH_RULE: Self = Self(1004);
    /// Go `StatusRegionNoLeader`.
    pub const REGION_NO_LEADER: Self = Self(1005);
    /// Go `StatusNoTargetRegion`.
    pub const NO_TARGET_REGION: Self = Self(1006);
    /// Go `StatusRegionLabelReject`.
    pub const REGION_LABEL_REJECT: Self = Self(1007);
    /// Go `StatusRegionSendSnapshotThrottled`.
    pub const REGION_SEND_SNAPSHOT_THROTTLED: Self = Self(1008);
    /// Go `StatusRegionAffinity`.
    pub const REGION_AFFINITY: Self = Self(1009);
    /// Go `StatusCreateOperatorFailed`.
    pub const CREATE_OPERATOR_FAILED: Self = Self(2000);
}
/// Returns an empty string for codes without a Go text mapping.
pub fn status_text(code: StatusCode) -> &'static str {
    match code {
        StatusCode::OK => "OK",
        StatusCode::STORE_SCORE_DISALLOWED => "StoreScoreDisallowed",
        StatusCode::STORE_ALREADY_HAS_PEER => "StoreAlreadyHasPeer",
        StatusCode::STORE_NOT_MATCH_RULE => "StoreNotMatchRule",
        StatusCode::STORE_SNAPSHOT_THROTTLED => "StoreSnapshotThrottled",
        StatusCode::STORE_PENDING_PEER_THROTTLED => "StorePendingPeerThrottled",
        StatusCode::STORE_ADD_LIMIT_THROTTLED => "StoreAddPeerThrottled",
        StatusCode::STORE_REMOVE_LIMIT_THROTTLED => "StoreRemovePeerThrottled",
        StatusCode::STORE_TRANSFER_LEADER_IN_LIMIT_THROTTLED => "StoreTransferLeaderInThrottled",
        StatusCode::STORE_REJECT_LEADER => "StoreRejectLeader",
        StatusCode::STORE_NOT_MATCH_ISOLATION => "StoreNotMatchIsolation",
        StatusCode::STORE_LOW_SPACE => "StoreLowSpace",
        StatusCode::STORE_REMOVING => "StoreRemoving",
        StatusCode::STORE_REMOVED => "StoreRemoved",
        StatusCode::STORE_DISCONNECTED => "StoreDisconnected",
        StatusCode::STORE_DOWN => "StoreDown",
        StatusCode::STORE_BUSY => "StoreBusy",
        StatusCode::STORE_NOT_EXISTED => "StoreNotExisted",
        StatusCode::REGION_HOT => "RegionHot",
        StatusCode::REGION_UNHEALTHY => "RegionUnhealthy",
        StatusCode::REGION_EMPTY => "RegionEmpty",
        StatusCode::REGION_NOT_REPLICATED => "RegionNotReplicated",
        StatusCode::REGION_NOT_MATCH_RULE => "RegionNotMatchRule",
        StatusCode::REGION_NO_LEADER => "RegionNoLeader",
        StatusCode::REGION_AFFINITY => "RegionAffinity",
        StatusCode::NO_TARGET_REGION => "NoTargetRegion",
        StatusCode::REGION_LABEL_REJECT => "RegionLabelReject",
        StatusCode::CREATE_OPERATOR_FAILED => "CreateOperatorFailed",
        _ => "",
    }
}
/// A diagnostic result with an optional explanatory reason.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Status {
    pub status_code: StatusCode,
    pub detailed_reason: String,
}
impl Status {
    /// Uses only the first reason, matching Go's variadic constructor.
    pub fn new(status_code: StatusCode, reasons: &[&str]) -> Self {
        Self {
            status_code,
            detailed_reason: reasons.first().copied().unwrap_or_default().to_owned(),
        }
    }
    /// Whether scheduling succeeded.
    pub fn is_ok(&self) -> bool {
        self.status_code == StatusCode::OK
    }
    /// Whether this is a normal diagnostic (codes 100 through 109).
    pub fn is_normal(&self) -> bool {
        self.status_code.0 / 10 == 10
    }
    /// Diagnostic priority in Go float32 precision.
    pub fn priority(&self) -> f32 {
        let category = self.status_code.0 / 100;
        if category == 1 {
            self.status_code.0 as f32 / 100.0
        } else {
            category as f32
        }
    }
}
impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(status_text(self.status_code))
    }
}
