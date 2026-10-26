// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    BasicCluster as CoreCluster, DownPeer, KeyRange, Peer, PeerRole, RegionInfo, RegionMeta,
    RegionOption, StoreInfo, StoreMeta,
};
use astersql_pd_core_constant::{Direction, PriorityLevel};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_core::BasicCluster;
use astersql_pd_schedule_operator::{OpKind, OpStep};
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use std::sync::Arc;
fn cluster() -> CoreCluster {
    let c = CoreCluster::new();
    for id in 1..=3 {
        let mut s = StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        });
        s.set_last_heartbeat_unix_nanos(1_000_000_000_000);
        c.stores.put_store(Arc::new(s));
    }
    c
}
#[tokio::test]
async fn task_077_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_077.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let mut c = cluster();
        let n = case["regions"].as_u64().unwrap_or(1);
        for id in 0..n {
            let peers: Vec<_> = (1..=3)
                .map(|s| Peer {
                    id: id * 10 + s,
                    store_id: s,
                    is_witness: s != 1 && case["witness"] == true,
                    role: if s != 1 && case["learner"] == true {
                        PeerRole::Learner
                    } else {
                        PeerRole::Voter
                    },
                    ..Default::default()
                })
                .collect();
            let ids = |field: &str| -> Vec<u64> {
                case[field]
                    .as_array()
                    .map(|a| a.iter().map(|v| v.as_u64().unwrap()).collect())
                    .unwrap_or_default()
            };
            let leader = if case["alternate"] == true {
                id % 2 + 1
            } else {
                1
            };
            let r = Arc::new(RegionInfo::new(
                RegionMeta {
                    id: id + 1,
                    key_range: KeyRange {
                        start_key: vec![b'a' + id as u8],
                        end_key: vec![b'b' + id as u8],
                    },
                    peers: peers.clone(),
                    ..Default::default()
                },
                Some(peers[leader as usize - 1].clone()),
                ids("down")
                    .iter()
                    .map(|s| DownPeer {
                        peer: peers[*s as usize - 1].clone(),
                        down_seconds: 1000,
                    })
                    .collect(),
                (if case["first_pending"] == true && id == 0 {
                    vec![2]
                } else {
                    ids("pending")
                })
                .iter()
                .map(|s| peers[*s as usize - 1].clone())
                .collect(),
            ));
            let (o, v, b) = c.regions.set_region(r.clone());
            c.regions.update_sub_tree(r, o.as_deref(), &v, b);
        }
        for (field, direction) in [("pause_in", Direction::IN), ("pause_out", Direction::OUT)] {
            if let Some(ids) = case[field].as_array() {
                for id in ids {
                    c.pause_leader_transfer(id.as_u64().unwrap(), direction)
                        .unwrap();
                }
            }
        }
        if case["missing_targets"] == true {
            c.stores = Default::default();
        }
        let ranges: Vec<KeyRange> = case["ranges"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|r| KeyRange {
                        start_key: r[0].as_str().unwrap().as_bytes().to_vec(),
                        end_key: r[1].as_str().unwrap().as_bytes().to_vec(),
                    })
                    .collect()
            })
            .unwrap_or_else(|| vec![KeyRange::default()]);
        let stores: Vec<u64> = case["stores"]
            .as_array()
            .map(|a| a.iter().map(|s| s.as_u64().unwrap()).collect())
            .unwrap_or_else(|| vec![3]);
        let cfg = GrantLeaderConfig::new(stores.iter().map(|s| (*s, ranges.clone())).collect());
        let scheduler = GrantLeaderScheduler::new(cfg);
        if case["prepare"] == true {
            scheduler.prepare(&c).unwrap();
            for id in &stores {
                assert!(!c.get_store(*id).unwrap().allow_leader_transfer_out());
                assert!(c.get_store(*id).unwrap().allow_leader_transfer_in());
            }
        }
        let config = ProviderSnapshot::<()>::default();
        let mut rules = astersql_pd_schedule_placement::RuleManager::new(
            Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
            ClusterPaths::new(75),
        );
        rules
            .initialize(3, &[], "", false, &config, &[])
            .await
            .unwrap();
        let mut draws = 0;
        let mut index = |bound: usize| {
            if case["cycle"] == true {
                let i = draws % bound;
                draws += 1;
                i
            } else if bound > 3 {
                let i = (draws / 10) % bound;
                draws += 1;
                i
            } else {
                0
            }
        };
        let mut shuffle = |_: &mut [usize]| {};
        let result = scheduler
            .schedule(&mut GrantLeaderContext {
                cluster: &c,
                config: &config,
                rules: &mut rules,
                now_unix_nanos: 1_000_000_000_000,
                random_index: &mut index,
                shuffle: &mut shuffle,
            })
            .await;
        assert_eq!(
            result.operators.len(),
            case["expected"].as_u64().unwrap() as usize,
            "{name}: errors={:?}",
            result.errors
        );
        assert_eq!(!result.errors.is_empty(), case["error"] == true, "{name}");
        if let Some(ids) = case["region_ids"].as_array() {
            assert_eq!(
                result
                    .operators
                    .iter()
                    .map(|op| op.region_id())
                    .collect::<Vec<_>>(),
                ids.iter().map(|v| v.as_u64().unwrap()).collect::<Vec<_>>(),
                "{name}"
            );
        }
        if !result.operators.is_empty() {
            let targets: Vec<_> = result
                .operators
                .iter()
                .map(|op| {
                    let OpStep::TransferLeader(step) = op.step(0).unwrap() else {
                        panic!("transfer expected")
                    };
                    step.to_store
                })
                .collect();
            assert_eq!(
                targets, stores,
                "{name}: every configured target receives one grant"
            );
        }
        if case["error"] == true {
            assert_eq!(
                result.errors,
                vec![astersql_pd_schedule_operator::BuilderError::LeaderNotAllowed],
                "{name}"
            );
        }
        for op in &result.operators {
            assert_eq!(op.kind(), OpKind::LEADER);
            assert_eq!(op.priority_level(), PriorityLevel::HIGH);
            assert_eq!(op.len(), 1);
            let OpStep::TransferLeader(step) = op.step(0).unwrap() else {
                panic!("transfer expected")
            };
            assert_eq!(step.from_store, 1, "{name}");
            assert!(stores.contains(&step.to_store), "{name}");
            assert!(step.to_stores.is_empty(), "{name}");
            let expected: Vec<u64> = vec![];
            let r = c.get_region(op.region_id()).unwrap();
            assert_eq!(step.conf_ver_changed(&r), 0);
            let influence = astersql_pd_schedule_operator::OpInfluence::new();
            step.influence(&influence, &r);
            assert_eq!(
                influence.get_store_influence(step.from_store).leader_count,
                -1
            );
            assert_eq!(influence.get_store_influence(step.to_store).leader_count, 1);
            for v2 in [false, true] {
                let command = step.get_cmd(&r, v2).transfer_leader.unwrap();
                assert_eq!(command.peer.as_ref().unwrap().store_id, step.to_store);
                assert_eq!(
                    command
                        .peers
                        .iter()
                        .map(|p| p.as_ref().unwrap().store_id)
                        .collect::<Vec<_>>(),
                    expected
                );
            }
            assert!(
                ranges
                    .iter()
                    .any(|range| r.meta().key_range.start_key >= range.start_key
                        && (range.end_key.is_empty()
                            || r.meta().key_range.end_key <= range.end_key))
            );
            assert!(op.start());
            assert!(op.check(&r).is_some());
            let applied = Arc::new(r.clone_with([RegionOption::Leader(
                r.get_store_peer(step.to_store).cloned(),
            )]));
            assert!(op.check(&applied).is_none());
            assert!(op.check_success());
            assert_eq!(applied.leader().unwrap().store_id, step.to_store);
        }
        if case["prepare"] == true {
            scheduler.clean(&c);
            for id in stores {
                assert!(c.get_store(id).unwrap().allow_leader_transfer_out());
            }
        }
        println!(
            "task_077 {name}: {} operators, {} errors",
            result.operators.len(),
            result.errors.len()
        );
    }
    println!(
        "task_077: {}/{} source-derived behavior cases passed",
        cases.len(),
        cases.len()
    );
}

#[test]
fn task_077_prepare_faults_and_limits() {
    let c = cluster();
    c.pause_leader_transfer(1, Direction::OUT).unwrap();
    c.pause_leader_transfer(1, Direction::IN).unwrap();
    let scheduler = GrantLeaderScheduler::new(GrantLeaderConfig::new(
        [(1, vec![KeyRange::default()]), (99, vec![])]
            .into_iter()
            .collect(),
    ));
    assert_eq!(
        scheduler.prepare(&c),
        Err(astersql_pd_core::StoreControlError::StoreNotFound(99))
    );
    assert!(!c.get_store(1).unwrap().allow_leader_transfer_out());
    scheduler.clean(&c);
    assert!(!c.get_store(1).unwrap().allow_leader_transfer_out());
    c.resume_leader_transfer(1, Direction::OUT);
    assert!(c.get_store(1).unwrap().allow_leader_transfer_out());
    assert!(!c.get_store(1).unwrap().allow_leader_transfer_in());
    assert!(GrantLeaderScheduler::is_schedule_allowed(2, 3));
    assert!(!GrantLeaderScheduler::is_schedule_allowed(3, 3));
    assert!(!GrantLeaderScheduler::is_schedule_allowed(0, 0));
}
