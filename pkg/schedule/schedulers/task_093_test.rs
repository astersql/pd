// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{KeyRange, Peer, PeerRole, RegionBuckets, RegionInfo, RegionMeta};
use astersql_pd_schedule_hbstream::SplitPolicy;
use astersql_pd_schedule_operator::{BuilderError, OpKind, OpStep};
use astersql_pd_statistics_buckets::BucketStat;
use astersql_pd_statistics_utils::{RWType, RegionStatKind};
use serde_json::Value;
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_093.json"
    ))
    .unwrap()
}
fn region() -> RegionInfo {
    RegionInfo::new(
        RegionMeta {
            id: 7,
            key_range: KeyRange {
                start_key: b"a".to_vec(),
                end_key: b"f".to_vec(),
            },
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    )
}
fn split(rw: RWType, dims: [usize; 2]) -> HotSplit {
    HotSplit::new(
        HotRegionParams::default(),
        dims,
        rw,
        HotSourceOperation::MovePeer,
    )
    .unwrap()
}
fn buckets(row: &Value, dim: usize) -> Vec<BucketStat> {
    let keys = row["keys"].as_array().unwrap();
    keys.windows(2)
        .enumerate()
        .map(|(i, k)| {
            let mut loads = [0; 6];
            loads[dim] = row["loads"][i].as_u64().unwrap();
            BucketStat {
                region_id: 7,
                start_key: k[0].as_str().unwrap().as_bytes().to_vec(),
                end_key: k[1].as_str().unwrap().as_bytes().to_vec(),
                hot_degree: 1,
                interval: 1000,
                loads,
            }
        })
        .collect()
}
fn assert_key(op: Option<astersql_pd_schedule_operator::Operator>, row: &Value, rw: RWType) {
    if row["expected"].is_null() {
        assert!(op.is_none(), "{row}");
        return;
    }
    let op = op.unwrap();
    assert_eq!(op.kind(), OpKind::SPLIT);
    assert_eq!(
        op.desc(),
        if rw == RWType::READ {
            "split-hot-read-region"
        } else {
            "split-hot-write-region"
        }
    );
    assert_eq!(op.region_id(), 7);
    let OpStep::SplitRegion(step) = op.step(0).unwrap() else {
        panic!("split step")
    };
    assert_eq!(step.policy, SplitPolicy::UseKey);
    assert_eq!(step.start_key, b"a");
    assert_eq!(step.end_key, b"f");
    assert_eq!(
        step.split_keys,
        vec![row["expected"].as_str().unwrap().as_bytes().to_vec()]
    );
    if !row["total"].is_null() {
        assert_eq!(
            op.additional_info().get("totalLoads"),
            Some(row["total"].to_string())
        );
        assert_eq!(
            op.additional_info().get("accLoads"),
            Some(row["acc"].to_string())
        );
    }
}
#[test]
fn task_093_behavior_cases() {
    let f = fixture();
    let mut r = region();
    let s = split(RWType::READ, [0, 1]);
    for row in f["size"].as_array().unwrap() {
        r = region();
        assert!(
            r.update_buckets(
                Some(&RegionBuckets {
                    keys: row["keys"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|k| k.as_str().unwrap().as_bytes().to_vec())
                        .collect(),
                    ..Default::default()
                }),
                None,
            )
        );
        assert_key(s.by_size(&r).unwrap(), row, RWType::READ);
    }
    for rw in [RWType::READ, RWType::WRITE] {
        for dims in [[0, 1], [3, 0]] {
            let s = split(rw, dims);
            let dim = s.bucket_first_stat().as_i32() as usize;
            for row in f["load"].as_array().unwrap() {
                assert_key(s.by_load(&r, &buckets(row, dim)).unwrap(), row, rw);
            }
        }
    }
    for row in f["dimensions"].as_array().unwrap() {
        let vals: Vec<_> = row
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as usize)
            .collect();
        assert_eq!(
            bucket_first_stat([vals[0], vals[1]], RWType::from_i32(vals[2] as i32)).unwrap(),
            RegionStatKind::from_i32(vals[3] as i32)
        );
    }
}
#[test]
fn task_093_threshold_and_gates() {
    for (version, strict, op, peer, expected) in [
        (
            "v2",
            true,
            HotSourceOperation::MovePeer,
            [20., 21., 0., 0.],
            true,
        ),
        (
            "v2",
            true,
            HotSourceOperation::MovePeer,
            [20., 20., 0., 0.],
            false,
        ),
        (
            "v1",
            true,
            HotSourceOperation::MovePeer,
            [20., 21., 0., 0.],
            false,
        ),
        (
            "v1",
            true,
            HotSourceOperation::MovePeer,
            [21., 21., 0., 0.],
            true,
        ),
        (
            "v1",
            false,
            HotSourceOperation::MovePeer,
            [21., 0., 0., 0.],
            true,
        ),
        (
            "v2",
            true,
            HotSourceOperation::TransferLeader,
            [20., 21., 0., 0.],
            false,
        ),
    ] {
        let rw = if op == HotSourceOperation::TransferLeader {
            RWType::WRITE
        } else {
            RWType::READ
        };
        let s = HotSplit::new(
            HotRegionParams {
                rank_formula_version: version.into(),
                strict_picking_store: strict,
                ..Default::default()
            },
            [0, 1],
            rw,
            op,
        )
        .unwrap();
        assert_eq!(
            s.too_hot_need_split(&[100.; 4], &peer),
            expected,
            "{version}/{strict}/{op:?}"
        );
    }
    let s = split(RWType::READ, [0, 1]);
    let mut r = region();
    r.set_bucket_meta(Some(astersql_pd_core::RegionBucketMeta {
        version: 1,
        keys: vec![b"b".to_vec(), b"c".to_vec()],
    }));
    r.stats_mut().approximate_size = 101;
    assert!(s.before_move(false, 100, &[&r]).unwrap().is_none());
    let result = s.before_move(true, 100, &[&r]).unwrap().unwrap();
    assert_eq!(result.progressive_rank, 5);
    assert_eq!(result.operators.len(), 1);
    assert!(s.before_move(true, 101, &[&r]).unwrap().is_none());
    let row = &fixture()["load"][0];
    let stats = buckets(row, 0);
    assert!(
        s.for_hot_region(false, &r, &[100.; 4], &[30., 0., 0., 0.], &stats)
            .unwrap()
            .is_none()
    );
    assert!(
        s.for_hot_region(true, &r, &[100.; 4], &[20.; 4], &stats)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        s.for_hot_region(true, &r, &[100.; 4], &[30., 0., 0., 0.], &stats)
            .unwrap()
            .unwrap()
            .progressive_rank,
        5
    );
    // A required size split with no valid keys suppresses move fallback.
    r.set_bucket_meta(None);
    let result = s.before_move(true, 100, &[&r]).unwrap().unwrap();
    assert!(result.operators.is_empty());
    assert_eq!(result.progressive_rank, 0);
}
#[test]
fn task_093_errors_and_boundaries() {
    assert!(matches!(
        HotSplit::new(
            HotRegionParams::default(),
            [4, 0],
            RWType::READ,
            HotSourceOperation::MovePeer
        ),
        Err(HotSplitError::InvalidPriorities)
    ));
    assert!(bucket_first_stat([0, 1], RWType::from_i32(4)).is_err());
    let s = split(RWType::READ, [0, 1]);
    let mut r = region();
    assert!(s.by_size(&r).unwrap().is_none());
    assert!(s.by_load(&r, &[]).unwrap().is_none());
    let mut stats = buckets(&fixture()["load"][0], 0);
    stats[0].start_key = b"x".to_vec();
    stats[0].end_key = b"z".to_vec();
    assert!(s.by_load(&r, &stats).unwrap().is_none());
    r = RegionInfo::new(
        RegionMeta {
            id: 7,
            key_range: r.meta().key_range.clone(),
            peers: vec![Peer {
                id: 1,
                store_id: 1,
                role: PeerRole::IncomingVoter,
                ..Default::default()
            }],
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    );
    let err = s.by_load(&r, &buckets(&fixture()["load"][0], 0));
    assert!(matches!(
        err,
        Err(HotSplitError::Builder(BuilderError::OriginJoint))
    ));
    // uint64 counters wrap, matching Go even in debug Rust builds.
    let mut stats = buckets(&fixture()["load"][0], 0);
    stats[0].loads[0] = u64::MAX;
    stats[1].loads[0] = 1;
    assert!(s.by_load(&region(), &stats).unwrap().is_none());
}

#[test]
fn task_093_real_bucket_report_and_reverse_size() {
    use astersql_pd_statistics_buckets::{BucketFlows, BucketReport, convert_to_bucket_tree_item};
    let r = region();
    // CPU fallback chooses bytes (d), while query and key priorities choose b.
    let report = BucketReport {
        region_id: 7,
        keys: vec![vec![], b"b".to_vec(), b"d".to_vec(), vec![]],
        period_in_ms: 1000,
        flows: BucketFlows {
            read_bytes: vec![2, 2, 20],
            read_keys: vec![1000, 1, 1],
            read_qps: vec![1000, 1, 1],
            write_bytes: vec![2, 2, 20],
            write_keys: vec![1000, 1, 1],
            write_qps: vec![1000, 1, 1],
        },
        ..Default::default()
    };
    let item = convert_to_bucket_tree_item(&report);
    for rw in [RWType::READ, RWType::WRITE] {
        for (dims, key) in [([3, 0], "d"), ([1, 0], "b"), ([2, 0], "b")] {
            assert_key(
                split(rw, dims).by_load(&r, &item.stats).unwrap(),
                &serde_json::json!({"expected":key}),
                rw,
            );
        }
    }
    let mut oversized = region();
    oversized.stats_mut().approximate_size = 1000;
    oversized.set_bucket_meta(Some(astersql_pd_core::RegionBucketMeta {
        version: 1,
        keys: vec![b"b".to_vec()],
    }));
    let s = split(RWType::READ, [0, 1]);
    let result = s
        .before_move(true, 100, &[&r, &oversized])
        .unwrap()
        .unwrap();
    assert_eq!(result.operators.len(), 1);
    assert_eq!(result.progressive_rank, 5);
    let leader = HotSplit::new(
        HotRegionParams::default(),
        [0, 1],
        RWType::READ,
        HotSourceOperation::TransferLeader,
    )
    .unwrap();
    assert!(
        leader
            .before_move(true, 100, &[&oversized])
            .unwrap()
            .is_none()
    );
    // Binary and infinite end keys preserve canonical key ordering.
    let mut unbounded = RegionInfo::new(
        RegionMeta {
            id: 7,
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    );
    unbounded.set_bucket_meta(Some(astersql_pd_core::RegionBucketMeta {
        version: 1,
        keys: vec![vec![], vec![0], vec![255], vec![]],
    }));
    let op = s.by_size(&unbounded).unwrap().unwrap();
    let OpStep::SplitRegion(step) = op.step(0).unwrap() else {
        panic!("split")
    };
    assert_eq!(step.split_keys, vec![vec![255]]);
}

#[test]
fn task_093_batch_builder_failure_keeps_other_splits() {
    let mut good = region();
    good.stats_mut().approximate_size = 1000;
    good.set_bucket_meta(Some(astersql_pd_core::RegionBucketMeta {
        version: 1,
        keys: vec![b"b".to_vec()],
    }));
    let mut bad = RegionInfo::new(
        RegionMeta {
            id: 8,
            key_range: good.meta().key_range.clone(),
            peers: vec![Peer {
                id: 1,
                store_id: 1,
                role: PeerRole::IncomingVoter,
                ..Default::default()
            }],
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    );
    bad.stats_mut().approximate_size = 1000;
    bad.set_bucket_meta(Some(astersql_pd_core::RegionBucketMeta {
        version: 1,
        keys: vec![b"b".to_vec()],
    }));
    let s = split(RWType::READ, [0, 1]);
    for regions in [[&good, &bad], [&bad, &good]] {
        let result = s.before_move(true, 100, &regions).unwrap().unwrap();
        assert_eq!(result.operators.len(), 1);
        assert_eq!(result.operators[0].region_id(), 7);
        assert_eq!(result.progressive_rank, 5);
        assert_eq!(
            result.errors,
            vec![(8, HotSplitError::Builder(BuilderError::OriginJoint))]
        );
    }
    let result = s.before_move(true, 100, &[&bad]).unwrap().unwrap();
    assert!(result.operators.is_empty());
    assert_eq!(result.errors.len(), 1);
    assert_eq!(result.progressive_rank, 0);
}
