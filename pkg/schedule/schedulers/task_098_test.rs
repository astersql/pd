// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    BasicCluster, DownPeer, KeyRange, Peer, PeerRole, RegionInfo, RegionMeta, StoreInfo, StoreMeta,
};
use astersql_pd_core_constant::{KeyType, PriorityLevel};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_filter::RegionReplicationContext;
use astersql_pd_schedule_labeler::{LabelRule, RegionLabeler};
use astersql_pd_schedule_operator::{BuilderError, OpKind, OpStep};
use astersql_pd_schedule_placement::{Rule, RuleManager};
use astersql_pd_statistics::{HotPeerStat, StatisticsSnapshot};
use astersql_pd_storage_domain::MemoryStore;
use astersql_pd_storage_domain::StorageEndpoint;
use std::{cell::RefCell, sync::Arc};
#[tokio::test]
async fn task_098_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_098.json"
    ))
    .unwrap();
    let mut count = 0;
    for case in fixture["cases"].as_array().unwrap() {
        for v2 in [false, true] {
            let mut cluster = BasicCluster::new();
            for id in 1..=4 {
                let mut store = StoreInfo::new(StoreMeta {
                    id,
                    ..Default::default()
                });
                store.set_last_heartbeat_unix_nanos(if case["store_down"] == true {
                    1
                } else {
                    1_000_000_000_000
                });
                store.update_raw_stats(astersql_pd_core::StoreStats {
                    is_busy: case["busy"] == true,
                    ..Default::default()
                });
                cluster.stores.put_store(Arc::new(store));
            }
            let mut conf = ProviderSnapshot::<()>::default();
            conf.key_type = KeyType::RAW;
            conf.replication.max_replicas = if case["unreplicated"] == true { 4 } else { 3 };
            conf.replication.enable_placement_rules = case["rules"] == true;
            conf.schedule.enable_one_way_merge = case["one_way"] == true;
            let mut manager = RuleManager::new(
                Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
                ClusterPaths::new(98),
            );
            manager
                .initialize(3, &[], "", false, &conf, &[])
                .await
                .unwrap();
            for id in 1..=3 {
                if case["missing_next"] == true && id == 3 {
                    continue;
                }
                let fault = if id == 2 {
                    case["source_fault"].as_str().unwrap_or("")
                } else if id == 3 {
                    case["target_fault"].as_str().unwrap_or("")
                } else {
                    ""
                };
                let mut peers: Vec<_> = (1..=3)
                    .map(|s| Peer {
                        id: id * 10 + s,
                        store_id: if id == 3 && case["new_peer"] == true && s == 3 {
                            4
                        } else {
                            s
                        },
                        ..Default::default()
                    })
                    .collect();
                if fault == "learner" {
                    peers[2].role = PeerRole::Learner;
                }
                if fault == "joint" {
                    peers[2].role = PeerRole::IncomingVoter;
                }
                let mut region = RegionInfo::new(
                    RegionMeta {
                        id,
                        key_range: KeyRange {
                            start_key: vec![
                                b'a' + id as u8 - 1 + u8::from(id == 3 && case["gap"] == true),
                            ],
                            end_key: vec![
                                b'a' + id as u8 + u8::from(id == 3 && case["gap"] == true),
                            ],
                        },
                        peers: peers.clone(),
                        ..Default::default()
                    },
                    Some(peers[0].clone()),
                    if fault == "down" {
                        vec![DownPeer {
                            peer: peers[0].clone(),
                            down_seconds: 0,
                        }]
                    } else {
                        vec![]
                    },
                    if fault == "pending" {
                        vec![peers[0].clone()]
                    } else {
                        vec![]
                    },
                );
                region.stats_mut().approximate_size = 1000; // random merge has no size gate
                let r = Arc::new(region);
                let (o, v, b) = cluster.regions.set_region(r.clone());
                cluster.regions.update_sub_tree(r, o.as_deref(), &v, b);
            }
            if case["rule_split"] == true {
                manager
                    .set_rule(
                        Rule {
                            group_id: "pd".into(),
                            id: "split".into(),
                            role: astersql_pd_schedule_placement::PeerRoleType::Voter,
                            start_key_hex: "63".into(),
                            count: 3,
                            ..Default::default()
                        },
                        3,
                        &conf,
                        &[],
                    )
                    .await
                    .unwrap();
            }
            let mut labels = RegionLabeler::load(
                StorageEndpoint::new(Arc::new(MemoryStore::default())),
                ClusterPaths::new(98),
                0,
            )
            .await
            .unwrap();
            if case["label_deny"] == true || case["label_split"] == true {
                let rule=LabelRule::from_json(&serde_json::to_vec(&serde_json::json!({"id":"merge","labels":[{"key":"merge_option","value":if case["label_deny"]==true {"deny"} else {"allow"}}],"rule_type":"key-range","data":[{"start_key":if case["label_split"]==true {"63"} else {"62"},"end_key":"64"}]})).unwrap(),0).unwrap();
                labels.set_label_rule(rule, 0).await.unwrap();
            }
            let mut stats = StatisticsSnapshot::default();
            if let Some(id) = case["hot"].as_u64() {
                stats.read.insert(
                    1,
                    vec![HotPeerStat {
                        region_id: id,
                        store_id: 1,
                        hot_degree: 3,
                        ..Default::default()
                    }],
                );
            }
            let stores = cluster.stores.get_stores();
            let rep = RegionReplicationContext {
                conf: &conf,
                stores: &stores,
                manager: RefCell::new(&mut manager),
                now: 1_000_000_000_000,
            };
            // Restrict only the source. Neighbors outside the range remain valid targets.
            let ranges = if case["range_miss"] == true {
                vec![KeyRange {
                    start_key: b"x".to_vec(),
                    end_key: b"z".to_vec(),
                }]
            } else {
                vec![KeyRange {
                    start_key: b"b".to_vec(),
                    end_key: b"c".to_vec(),
                }]
            };
            let scheduler = RandomMergeScheduler::new(RandomMergeConfig::new(ranges));
            let mut random = |n: usize| if case["store_last"] == true { n - 1 } else { 0 };
            let mut allocations = 0;
            let result = scheduler
                .schedule(
                    &mut RandomMergeContext {
                        cluster: &cluster,
                        replication: &rep,
                        labels: Some(&labels),
                        statistics: &stats,
                        hot_degree: 3,
                        random_index: &mut random,
                        shuffle: &mut |_| {},
                        choose_previous: case["previous"] == true,
                        supports_conf_change_v2: v2,
                    },
                    || {
                        allocations += 1;
                        if case["allocation_fail"] == true {
                            Err(BuilderError::Allocation("injected".into()))
                        } else {
                            Ok(100 + allocations)
                        }
                    },
                )
                .await;
            let target = case["target"].as_u64().unwrap();
            assert_eq!(
                result
                    .as_ref()
                    .ok()
                    .and_then(|p| p.as_ref())
                    .map(|_| 2)
                    .unwrap_or(0),
                if target > 0 { 2 } else { 0 },
                "{case}: {:?}",
                result.as_ref().err()
            );
            assert_eq!(result.is_err(), case["error"] == true, "{case}");
            if case["allocation_fail"] == true {
                assert!(matches!(result, Err(BuilderError::Allocation(_))));
                assert_eq!(allocations, 1);
            } else if case["error"] == true {
                assert!(matches!(result, Err(BuilderError::OriginJoint)));
            }
            if let Ok(Some(pair)) = result {
                for (i, op) in pair.iter().enumerate() {
                    assert_eq!(op.desc(), "random-merge-scheduler");
                    assert!(op.kind().0 & OpKind::MERGE.0 != 0);
                    assert_eq!(op.priority_level(), PriorityLevel::LOW);
                    assert_eq!(op.region_id(), if i == 0 { 2 } else { target });
                    let OpStep::MergeRegion(step) = op.step(op.len() - 1).unwrap() else {
                        panic!("missing merge")
                    };
                    assert_eq!(step.is_passive, i == 1);
                    assert_eq!(step.from_region.as_ref().unwrap().id, 2);
                    assert_eq!(step.to_region.as_ref().unwrap().id, target);
                    let r = cluster.regions.get_region(op.region_id()).unwrap();
                    assert_eq!(step.conf_ver_changed(&r), 0);
                    assert_eq!(step.get_cmd(&r, v2).is_some(), i == 0);
                    if let Some(cmd) = step.get_cmd(&r, v2) {
                        assert_eq!(cmd.merge.unwrap().target.as_ref(), step.to_region.as_ref());
                    }
                    let influence = astersql_pd_schedule_operator::OpInfluence::new();
                    step.influence(&influence, &r);
                    for store in 1..=3 {
                        let got = influence.get_store_influence(store);
                        assert_eq!(
                            got.region_count,
                            if i == 1 && r.get_store_peer(store).is_some() {
                                -1
                            } else {
                                0
                            }
                        );
                        assert_eq!(got.leader_count, if i == 1 && store == 1 { -1 } else { 0 });
                    }
                    assert!(!step.is_finish(&r));
                    let mut finished_meta = r.meta().clone();
                    finished_meta.key_range.start_key = b"a".to_vec();
                    finished_meta.key_range.end_key = b"d".to_vec();
                    let finished =
                        RegionInfo::new(finished_meta, r.leader().cloned(), vec![], vec![]);
                    assert_eq!(step.is_finish(&finished), i == 1);
                    if case["new_peer"] != true {
                        assert_eq!(op.len(), 1);
                        assert_eq!(op.kind(), OpKind::MERGE);
                    }
                }
            }
            count += 1;
        }
    }
    println!("task_098 effective behavior cases: {count}");
}
#[test]
fn task_098_config_and_limits() {
    let c = RandomMergeConfig::from_args(&["a", "z"]).unwrap();
    let s = RandomMergeScheduler::new(c);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&s.encode_config().unwrap()).unwrap(),
        serde_json::json!({"ranges":[{"start-key":"61","end-key":"7a"}]})
    );
    assert_eq!(
        serde_json::from_str::<RandomMergeConfig>(r#"{"ranges":null}"#)
            .unwrap()
            .ranges(),
        vec![]
    );
    assert_eq!(
        RandomMergeConfig::from_args(&[]).unwrap().ranges(),
        vec![KeyRange::default()]
    );
    assert!(RandomMergeConfig::from_args(&["%zz", ""]).is_err());
    assert!(
        serde_json::from_str::<RandomMergeConfig>(r#"{"ranges":[{"start-key":"Z"}]}"#).is_err()
    );
    for (count, limit, want) in [
        (0, 0, false),
        (0, 1, true),
        (1, 1, false),
        (2, 1, false),
        (u64::MAX - 1, u64::MAX, true),
    ] {
        assert_eq!(
            RandomMergeScheduler::is_schedule_allowed(count, limit),
            want
        );
    }
}
