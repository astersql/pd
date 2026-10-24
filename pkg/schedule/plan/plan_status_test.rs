// Copyright 2026 AsterSQL.
use super::*;
#[test]
fn plan_status_codes_and_text() {
    assert_eq!(StatusCode::OK.0, 0);
    assert_eq!(status_text(StatusCode::OK), "OK");
    assert_eq!(Status::new(StatusCode::OK, &[]).to_string(), "OK");
    assert_eq!(StatusCode::STORE_SCORE_DISALLOWED.0, 100);
    assert_eq!(
        status_text(StatusCode::STORE_SCORE_DISALLOWED),
        "StoreScoreDisallowed"
    );
    assert_eq!(
        Status::new(StatusCode::STORE_SCORE_DISALLOWED, &[]).to_string(),
        "StoreScoreDisallowed"
    );
    assert_eq!(StatusCode::STORE_ALREADY_HAS_PEER.0, 101);
    assert_eq!(
        status_text(StatusCode::STORE_ALREADY_HAS_PEER),
        "StoreAlreadyHasPeer"
    );
    assert_eq!(
        Status::new(StatusCode::STORE_ALREADY_HAS_PEER, &[]).to_string(),
        "StoreAlreadyHasPeer"
    );
    assert_eq!(StatusCode::STORE_NOT_MATCH_RULE.0, 102);
    assert_eq!(
        status_text(StatusCode::STORE_NOT_MATCH_RULE),
        "StoreNotMatchRule"
    );
    assert_eq!(
        Status::new(StatusCode::STORE_NOT_MATCH_RULE, &[]).to_string(),
        "StoreNotMatchRule"
    );
    assert_eq!(StatusCode::STORE_SNAPSHOT_THROTTLED.0, 200);
    assert_eq!(
        status_text(StatusCode::STORE_SNAPSHOT_THROTTLED),
        "StoreSnapshotThrottled"
    );
    assert_eq!(
        Status::new(StatusCode::STORE_SNAPSHOT_THROTTLED, &[]).to_string(),
        "StoreSnapshotThrottled"
    );
    assert_eq!(StatusCode::STORE_PENDING_PEER_THROTTLED.0, 201);
    assert_eq!(
        status_text(StatusCode::STORE_PENDING_PEER_THROTTLED),
        "StorePendingPeerThrottled"
    );
    assert_eq!(
        Status::new(StatusCode::STORE_PENDING_PEER_THROTTLED, &[]).to_string(),
        "StorePendingPeerThrottled"
    );
    assert_eq!(StatusCode::STORE_ADD_LIMIT_THROTTLED.0, 202);
    assert_eq!(
        status_text(StatusCode::STORE_ADD_LIMIT_THROTTLED),
        "StoreAddPeerThrottled"
    );
    assert_eq!(
        Status::new(StatusCode::STORE_ADD_LIMIT_THROTTLED, &[]).to_string(),
        "StoreAddPeerThrottled"
    );
    assert_eq!(StatusCode::STORE_REMOVE_LIMIT_THROTTLED.0, 203);
    assert_eq!(
        status_text(StatusCode::STORE_REMOVE_LIMIT_THROTTLED),
        "StoreRemovePeerThrottled"
    );
    assert_eq!(
        Status::new(StatusCode::STORE_REMOVE_LIMIT_THROTTLED, &[]).to_string(),
        "StoreRemovePeerThrottled"
    );
    assert_eq!(StatusCode::STORE_TRANSFER_LEADER_IN_LIMIT_THROTTLED.0, 204);
    assert_eq!(
        status_text(StatusCode::STORE_TRANSFER_LEADER_IN_LIMIT_THROTTLED),
        "StoreTransferLeaderInThrottled"
    );
    assert_eq!(
        Status::new(StatusCode::STORE_TRANSFER_LEADER_IN_LIMIT_THROTTLED, &[]).to_string(),
        "StoreTransferLeaderInThrottled"
    );
    assert_eq!(StatusCode::STORE_REJECT_LEADER.0, 300);
    assert_eq!(
        status_text(StatusCode::STORE_REJECT_LEADER),
        "StoreRejectLeader"
    );
    assert_eq!(
        Status::new(StatusCode::STORE_REJECT_LEADER, &[]).to_string(),
        "StoreRejectLeader"
    );
    assert_eq!(StatusCode::STORE_NOT_MATCH_ISOLATION.0, 301);
    assert_eq!(
        status_text(StatusCode::STORE_NOT_MATCH_ISOLATION),
        "StoreNotMatchIsolation"
    );
    assert_eq!(
        Status::new(StatusCode::STORE_NOT_MATCH_ISOLATION, &[]).to_string(),
        "StoreNotMatchIsolation"
    );
    assert_eq!(StatusCode::STORE_BUSY.0, 400);
    assert_eq!(status_text(StatusCode::STORE_BUSY), "StoreBusy");
    assert_eq!(
        Status::new(StatusCode::STORE_BUSY, &[]).to_string(),
        "StoreBusy"
    );
    assert_eq!(StatusCode::STORE_REMOVED.0, 401);
    assert_eq!(status_text(StatusCode::STORE_REMOVED), "StoreRemoved");
    assert_eq!(
        Status::new(StatusCode::STORE_REMOVED, &[]).to_string(),
        "StoreRemoved"
    );
    assert_eq!(StatusCode::STORE_REMOVING.0, 402);
    assert_eq!(status_text(StatusCode::STORE_REMOVING), "StoreRemoving");
    assert_eq!(
        Status::new(StatusCode::STORE_REMOVING, &[]).to_string(),
        "StoreRemoving"
    );
    assert_eq!(StatusCode::STORE_DOWN.0, 403);
    assert_eq!(status_text(StatusCode::STORE_DOWN), "StoreDown");
    assert_eq!(
        Status::new(StatusCode::STORE_DOWN, &[]).to_string(),
        "StoreDown"
    );
    assert_eq!(StatusCode::STORE_DISCONNECTED.0, 404);
    assert_eq!(
        status_text(StatusCode::STORE_DISCONNECTED),
        "StoreDisconnected"
    );
    assert_eq!(
        Status::new(StatusCode::STORE_DISCONNECTED, &[]).to_string(),
        "StoreDisconnected"
    );
    assert_eq!(StatusCode::STORE_LOW_SPACE.0, 500);
    assert_eq!(status_text(StatusCode::STORE_LOW_SPACE), "StoreLowSpace");
    assert_eq!(
        Status::new(StatusCode::STORE_LOW_SPACE, &[]).to_string(),
        "StoreLowSpace"
    );
    assert_eq!(StatusCode::STORE_NOT_EXISTED.0, 501);
    assert_eq!(
        status_text(StatusCode::STORE_NOT_EXISTED),
        "StoreNotExisted"
    );
    assert_eq!(
        Status::new(StatusCode::STORE_NOT_EXISTED, &[]).to_string(),
        "StoreNotExisted"
    );
    assert_eq!(StatusCode::REGION_HOT.0, 1000);
    assert_eq!(status_text(StatusCode::REGION_HOT), "RegionHot");
    assert_eq!(
        Status::new(StatusCode::REGION_HOT, &[]).to_string(),
        "RegionHot"
    );
    assert_eq!(StatusCode::REGION_UNHEALTHY.0, 1001);
    assert_eq!(status_text(StatusCode::REGION_UNHEALTHY), "RegionUnhealthy");
    assert_eq!(
        Status::new(StatusCode::REGION_UNHEALTHY, &[]).to_string(),
        "RegionUnhealthy"
    );
    assert_eq!(StatusCode::REGION_EMPTY.0, 1002);
    assert_eq!(status_text(StatusCode::REGION_EMPTY), "RegionEmpty");
    assert_eq!(
        Status::new(StatusCode::REGION_EMPTY, &[]).to_string(),
        "RegionEmpty"
    );
    assert_eq!(StatusCode::REGION_NOT_REPLICATED.0, 1003);
    assert_eq!(
        status_text(StatusCode::REGION_NOT_REPLICATED),
        "RegionNotReplicated"
    );
    assert_eq!(
        Status::new(StatusCode::REGION_NOT_REPLICATED, &[]).to_string(),
        "RegionNotReplicated"
    );
    assert_eq!(StatusCode::REGION_NOT_MATCH_RULE.0, 1004);
    assert_eq!(
        status_text(StatusCode::REGION_NOT_MATCH_RULE),
        "RegionNotMatchRule"
    );
    assert_eq!(
        Status::new(StatusCode::REGION_NOT_MATCH_RULE, &[]).to_string(),
        "RegionNotMatchRule"
    );
    assert_eq!(StatusCode::REGION_NO_LEADER.0, 1005);
    assert_eq!(status_text(StatusCode::REGION_NO_LEADER), "RegionNoLeader");
    assert_eq!(
        Status::new(StatusCode::REGION_NO_LEADER, &[]).to_string(),
        "RegionNoLeader"
    );
    assert_eq!(StatusCode::NO_TARGET_REGION.0, 1006);
    assert_eq!(status_text(StatusCode::NO_TARGET_REGION), "NoTargetRegion");
    assert_eq!(
        Status::new(StatusCode::NO_TARGET_REGION, &[]).to_string(),
        "NoTargetRegion"
    );
    assert_eq!(StatusCode::REGION_LABEL_REJECT.0, 1007);
    assert_eq!(
        status_text(StatusCode::REGION_LABEL_REJECT),
        "RegionLabelReject"
    );
    assert_eq!(
        Status::new(StatusCode::REGION_LABEL_REJECT, &[]).to_string(),
        "RegionLabelReject"
    );
    assert_eq!(StatusCode::REGION_SEND_SNAPSHOT_THROTTLED.0, 1008);
    assert_eq!(status_text(StatusCode::REGION_SEND_SNAPSHOT_THROTTLED), "");
    assert_eq!(
        Status::new(StatusCode::REGION_SEND_SNAPSHOT_THROTTLED, &[]).to_string(),
        ""
    );
    assert_eq!(StatusCode::REGION_AFFINITY.0, 1009);
    assert_eq!(status_text(StatusCode::REGION_AFFINITY), "RegionAffinity");
    assert_eq!(
        Status::new(StatusCode::REGION_AFFINITY, &[]).to_string(),
        "RegionAffinity"
    );
    assert_eq!(StatusCode::CREATE_OPERATOR_FAILED.0, 2000);
    assert_eq!(
        status_text(StatusCode::CREATE_OPERATOR_FAILED),
        "CreateOperatorFailed"
    );
    assert_eq!(
        Status::new(StatusCode::CREATE_OPERATOR_FAILED, &[]).to_string(),
        "CreateOperatorFailed"
    );
    for code in [-1, 99, 110, 999, 10000, i64::MAX] {
        assert_eq!(status_text(StatusCode(code)), "");
    }
}
#[test]
fn plan_status_reason_and_flags() {
    assert_eq!(Status::default(), Status::new(StatusCode::OK, &[]));
    for code in -110..=2100 {
        let s = Status::new(StatusCode(code), &["first: 原因", "ignored"]);
        assert_eq!(s.detailed_reason, "first: 原因");
        assert_eq!(s.is_ok(), code == 0);
        assert_eq!(s.is_normal(), (100..110).contains(&code));
    }
    assert_eq!(
        Status::new(StatusCode::OK, &["", "ignored"]).detailed_reason,
        ""
    );
}
#[test]
fn plan_status_priority_boundaries() {
    for (code, expected) in [
        (-101, -1.0),
        (-99, 0.0),
        (0, 0.0),
        (99, 0.0),
        (100, 1.0),
        (101, 1.01),
        (102, 1.02),
        (109, 1.09),
        (110, 1.1),
        (199, 1.99),
        (200, 2.0),
        (204, 2.0),
        (301, 3.0),
        (404, 4.0),
        (501, 5.0),
        (1008, 10.0),
        (1009, 10.0),
        (2000, 20.0),
    ] {
        assert_eq!(
            Status::new(StatusCode(code), &[]).priority(),
            expected,
            "code={code}"
        );
    }
}

#[test]
fn plan_status_runtime_go_oracle() {
    let mut count = 0;
    for line in include_str!("fixtures/status.tsv").lines() {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 5);
        let code = StatusCode(fields[0].parse().unwrap());
        let status = Status::new(code, &[]);
        assert_eq!(status.to_string(), fields[1], "{code:?}");
        assert_eq!(
            status.priority(),
            fields[2].parse::<f32>().unwrap(),
            "{code:?}"
        );
        assert_eq!(
            status.is_ok(),
            fields[3].parse::<bool>().unwrap(),
            "{code:?}"
        );
        assert_eq!(
            status.is_normal(),
            fields[4].parse::<bool>().unwrap(),
            "{code:?}"
        );
        assert_eq!(status.detailed_reason, "");
        count += 1;
    }
    assert_eq!(count, 38);
}
