// Copyright 2026 AsterSQL.
use crate::*;
use astersql_pd_core::{RegionInfo as CoreRegion, RegionMeta};
#[test]
fn core_task_64_empty_and_absent() {
    let r = CoreRegion::new(RegionMeta::default(), None, vec![], vec![]);
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_64.json")).unwrap();
    assert_eq!(
        serde_json::to_value(new_api_region_info(Some(&r))).unwrap(),
        expected["empty"]
    );
    assert!(new_api_region_info(None).is_none());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&marshal_region_info_json(None).unwrap())
            .unwrap(),
        expected["empty"]
    );
}
#[test]
fn core_task_64_full_and_reuse() {
    use astersql_pd_core::*;
    let p = Peer {
        id: 101,
        store_id: 1,
        role: PeerRole::Voter,
        is_witness: false,
    };
    let mut r = CoreRegion::new(
        RegionMeta {
            id: u64::MAX,
            key_range: KeyRange {
                start_key: vec![0, 255],
                end_key: vec![128],
            },
            epoch: Some(RegionEpoch {
                conf_ver: 1,
                version: 2,
            }),
            peers: vec![p.clone()],
            ..Default::default()
        },
        Some(p.clone()),
        vec![DownPeer {
            peer: p.clone(),
            down_seconds: 9,
        }],
        vec![p],
    );
    r.set_response_cpu_usage(77);
    r.set_replication_status(2, 1);
    r.set_heartbeat_stats(
        RegionHeartbeatStats {
            written_bytes: 1000,
            read_bytes: 2000,
            written_keys: 100,
            read_keys: 200,
            approximate_size: 500 * 1048576,
            approximate_keys: 1000,
            approximate_kv_size: 450 * 1048576,
            approximate_columnar_kv_size: 50 * 1048576,
            ..Default::default()
        },
        0,
    );
    r.set_bucket_meta(Some(RegionBucketMeta {
        version: 7,
        keys: vec![vec![], vec![255]],
    }));
    let mut api = new_api_region_info(Some(&r)).unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_64.json")).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&marshal_region_info_json(Some(&r)).unwrap())
            .unwrap(),
        expected["full"]
    );
    assert!(init_region(None, &mut api).is_none());
    assert_eq!(api.bucket_version, 7);
    let empty = CoreRegion::new(RegionMeta::default(), None, vec![], vec![]);
    init_region(Some(&empty), &mut api).unwrap();
    assert!(api.buckets.is_empty());
    assert_eq!(api.bucket_version, 7);
    assert!(api.replication_status.is_none());
}
#[test]
fn core_task_64_present_zero_and_unknown_replication() {
    let mut r = CoreRegion::new(RegionMeta::default(), None, vec![], vec![]);
    r.set_replication_status(0, 0);
    assert_eq!(
        new_api_region_info(Some(&r))
            .unwrap()
            .replication_status
            .unwrap()
            .state,
        "UNKNOWN"
    );
    r.set_replication_status(-7, u64::MAX);
    assert_eq!(
        new_api_region_info(Some(&r))
            .unwrap()
            .replication_status
            .unwrap()
            .state,
        "-7"
    );
}
#[test]
fn core_task_64_zero_epoch_signed_and_clone() {
    use astersql_pd_core::*;
    let mut r = CoreRegion::new(
        RegionMeta {
            epoch: Some(RegionEpoch::default()),
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    );
    r.set_response_cpu_usage(u64::MAX);
    r.set_replication_status(1, u64::MAX);
    r.set_heartbeat_stats(
        RegionHeartbeatStats {
            approximate_keys: u64::MAX,
            ..Default::default()
        },
        0,
    );
    let json = serde_json::to_value(new_api_region_info(Some(&r)).unwrap()).unwrap();
    assert_eq!(json["epoch"], serde_json::json!({}));
    assert_eq!(json["cpu_usage"], u64::MAX);
    assert_eq!(json["approximate_keys"], -1);
    assert_eq!(json["replication_status"]["state"], "SIMPLE_MAJORITY");
    assert_eq!(
        marshal_region_info_json(Some(&r)).unwrap(),
        marshal_region_info_json(Some(&r.clone_with([]))).unwrap()
    );
}

#[test]
fn core_task_68_single_region_cancellation() {
    // response/region.go checks ctx.Done() before conversion, even for absent input.
    use crate::{
        RegionListCancellation, RegionListError, marshal_region_info_json_with_cancellation,
    };
    for reason in [
        RegionListCancellation::Cancelled,
        RegionListCancellation::DeadlineExceeded,
    ] {
        let region = astersql_pd_core::RegionInfo::new(Default::default(), None, vec![], vec![]);
        assert!(
            matches!(marshal_region_info_json_with_cancellation(Some(&region), || Some(reason)), Err(RegionListError::Cancelled(r)) if r == reason)
        );
        assert!(
            matches!(marshal_region_info_json_with_cancellation(None, || Some(reason)), Err(RegionListError::Cancelled(r)) if r == reason)
        );
        assert_eq!(
            marshal_region_info_json_with_cancellation(None, || None).unwrap(),
            crate::marshal_region_info_json(None).unwrap()
        );
        assert_eq!(
            marshal_region_info_json_with_cancellation(Some(&region), || None).unwrap(),
            crate::marshal_region_info_json(Some(&region)).unwrap()
        );
    }
}
