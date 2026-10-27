// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{Peer, PeerRole, RegionEpoch, RegionInfo, RegionMeta, StoreInfo, StoreMeta};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_placement::{PeerRoleType, Rule, RuleManager};
use astersql_pd_storage_domain::MemoryStore;
use astersql_pd_storage_domain::StorageEndpoint;
use serde_json::Value;
use std::sync::Arc;
fn region(id: u64, peers: u64, learners: u64, epoch: u64) -> RegionInfo {
    RegionInfo::new(
        RegionMeta {
            id,
            epoch: Some(RegionEpoch {
                conf_ver: epoch,
                version: epoch,
            }),
            peers: (1..=peers)
                .map(|id| Peer {
                    id,
                    store_id: id,
                    role: if id > peers - learners {
                        PeerRole::Learner
                    } else {
                        PeerRole::Voter
                    },
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    )
}
#[tokio::test]
async fn task_051_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_051.json"
    ))
    .unwrap();
    let mut count = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let mut conf = ProviderSnapshot::<()>::default();
        conf.replication.max_replicas = 3;
        conf.replication.enable_placement_rules = case["rules"].as_bool().unwrap();
        conf.schedule.patrol_region_interval = "1ns".parse().unwrap();
        let mut manager = RuleManager::new(
            Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
            ClusterPaths::new(51),
        );
        manager
            .initialize(3, &[], "", false, &conf, &[])
            .await
            .unwrap();
        if let Some(n) = case["learner_rule"].as_i64() {
            manager
                .set_rule(
                    Rule {
                        group_id: "test".into(),
                        id: "learner".into(),
                        role: PeerRoleType::Learner,
                        count: n,
                        ..Default::default()
                    },
                    3,
                    &conf,
                    &[],
                )
                .await
                .unwrap();
        }
        if case["empty_fit"] == true {
            manager
                .set_rule(
                    Rule {
                        group_id: "pd".into(),
                        id: "default".into(),
                        role: PeerRoleType::Voter,
                        count: 3,
                        start_key_hex: "61".into(),
                        ..Default::default()
                    },
                    3,
                    &conf,
                    &[],
                )
                .await
                .unwrap();
        }
        let stores: Vec<_> = (1..=6)
            .map(|id| {
                Arc::new(StoreInfo::new(StoreMeta {
                    id,
                    ..Default::default()
                }))
            })
            .collect();
        let inspector =
            PriorityInspector::with_capacity(case["capacity"].as_i64().unwrap_or(1280) as isize);
        for action in case["actions"].as_array().unwrap() {
            count += 1;
            let name = format!("{} action {}", case["name"], count);
            let now = action["now"].as_i64().unwrap_or(0) as i128;
            let id = action["id"].as_u64().unwrap_or(0);
            match action["op"].as_str().unwrap() {
                "inspect" => {
                    let region = region(
                        id,
                        action["peers"].as_u64().unwrap(),
                        action["learners"].as_u64().unwrap_or(0),
                        action["epoch"].as_u64().unwrap_or(1),
                    );
                    let fit = inspector.inspect(&conf, &mut manager, &stores, &region, now);
                    assert_eq!(
                        fit.is_some(),
                        conf.replication.enable_placement_rules,
                        "{name}"
                    );
                    if case["empty_fit"] == true {
                        assert!(fit.unwrap().rule_fits.is_empty());
                    }
                }
                "remove" => inspector.remove_priority_region(id),
                "query" => {
                    let expected: Vec<_> = action["ids"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_u64().unwrap())
                        .collect();
                    assert_eq!(
                        inspector.get_priority_regions(&conf, now),
                        expected,
                        "{name}"
                    );
                }
                _ => panic!("unknown fixture action"),
            }
            if let Some(len) = action["len"].as_u64() {
                assert_eq!(inspector.len(), len as usize, "{name}");
                assert_eq!(inspector.is_empty(), len == 0, "{name}");
            }
            if let Some(expected) = action["entry"].as_array() {
                let (priority, entry) = inspector.entry(id).expect(&name);
                assert_eq!(
                    (priority, entry.attempt, entry.last_unix_nanos),
                    (
                        expected[0].as_i64().unwrap(),
                        expected[1].as_u64().unwrap(),
                        expected[2].as_i64().unwrap() as i128
                    ),
                    "{name}"
                );
                assert_eq!(entry.region_id, id);
            }
            if action["absent"] == true {
                assert!(inspector.entry(id).is_none(), "{name}");
            }
        }
    }
    println!(
        "task_051: {} scenarios, {count} action cases passed",
        fixture["cases"].as_array().unwrap().len()
    );
}
#[tokio::test]
async fn task_051_concurrent_removal() {
    let inspector = Arc::new(PriorityInspector::new());
    let mut conf = ProviderSnapshot::<()>::default();
    conf.replication.max_replicas = 3;
    let mut manager = RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        ClusterPaths::new(51),
    );
    inspector.inspect(&conf, &mut manager, &[], &region(51, 1, 0, 1), 0);
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let inspector = inspector.clone();
            std::thread::spawn(move || {
                for _ in 0..100 {
                    inspector.remove_priority_region(51);
                    let _ = inspector.entry(51);
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert!(inspector.is_empty());
    assert!(inspector.entry(51).is_none());
}
