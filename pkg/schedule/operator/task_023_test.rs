// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    BasicCluster, KeyRange, Peer, PeerRole, RegionEpoch, RegionInfo, RegionMeta,
};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_hbstream::{Merge, Operation, SplitPolicy, SplitRegion as SplitCommand};
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_023.json"
    ))
    .unwrap()
}
fn bytes(v: &serde_json::Value) -> Vec<u8> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_u64().unwrap() as u8)
        .collect()
}
fn target() -> RegionMeta {
    RegionMeta {
        id: 42,
        key_range: KeyRange {
            start_key: vec![10],
            end_key: vec![20],
        },
        epoch: Some(RegionEpoch {
            version: 7,
            conf_ver: 8,
        }),
        peers: vec![
            Peer {
                id: 11,
                store_id: 1,
                ..Default::default()
            },
            Peer {
                id: 12,
                store_id: 2,
                role: PeerRole::Learner,
                is_witness: true,
            },
        ],
        ..Default::default()
    }
}
fn steps(passive: bool) -> (MergeRegion, SplitRegion) {
    (
        MergeRegion {
            from_region: Some(RegionMeta {
                id: 41,
                ..Default::default()
            }),
            to_region: Some(target()),
            is_passive: passive,
        },
        SplitRegion {
            start_key: vec![10],
            end_key: vec![20],
            policy: SplitPolicy::UseKey,
            split_keys: vec![vec![255, 0], vec![], vec![255, 0], vec![10]],
        },
    )
}
#[test]
fn task_023_behavior_cases() {
    let f = fixture();
    let cluster = BasicCluster::default();
    let config: ProviderSnapshot<()> = ProviderSnapshot::default();
    for c in f["cases"].as_array().unwrap() {
        let mut meta = target();
        meta.key_range = KeyRange {
            start_key: bytes(&c["start"]),
            end_key: bytes(&c["end"]),
        };
        meta.epoch = Some(RegionEpoch {
            version: c["version"].as_u64().unwrap(),
            conf_ver: 100,
        });
        let r = RegionInfo::new(meta, None, vec![], vec![]);
        for passive in [false, true] {
            let (m, s) = steps(passive);
            assert_eq!(
                m.is_finish(&r),
                passive && c["finished"].as_bool().unwrap(),
                "{} passive={passive}",
                c["name"]
            );
            assert_eq!(
                s.is_finish(&r),
                c["finished"].as_bool().unwrap(),
                "{}",
                c["name"]
            );
            assert_eq!(m.conf_ver_changed(&r), 0);
            assert_eq!(s.conf_ver_changed(&r), 0);
            for health in [false, true] {
                assert_eq!(
                    m.check_in_progress(&cluster, &config, &r, health, 0),
                    Ok(())
                );
                assert_eq!(
                    s.check_in_progress(&cluster, &config, &r, health, 0),
                    Ok(())
                );
            }
        }
    }
    let r = RegionInfo::new(RegionMeta::default(), None, vec![], vec![]);
    assert!(!SplitRegion::default().is_finish(&r));
    assert!(!MergeRegion::default().is_finish(&r));
    assert_eq!(
        MergeRegion::default().get_cmd(&r, false),
        Some(Operation {
            merge: Some(Merge { target: None }),
            ..Default::default()
        })
    );
    let mut meta = target();
    meta.id = 999;
    meta.epoch = None;
    let identity_only = RegionInfo::new(meta, None, vec![], vec![]);
    assert!(!steps(true).0.is_finish(&identity_only));
    assert!(!steps(true).1.is_finish(&identity_only));
    let missing_target = MergeRegion {
        is_passive: true,
        ..Default::default()
    };
    assert!(std::panic::catch_unwind(|| missing_target.is_finish(&r)).is_err());
    assert_eq!(missing_target.get_cmd(&r, true), None);
    assert_eq!(
        MergeRegion::default().to_string(),
        "merge region 0 into region 0"
    );
}
#[test]
fn task_023_commands_and_timeout() {
    let r = RegionInfo::new(target(), None, vec![], vec![]);
    for passive in [false, true] {
        let (m, mut s) = steps(passive);
        assert_eq!(m.to_string(), "merge region 41 into region 42");
        for v2 in [false, true] {
            assert_eq!(
                m.get_cmd(&r, v2),
                if passive {
                    None
                } else {
                    Some(Operation {
                        merge: Some(Merge {
                            target: Some(target()),
                        }),
                        ..Default::default()
                    })
                }
            );
            for (policy, name) in [
                (SplitPolicy::Scan, "SCAN"),
                (SplitPolicy::Approximate, "APPROXIMATE"),
                (SplitPolicy::UseKey, "USEKEY"),
                (SplitPolicy::Unknown(-7), "-7"),
            ] {
                s.policy = policy;
                assert_eq!(s.to_string(), format!("split region with policy {name}"));
                assert_eq!(
                    s.get_cmd(&r, v2),
                    Operation {
                        split_region: Some(SplitCommand {
                            policy,
                            keys: vec![vec![255, 0], vec![], vec![255, 0], vec![10]]
                        }),
                        ..Default::default()
                    }
                );
            }
            assert_eq!(
                SplitRegion::default().get_cmd(&r, v2),
                Operation {
                    split_region: Some(SplitCommand::default()),
                    ..Default::default()
                }
            );
        }
        for c in fixture()["timeouts"].as_array().unwrap() {
            let size = c["size"].as_i64().unwrap();
            assert_eq!(m.timeout_nanos(size), c["merge_ns"].as_i64().unwrap());
            assert_eq!(
                m.timeout(size).as_nanos(),
                c["merge_ns"].as_i64().unwrap() as u64 as u128
            );
            assert_eq!(
                s.timeout(size).as_nanos(),
                c["split_ns"].as_i64().unwrap() as u128
            );
        }
    }
}
#[test]
fn task_023_influence_counts_and_fault_inputs() {
    // All peers count, including learners/witnesses and duplicate stores. Leader
    // identity uses peer ID, not store ID; absent leader has protobuf ID zero.
    for leader in [
        Some(Peer {
            id: 11,
            store_id: 999,
            ..Default::default()
        }),
        None,
    ] {
        let mut meta = target();
        meta.peers.push(Peer {
            id: 0,
            store_id: 2,
            ..Default::default()
        });
        let mut r = RegionInfo::new(meta, leader.clone(), vec![], vec![]);
        r.stats_mut().approximate_size = 123;
        for passive in [false, true] {
            let (m, s) = steps(passive);
            let inf = OpInfluence::new();
            for id in [1, 2] {
                inf.update_store(id, |v| {
                    v.region_size = 900;
                    v.leader_size = 800;
                    v.region_count = 10;
                    v.leader_count = 20;
                    v.witness_count = 3;
                });
            }
            let before = inf.snapshot();
            m.influence(&inf, &r);
            for id in [1, 2] {
                let mut expected = before[&id].clone();
                if passive {
                    expected.region_count -= if id == 1 { 1 } else { 2 };
                    expected.leader_count -=
                        i64::from((leader.is_some() && id == 1) || (leader.is_none() && id == 2));
                }
                assert_eq!(inf.get_store_influence(id), expected);
            }
            let before = inf.snapshot();
            s.influence(&inf, &r);
            for id in [1, 2] {
                let mut expected = before[&id].clone();
                expected.region_count += if id == 1 { 1 } else { 2 };
                expected.leader_count +=
                    i64::from((leader.is_some() && id == 1) || (leader.is_none() && id == 2));
                assert_eq!(inf.get_store_influence(id), expected);
            }
        }
    }
    let r = RegionInfo::new(RegionMeta::default(), None, vec![], vec![]);
    let inf = OpInfluence::new();
    SplitRegion::default().influence(&inf, &r);
    steps(true).0.influence(&inf, &r);
    assert!(inf.snapshot().is_empty());
    let r = RegionInfo::new(target(), None, vec![], vec![]);
    inf.update_store(1, |s| s.region_count = i64::MAX);
    SplitRegion::default().influence(&inf, &r);
    assert_eq!(inf.get_store_influence(1).region_count, i64::MIN);
    steps(true).0.influence(&inf, &r);
    assert_eq!(inf.get_store_influence(1).region_count, i64::MAX);
}
