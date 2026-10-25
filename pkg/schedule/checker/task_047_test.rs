// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    DownPeer, KeyRange, Peer, PeerRole, RegionEpoch, RegionInfo, RegionMeta, StoreInfo, StoreMeta,
    StoresInfo,
};
use astersql_pd_core_constant::{KeyType, PriorityLevel};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_filter::RegionReplicationContext;
use astersql_pd_schedule_operator::{BuilderError, OpInfluence, OpKind, OpStep};
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeSet, sync::Arc};
fn region(id: u64, start: u8, size: i64, keys: i64, fault: &str, new_peer: bool) -> RegionInfo {
    let mut peers: Vec<_> = (1..=3)
        .map(|s| Peer {
            id: id * 10 + s,
            store_id: if new_peer && s == 3 { 4 } else { s },
            ..Default::default()
        })
        .collect();
    if fault == "learner" {
        peers[2].role = PeerRole::Learner;
    }
    let down = if fault == "down" {
        vec![DownPeer {
            peer: peers[0].clone(),
            down_seconds: 0,
        }]
    } else {
        vec![]
    };
    let pending = if fault == "pending" {
        vec![peers[0].clone()]
    } else {
        vec![]
    };
    let leader = if fault == "no-leader" {
        None
    } else {
        Some(peers[0].clone())
    };
    if fault == "missing" {
        peers.pop();
    }
    let mut r = RegionInfo::new(
        RegionMeta {
            id,
            key_range: KeyRange {
                start_key: vec![start],
                end_key: vec![start + 1],
            },
            peers,
            epoch: Some(RegionEpoch {
                conf_ver: 7,
                version: 9,
            }),
            ..Default::default()
        },
        leader,
        down,
        pending,
    );
    r.stats_mut().approximate_size = size;
    r.stats_mut().approximate_keys = keys;
    r
}
fn number(c: &Value, k: &str, d: i64) -> i64 {
    c[k].as_i64().unwrap_or(d)
}
async fn evaluate(
    checker: &mut MergeChecker,
    c: &Value,
    now: i128,
) -> Result<Option<[astersql_pd_schedule_operator::Operator; 2]>, BuilderError> {
    let mut conf = ProviderSnapshot::<()>::default();
    conf.key_type = KeyType::RAW;
    conf.replication.max_replicas = 3;
    conf.replication.enable_placement_rules = c["rules"] == true;
    conf.schedule.max_merge_region_size = number(c, "max_size", 2) as u64;
    conf.schedule.max_merge_region_keys = number(c, "max_keys", 2) as u64;
    conf.schedule.split_merge_interval = format!("{}ns", number(c, "interval", 0)).parse().unwrap();
    conf.schedule.enable_one_way_merge = c["one_way"] == true;
    conf.store.coprocessor.region_max_size = "custom".into();
    conf.store.region_max_size_mb = number(c, "region_max", 144) as u64;
    conf.store.coprocessor.region_max_keys = number(c, "region_max_keys", 1440000);
    conf.store.coprocessor.region_split_keys = number(c, "region_split_keys", 960000);
    let stores = StoresInfo::new();
    for id in 1..=4 {
        stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        })));
    }
    let snapshots = stores.get_stores();
    let mut manager = RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        ClusterPaths::new(47),
    );
    manager
        .initialize(3, &[], "", false, &conf, &[])
        .await
        .unwrap();
    let rep = RegionReplicationContext {
        conf: &conf,
        stores: &snapshots,
        manager: RefCell::new(&mut manager),
        now,
    };
    let hot: BTreeSet<_> = c["hot"]
        .as_array()
        .map(|v| v.iter().map(|v| v.as_u64().unwrap()).collect())
        .unwrap_or_default();
    let source = region(
        1,
        b'b',
        number(c, "size", 2),
        number(c, "keys", 2),
        c["source_fault"].as_str().unwrap_or(""),
        false,
    );
    let prev = region(2, b'a', number(c, "prev_size", 3), 3, "", false);
    let next = region(
        3,
        b'c',
        number(c, "next_size", 3),
        number(c, "next_keys", 3),
        c["next_fault"].as_str().unwrap_or(""),
        c["next_new_peer"] == true,
    );
    if c["paused"] == true {
        checker.pause.pause_or_resume_at(1, 0);
    }
    let outcome = checker
        .check(
            MergeCheckContext {
                replication: &rep,
                stores: &stores,
                labels: None,
                hot_regions: &hot,
                supports_conf_change_v2: false,
            },
            &source,
            if c["prev_absent"] == true {
                None
            } else {
                Some(&prev)
            },
            if c["next_absent"] == true {
                None
            } else {
                Some(&next)
            },
            || {
                assert_eq!(c["next_new_peer"], true, "unexpected allocation");
                if c["allocation_fail"] == true {
                    Err(BuilderError::Allocation("injected".into()))
                } else {
                    Ok(999)
                }
            },
        )
        .await;
    if let Ok(Some(ops)) = &outcome {
        assert_eq!(ops[0].region_id(), 1);
        assert_eq!(ops[0].region_epoch(), source.meta().epoch.as_ref());
        for (i, op) in ops.iter().enumerate() {
            assert!(op.kind().0 & OpKind::MERGE.0 != 0);
            assert_eq!(op.priority_level(), PriorityLevel::MEDIUM);
            let OpStep::MergeRegion(s) = op.step(op.len() - 1).unwrap() else {
                panic!("missing merge")
            };
            assert_eq!(s.is_passive, i == 1);
            let evaluated = if i == 0 {
                &source
            } else if op.region_id() == 2 {
                &prev
            } else {
                &next
            };
            assert_eq!(s.conf_ver_changed(evaluated), 0);
            let command = s.get_cmd(evaluated, false);
            if i == 0 {
                assert_eq!(
                    command.unwrap().merge.unwrap().target.as_ref(),
                    s.to_region.as_ref()
                );
            } else {
                assert!(command.is_none());
            }
            let influence = OpInfluence::new();
            s.influence(&influence, evaluated);
            for peer in &evaluated.meta().peers {
                let delta = influence.get_store_influence(peer.store_id);
                assert_eq!(delta.region_count, if i == 0 { 0 } else { -1 });
                assert_eq!(delta.region_size, 0);
                assert_eq!(
                    delta.leader_count,
                    if i == 1
                        && evaluated
                            .leader()
                            .is_some_and(|leader| leader.store_id == peer.store_id)
                    {
                        -1
                    } else {
                        0
                    }
                );
            }
            assert_eq!(s.from_region.as_ref(), Some(source.meta()));
            assert_eq!(s.to_region.as_ref().unwrap().id, ops[1].region_id());
        }
        assert_eq!(ops[1].len(), 1);
        if c["next_new_peer"] == true {
            assert!(ops[0].len() > 1);
        } else {
            assert_eq!(ops[0].len(), 1);
        }
    }
    outcome
}
#[tokio::test]
async fn task_047_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_047.json"
    ))
    .unwrap();
    for c in fixture["cases"].as_array().unwrap() {
        let mut checker = MergeChecker::new(0, 0);
        let result = evaluate(&mut checker, c, 1).await;
        if c["error"] == true {
            assert_eq!(
                result.err(),
                Some(BuilderError::Allocation("injected".into())),
                "{}",
                c["name"]
            );
        } else {
            let id = result.unwrap().map(|o| o[1].region_id()).unwrap_or(0);
            assert_eq!(id, c["expected"].as_u64().unwrap(), "{}", c["name"]);
        }
    }
    println!(
        "task_047: {} frozen behavior cases",
        fixture["cases"].as_array().unwrap().len()
    );
}
#[tokio::test]
async fn task_047_split_cooldown() {
    let mut checker = MergeChecker::new(0, 10);
    let c = json!({"interval":10});
    assert!(evaluate(&mut checker, &c, 9).await.unwrap().is_none());
    assert!(evaluate(&mut checker, &c, 10).await.unwrap().is_some());
    checker.record_region_split(&[1, 3], 10, 10);
    assert!(evaluate(&mut checker, &c, 20).await.unwrap().is_none()); // TTL equality still exists
    assert!(evaluate(&mut checker, &c, 21).await.unwrap().is_some());
    checker.record_region_split(&[3], 21, 10);
    assert_eq!(
        evaluate(&mut checker, &c, 22).await.unwrap().unwrap()[1].region_id(),
        2
    );
    checker.record_region_split(&[2], 22, 10);
    assert!(evaluate(&mut checker, &c, 23).await.unwrap().is_none());
    let short = json!({"interval":2});
    assert!(evaluate(&mut checker, &short, 24).await.unwrap().is_none()); // reset both cache deadlines
    assert!(evaluate(&mut checker, &short, 26).await.unwrap().is_none());
    assert!(evaluate(&mut checker, &short, 27).await.unwrap().is_some());
    checker.record_region_split(&[1], 28, 100); // record doesn't change cache default
    assert!(evaluate(&mut checker, &short, 29).await.unwrap().is_none());
    let changed = json!({"interval":3});
    assert!(
        evaluate(&mut checker, &changed, 30)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        evaluate(&mut checker, &changed, 33)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        evaluate(&mut checker, &changed, 34)
            .await
            .unwrap()
            .is_some()
    );
    checker.record_region_split(&[1], 35, 0);
    let zero = json!({"interval":0});
    assert!(evaluate(&mut checker, &zero, 36).await.unwrap().is_none());
    assert!(evaluate(&mut checker, &zero, 37).await.unwrap().is_some());
    println!("task_047: 15 sequential cooldown assertions");
}

#[tokio::test]
async fn task_047_config_update_before_startup_gate() {
    let mut checker = MergeChecker::new(0, 10);
    checker.record_region_split(&[1, 1], 0, 100);
    let c = json!({"interval":2});
    assert!(evaluate(&mut checker, &c, 1).await.unwrap().is_none());
    assert!(evaluate(&mut checker, &c, 3).await.unwrap().is_none());
    assert!(evaluate(&mut checker, &c, 4).await.unwrap().is_some());
    checker.record_region_split(&[1], 5, 2);
    checker.record_region_split(&[1], 6, 2);
    assert!(evaluate(&mut checker, &c, 8).await.unwrap().is_none());
    assert!(evaluate(&mut checker, &c, 9).await.unwrap().is_some());
    checker.record_region_split(&[1], 10, 2);
    let extended = json!({"interval":10});
    assert!(
        evaluate(&mut checker, &extended, 11)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        evaluate(&mut checker, &extended, 21)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        evaluate(&mut checker, &extended, 22)
            .await
            .unwrap()
            .is_some()
    );
    println!("task_047: 8 startup/config/repeated-split assertions");
}
